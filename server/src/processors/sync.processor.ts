import prisma from '../utils/prisma';
import logger from '../utils/logger';
import type { SyncJobPayload } from '../types/job-payloads';

export async function processSync(data: SyncJobPayload): Promise<void> {
    const { syncType, userId, resourceId, metadata } = data;
    const logCtx = { component: 'sync-processor', syncType, userId };

    logger.info('Processing sync job', logCtx);

    switch (syncType) {
        case 'user_data':
            if (!userId) {
                throw new Error('Sync user_data requires userId');
            }
            await syncUserData(userId, logCtx);
            break;
        case 'analytics':
            await syncAnalytics(logCtx);
            break;
        case 'backup':
            await syncBackup(logCtx);
            break;
        case 'on_chain_sync':
            await syncOnChain(resourceId, metadata, logCtx);
            break;
        default:
            logger.warn('Unknown sync type', { ...logCtx, syncType });
            throw new Error(`Unknown sync type: ${syncType}`);
    }

    logger.info('Sync job completed', logCtx);
}

async function syncUserData(userId: string, logCtx: Record<string, unknown>) {
    const user = await prisma.user.findUnique({
        where: { userId },
        include: { profile: true, balances: true },
    });
    if (!user) {
        logger.warn('User not found for sync', logCtx);
        return;
    }
    logger.debug('Synced user data', { ...logCtx, hasProfile: !!user.profile });
}

async function syncAnalytics(logCtx: Record<string, unknown>) {
    const [paymentCount, transferCount] = await Promise.all([
        prisma.payment.count(),
        prisma.transfer.count(),
    ]);
    logger.debug('Analytics sync completed', { ...logCtx, paymentCount, transferCount });
}

async function syncBackup(logCtx: Record<string, unknown>) {
    // Placeholder for backup/export logic
    logger.debug('Backup sync completed', logCtx);
}

async function syncOnChain(
    resourceId: string | undefined,
    metadata: Record<string, unknown> | undefined,
    logCtx: Record<string, unknown>
) {
    logger.debug('On-chain sync', { ...logCtx, resourceId, metadata });
}
