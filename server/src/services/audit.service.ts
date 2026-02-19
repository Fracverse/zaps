import prisma from '../utils/prisma';
import logger from '../utils/logger';

/**
 * Skeletal Blueprint for Audit Logging.
 */
class AuditService {
    /**
     * Durably records a system action.
     */
    async log(data: any) {
        logger.info(`Skeletal Audit: Logging action ${data.action} by ${data.actorId}`);

        return prisma.auditLog.create({
            data: {
                actorId: data.actorId,
                action: data.action,
                resource: data.resource,
                metadata: data.metadata || {},
                ipAddress: data.ipAddress,
                userAgent: data.userAgent,
            },
        });
    }

    /**
     * Retrieves logs for admin oversight.
     */
    async getLogs(actorId?: string, limit: number = 50) {
        return prisma.auditLog.findMany({
            where: actorId ? { actorId } : {},
            take: limit,
            orderBy: { timestamp: 'desc' },
        });
    }
}

export default new AuditService();
