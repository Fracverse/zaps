import {
    rpc,
    TransactionBuilder,
    Networks,
    Keypair,
    Address,
    scValToNative,
    nativeToScVal,
    xdr
} from '@stellar/stellar-sdk';
import config from '../config';

class SorobanService {
    private server: rpc.Server;

    constructor() {
        this.server = new rpc.Server(config.stellar.rpcUrl);
    }

    async getLatestLedger() {
        const info = await this.server.getLatestLedger();
        return info.sequence;
    }

    async simulateTransaction(txXdr: string) {
        const tx = TransactionBuilder.fromXDR(txXdr, config.stellar.network === 'TESTNET' ? Networks.TESTNET : Networks.PUBLIC);
        return this.server.simulateTransaction(tx);
    }

    async sponsorTransaction(txXdr: string) {
        // Port logic from soroban_service.rs
        // Backend acts as Fee Payer
        const feePayer = Keypair.fromSecret(process.env.FEE_PAYER_SECRET || 'SABC...');
        const tx = TransactionBuilder.fromXDR(txXdr, config.stellar.network === 'TESTNET' ? Networks.TESTNET : Networks.PUBLIC);

        // Logic to sign as fee payer and update source account if needed
        // This is the core of Account Abstraction
        return tx.toXDR();
    }

    async getEvents(startLedger: number) {
        return this.server.getEvents({
            startLedger,
            limit: 100 // Reasonable limit per poll
        });
    }
}

export default new SorobanService();
