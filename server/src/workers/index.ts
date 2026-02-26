import { Worker } from 'bullmq';
import { connection } from '../utils/redis';
import queueService, { JobType } from '../services/queue.service';
import { JobType } from '../services/queue.service';
import { workerConfig } from '../config/worker.config';
import { getProcessor } from '../processors';
import logger from '../utils/logger';
import prisma from '../utils/prisma';
import { PaymentStatus } from '@prisma/client';

export const startWorkers = () => {
    // Email Worker
    new Worker('email-queue', async (job: Job) => {
        logger.info(`Processing EMAIL job ${job.id}`);
        await processEmail(job.data);
    }, { connection: connection as any, concurrency: 5 });

    // Push Notification Worker
    new Worker('push-queue', async (job: Job) => {
        logger.info(`Processing PUSH job ${job.id}`);
        await processNotification(job.data);
    }, { connection: connection as any, concurrency: 5 });

    // Sync Worker
    new Worker('sync-queue', async (job: Job) => {
        logger.info(`Processing SYNC job ${job.id}`);
        await processSync(job.data);
    }, { connection: connection as any, concurrency: 1 }); // Sequential processing for sync might be safer or just 1 for now

    // Blockchain Tx Worker
    new Worker('blockchain-tx-queue', async (job: Job) => {
        logger.info(`Processing BLOCKCHAIN_TX job ${job.id}`);
        await processBlockchainTx(job.data);
    }, { connection: connection as any, concurrency: 5 });

    logger.info('Background workers started for all queues...');
};
let worker: Worker | null = null;

export function startWorkers(): Worker {
    if (worker) {
        logger.warn('Workers already started', { component: 'worker' });
        return worker;
    }

    worker = new Worker(
        workerConfig.defaultQueue,
        async (job) => {
            const { id, name, data, attemptsMade } = job;
            const logCtx = { component: 'worker', jobId: id, jobType: name, attempt: attemptsMade + 1 };

            logger.info('Processing job', logCtx);

            const processor = getProcessor(name as JobType);
            if (!processor) {
                logger.warn('Unknown job type, skipping', { ...logCtx, jobType: name });
                return;
            }

            try {
                await processor(data);
                logger.info('Job completed', logCtx);
            } catch (err) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error('Job processing failed', { ...logCtx, error: msg });
                throw err;
            }
        },
        {
            connection: connection as any,
            concurrency: workerConfig.concurrency,
            limiter: {
                max: 50,
                duration: 1000,
            },
            lockDuration: workerConfig.lockDuration,
            stalledInterval: workerConfig.stalledInterval,
        }
    );

    worker.on('completed', (job) => {
        logger.debug('Job completed', { component: 'worker', jobId: job.id, jobType: job.name });
    });

    worker.on('failed', (job, err) => {
        logger.error('Job failed', {
            component: 'worker',
            jobId: job?.id,
            jobType: job?.name,
            error: err?.message ?? String(err),
            attemptsMade: job?.attemptsMade,
        });
    });

    worker.on('error', (err) => {
        logger.error('Worker error', { component: 'worker', error: err.message });
    });

    logger.info('Background workers started', {
        component: 'worker',
        concurrency: workerConfig.concurrency,
        queue: workerConfig.defaultQueue,
    });

    return worker;
}

export async function stopWorkers(): Promise<void> {
    if (worker) {
        logger.info('Stopping workers gracefully', { component: 'worker' });
        await worker.close();
        worker = null;
        logger.info('Workers stopped', { component: 'worker' });
    }
}

const processSync = async (data: any) => {
    logger.info('Processing SYNC job', { data });

    if (data.syncType === 'ON_CHAIN_COMPLETION' && (data.eventType === 'PAY_DONE' || data.eventType === 'TRANSFER_DONE')) {
        const { paymentId } = data;

        if (!paymentId) return;

        const payment = await prisma.payment.findUnique({ where: { id: paymentId } });

        if (!payment) {
            logger.warn(`Payment not found for sync: ${paymentId}`);
            return;
        }

        if (payment.status === PaymentStatus.COMPLETED) {
            logger.info(`Payment ${paymentId} already completed. Skipping.`);
            return;
        }

        await prisma.payment.update({
            where: { id: paymentId },
            data: { status: PaymentStatus.COMPLETED },
        });
        logger.info(`Payment ${paymentId} marked as COMPLETED.`);

        // Dispatch downstream jobs
        await queueService.addJob({
            type: JobType.EMAIL,
            data: {
                to: 'user@example.com', // Placeholder
                subject: 'Payment Completed',
                paymentId,
                amount: payment.sendAmount.toString()
            }
        });

        if (payment.userAddress) {
            await queueService.addJob({
                type: JobType.NOTIFICATION,
                data: {
                    userId: payment.userAddress,
                    title: 'Payment Completed',
                    paymentId
                }
            });
        }
    }
};

export function getWorker(): Worker | null {
    return worker;
}
