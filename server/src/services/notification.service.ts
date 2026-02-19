import prisma from '../utils/prisma';
import queueService, { JobType } from './queue.service';

/**
 * Skeletal Blueprint for the Notification Center.
 */
class NotificationService {
    /**
     * Orchestrates in-app creation and background delivery (Email/Push).
     */
    async createNotification(userId: string, title: string, message: string) {
        const notification = await prisma.notification.create({
            data: { userId, title, message },
        });

        // Blueprint: Delegate delivery to a background worker.
        await queueService.addJob({
            type: JobType.NOTIFICATION,
            data: { userId, title, message },
        });

        return notification;
    }

    /**
     * Lists unread notifications for a user.
     */
    async getNotifications(userId: string) {
        return prisma.notification.findMany({
            where: { userId },
            orderBy: { createdAt: 'desc' },
        });
    }
}

export default new NotificationService();
