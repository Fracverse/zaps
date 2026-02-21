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

    async getEvents(
        startLedger: number,
        filters?: { type: 'contract' | 'system' | 'diagnostic'; contractIds?: string[]; topics?: string[][] }[]
    ) {
        const defaultFilters = filters ?? [{ type: 'contract' as const }];
        return this.server.getEvents({
            startLedger,
            filters: defaultFilters.map((f) => ({
                type: f.type,
                contractIds: f.contractIds,
                topics: f.topics,
            })),
        });
    }
}

export default new SorobanService();
