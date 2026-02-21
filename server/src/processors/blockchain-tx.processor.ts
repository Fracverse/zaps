import sorobanService from '../services/soroban.service';
import prisma from '../utils/prisma';
import { PaymentStatus } from '@prisma/client';
import { TransferStatus } from '@prisma/client';
import logger from '../utils/logger';
import type { BlockchainTxJobPayload } from '../types/job-payloads';

export async function processBlockchainTx(data: BlockchainTxJobPayload): Promise<void> {
    const { fromAddress, toAddress, amount, xdr, paymentId, transferId } = data;
    const logCtx = {
        component: 'blockchain-tx-processor',
        fromAddress,
        toAddress,
        paymentId,
    };

    logger.info('Processing blockchain tx job', logCtx);

    if (!fromAddress || !toAddress) {
        throw new Error('Invalid blockchain tx payload: missing fromAddress or toAddress');
    }

    try {
        if (xdr) {
            const simulated = await sorobanService.simulateTransaction(xdr);
            logger.debug('Transaction simulated', { ...logCtx, simulated });
        }

        // Submit XDR to network (via Horizon / Soroban RPC)
        // const result = await horizon.submitTransaction(xdr);
        const txHash = `tx_${Date.now()}_${Math.random().toString(36).slice(2, 10)}`;

        if (paymentId) {
            await prisma.payment.updateMany({
                where: { id: paymentId },
                data: { txHash, status: PaymentStatus.PROCESSING },
            });
        }
        if (transferId) {
            await prisma.transfer.updateMany({
                where: { id: transferId },
                data: { txHash, status: TransferStatus.PROCESSING },
            });
        }

        logger.info('Blockchain tx processed', { ...logCtx, txHash });
    } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        logger.error('Blockchain tx failed', { ...logCtx, error: msg });
        throw err;
    }
}
