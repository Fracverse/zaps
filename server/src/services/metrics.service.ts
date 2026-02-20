import prisma from '../utils/prisma';
import redisClient from '../utils/redis';
import sorobanService from './soroban.service';

/**
 * Blueprint for Metrics & System Health.
 */
class MetricsService {
    /**
     * Skeletal blueprint for recording a request status.
     */
    recordRequest(status: number) {
        // Implementation: Increment request and error counters.
    }

    /**
     * Aggregates business KPIs for the admin dashboard.
     */
    async getDashboardStats() {
        const [totalUsers, totalMerchants, tvlAggregate] = await Promise.all([
            prisma.user.count(),
            prisma.merchant.count(),
            prisma.payment.aggregate({
                _sum: {
                    sendAmount: true
                },
                where: {
                    status: 'COMPLETED'
                }
            })
        ]);

        const tvl = tvlAggregate._sum.sendAmount 
            ? tvlAggregate._sum.sendAmount.toString() 
            : '0';

        return { 
            totalUsers, 
            totalMerchants, 
            tvl 
        };
    }

    /**
     * Unified health check for Database, Redis, and Soroban RPC connectivity.
     */
    async getSystemHealth() {
        // Check Database
        let database = 'disconnected';
        try {
            await prisma.$queryRaw`SELECT 1`;
            database = 'connected';
        } catch (error) {
            database = 'error';
        }

        // Check Redis
        let redis = 'disconnected';
        try {
            if (redisClient.status === 'ready') {
                redis = 'connected';
            } else {
                const pingResult = await redisClient.ping();
                redis = pingResult === 'PONG' ? 'connected' : 'error';
            }
        } catch (error) {
            redis = 'error';
        }

        // Check Soroban RPC
        let sorobanRpc = 'disconnected';
        try {
            await sorobanService.getLatestLedger();
            sorobanRpc = 'connected';
        } catch (error) {
            sorobanRpc = 'error';
        }

        const isHealthy = database === 'connected' && redis === 'connected' && sorobanRpc === 'connected';

        return {
            status: isHealthy ? 'healthy' : 'unhealthy',
            services: {
                database,
                redis,
                sorobanRpc
            }
        };
    }
}

export default new MetricsService();
