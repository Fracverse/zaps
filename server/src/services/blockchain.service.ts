import { Horizon, TransactionBuilder, Networks, Transaction } from '@stellar/stellar-sdk';
import config from '../config';
import logger from '../utils/logger';
import { ApiError } from '../middleware/error.middleware';

const networkPassphrase =
    config.stellar.network === 'TESTNET' ? Networks.TESTNET : Networks.PUBLIC;

export class BlockchainService {
    private server: Horizon.Server;

    constructor() {
        this.server = new Horizon.Server(config.stellar.horizonUrl);
    }

    async getAccount(address: string) {
        return this.server.loadAccount(address);
    }

    async submitTransaction(txXdr: string) {
        const tx = TransactionBuilder.fromXDR(txXdr, networkPassphrase) as Transaction;
        try {
            const result = await this.server.submitTransaction(tx);
            logger.info('Transaction submitted via Horizon', { hash: result.hash });
            return result;
        } catch (err: any) {
            const extras = err?.response?.data?.extras;
            logger.error('Horizon submission failed', { extras });
            throw new ApiError(400, 'Transaction submission failed', 'HORIZON_SUBMIT_FAILED');
        }
    }
}

export default new BlockchainService();
