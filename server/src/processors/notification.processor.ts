import prisma from '../utils/prisma';
import logger from '../utils/logger';
import type { NotificationJobPayload } from '../types/job-payloads';
import { NotificationType } from '@prisma/client';

export async function processNotification(data: NotificationJobPayload): Promise<void> {
    const { userId, title, message, type = 'SYSTEM', metadata } = data;
    const logCtx = { component: 'notification-processor', userId, title };

    logger.info('Processing notification job', logCtx);

    if (!userId || typeof userId !== 'string') {
        logger.error('Notification job failed: missing or invalid "userId"', logCtx);
        throw new Error('Invalid notification payload: missing "userId"');
    }

    try {
        const user = await prisma.user.findUnique({ where: { userId } });
        if (!user) {
            logger.warn('Notification target user not found', logCtx);
            return;
        }

        const notifType = type === 'ACTION' ? NotificationType.ACTION : type === 'SECURITY' ? NotificationType.SECURITY : NotificationType.SYSTEM;
        await prisma.notification.create({
            data: {
                userId: user.userId,
                title,
                message,
                type: notifType,
                metadata: metadata != null ? (metadata as object) : undefined,
            },
        });

        logger.info('Notification created in DB', logCtx);

        // FCM / OneSignal push integration would go here
        // await fcm.send(user.fcmToken, { title, body: message });
    } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        logger.error('Notification processing failed', { ...logCtx, error: msg });
        throw err;
    }
}
