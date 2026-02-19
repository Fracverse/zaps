import { Worker, Job } from 'bullmq';
import { connection } from '../utils/redis';
import queueService, { JobType } from '../services/queue.service';
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

const processEmail = async (data: any) => {
    // Integration with SendGrid/AWS SES would go here
    logger.info('Sending email to:', { to: data.to, subject: data.subject });
};

const processNotification = async (data: any) => {
    // Integration with FCM/OneSignal would go here
    logger.info('Sending push notification to user:', { userId: data.userId, title: data.title });
};

const processBlockchainTx = async (data: any) => {
    // Logic to submit XDR to Stellar network and monitor status
    logger.info('Submitting blockchain transaction...');
};

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

