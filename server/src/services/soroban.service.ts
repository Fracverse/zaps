import {
    rpc,
    TransactionBuilder,
    Networks,
    Keypair,
    xdr,
    Transaction,
    SorobanRpc,
    Account,
    Contract,
    BASE_FEE,
} from '@stellar/stellar-sdk';
import config from '../config';
import logger from '../utils/logger';
import { ApiError } from '../middleware/error.middleware';

const networkPassphrase =
    config.stellar.network === 'TESTNET' ? Networks.TESTNET : Networks.PUBLIC;

class SorobanService {
    private server: rpc.Server;

    constructor() {
        this.server = new rpc.Server(config.stellar.rpcUrl);
    }

    /**
     * Returns the fee payer Keypair.
     * Throws if FEE_PAYER_SECRET is not configured.
     */
    private getFeePayerKeypair(): Keypair {
        const secret = config.stellar.feePayerSecret;
        if (!secret) {
            throw new ApiError(500, 'FEE_PAYER_SECRET is not configured', 'FEE_PAYER_MISSING');
        }
        return Keypair.fromSecret(secret);
    }

    async getLatestLedger() {
        const info = await this.server.getLatestLedger();
        return info.sequence;
    }

    /**
     * Loads the current account state (sequence number) from the RPC server.
     */
    async getAccount(address: string): Promise<Account> {
        return this.server.getAccount(address);
    }

    /**
     * Simulates a transaction and returns the prepared (assembled) transaction
     * with resource footprints, CPU/memory instructions, and fees populated.
     */
    async simulateTransaction(txXdr: string): Promise<{
        prepared: Transaction;
        minResourceFee: string;
        footprint: xdr.SorobanTransactionData | null;
    }> {
        const tx = TransactionBuilder.fromXDR(txXdr, networkPassphrase) as Transaction;
        const simResponse = await this.server.simulateTransaction(tx);

        if (SorobanRpc.Api.isSimulationError(simResponse)) {
            logger.error('Transaction simulation failed', { error: simResponse.error });
            throw new ApiError(400, `Simulation failed: ${simResponse.error}`, 'SIMULATION_FAILED');
        }

        if (!SorobanRpc.Api.isSimulationSuccess(simResponse)) {
            throw new ApiError(400, 'Simulation returned unexpected state', 'SIMULATION_UNEXPECTED');
        }

        // SorobanRpc.assembleTransaction merges resource footprints, auth entries,
        // and the min resource fee into the transaction envelope.
        const assembled = SorobanRpc.assembleTransaction(tx, simResponse).build();

        return {
            prepared: assembled as Transaction,
            minResourceFee: simResponse.minResourceFee,
            footprint: simResponse.transactionData?.build() ?? null,
        };
    }

    /**
     * Sponsors a transaction by rebuilding it with the fee payer as the
     * transaction source (fee source). The user remains the invoker (their
     * address is on the Soroban auth entries / operations) but the server
     * pays XLM fees.
     *
     * Flow:
     *   1. Deserialize the client-built XDR.
     *   2. Simulate to get footprints & resource fees.
     *   3. Rebuild with the fee payer account as the outer envelope source.
     *   4. Sign only the outer envelope with FEE_PAYER_SECRET.
     *   5. Return the half-signed XDR — client adds their signature for auth.
     *
     * The server NEVER touches user private keys.
     */
    async sponsorTransaction(txXdr: string): Promise<{
        sponsoredXdr: string;
        feePayerAddress: string;
        networkPassphrase: string;
    }> {
        const feePayer = this.getFeePayerKeypair();
        const feePayerAddress = feePayer.publicKey();

        // 1. Deserialize the original transaction
        const originalTx = TransactionBuilder.fromXDR(txXdr, networkPassphrase) as Transaction;

        // 2. Simulate to get footprints and resource requirements
        const { prepared } = await this.simulateTransaction(txXdr);

        // 3. Rebuild the transaction with the fee payer as the envelope source.
        //    The operations (and Soroban auth entries) still reference the
        //    user's address — we only swap the *fee source*.
        const feePayerAccount = await this.getAccount(feePayerAddress);

        const rebuiltTx = new TransactionBuilder(feePayerAccount, {
            fee: prepared.fee,
            networkPassphrase,
        });

        // Copy all operations from the prepared (simulated) transaction
        for (const op of prepared.operations) {
            rebuiltTx.addOperation(op);
        }

        // Copy the Soroban transaction data (footprint, resource limits)
        const sorobanData = prepared.toEnvelope().v1().tx().ext().sorobanData();
        if (sorobanData) {
            rebuiltTx.setSorobanData(sorobanData);
        }

        rebuiltTx.setTimeout(300); // 5-minute validity window

        const finalTx = rebuiltTx.build();

        // 4. Sign the outer envelope with the fee payer key ONLY
        finalTx.sign(feePayer);

        logger.info('Transaction sponsored', {
            feePayerAddress,
            userSource: originalTx.source,
            fee: finalTx.fee,
            operations: finalTx.operations.length,
        });

        // 5. Return — client must add their auth signature before submitting
        return {
            sponsoredXdr: finalTx.toXDR(),
            feePayerAddress,
            networkPassphrase,
        };
    }

    /**
     * Builds a Soroban contract invocation transaction.
     * Returns unsigned XDR ready for simulation and sponsorship.
     */
    async buildContractCall(
        sourceAddress: string,
        contractId: string,
        method: string,
        args: xdr.ScVal[],
    ): Promise<string> {
        const contract = new Contract(contractId);

        // Use a temporary account with sequence "0" for blueprint building.
        // The real sequence is set during sponsorship when the fee payer
        // becomes the envelope source.
        const source = new Account(sourceAddress, '0');

        const tx = new TransactionBuilder(source, {
            fee: BASE_FEE,
            networkPassphrase,
        })
            .addOperation(contract.call(method, ...args))
            .setTimeout(300)
            .build();

        return tx.toXDR();
    }

    /**
     * Submits a fully-signed transaction (user + fee payer signatures)
     * and polls until the RPC confirms inclusion.
     */
    async submitTransaction(txXdr: string): Promise<SorobanRpc.Api.GetTransactionResponse> {
        const tx = TransactionBuilder.fromXDR(txXdr, networkPassphrase) as Transaction;
        const sendResponse = await this.server.sendTransaction(tx);

        if (sendResponse.status === 'ERROR') {
            logger.error('Transaction send failed', { errorResult: sendResponse.errorResult });
            throw new ApiError(400, 'Transaction rejected by network', 'TX_SEND_FAILED');
        }

        // Poll for finality
        let getResponse = await this.server.getTransaction(sendResponse.hash);
        while (getResponse.status === SorobanRpc.Api.GetTransactionStatus.NOT_FOUND) {
            await new Promise((resolve) => setTimeout(resolve, 1000));
            getResponse = await this.server.getTransaction(sendResponse.hash);
        }

        if (getResponse.status === SorobanRpc.Api.GetTransactionStatus.FAILED) {
            logger.error('Transaction failed on-chain', { hash: sendResponse.hash });
            throw new ApiError(400, 'Transaction failed on-chain', 'TX_FAILED');
        }

        logger.info('Transaction confirmed', { hash: sendResponse.hash });
        return getResponse;
    }

    async getEvents(startLedger: number) {
        return this.server.getEvents({
            startLedger,
            filters: [
                {
                    type: 'contract',
                    contractIds: config.stellar.paymentRouterContract
                        ? [config.stellar.paymentRouterContract]
                        : [],
                },
            ],
        });
    }
}

export default new SorobanService();
