/**
 * Worker configuration for BullMQ job processors.
 * Controls concurrency, retries, and graceful shutdown.
 */
export const workerConfig = {
    defaultQueue: 'zaps:jobs',
    concurrency: parseInt(process.env.WORKER_CONCURRENCY || '5', 10),
    maxRetries: parseInt(process.env.JOB_MAX_RETRIES || '3', 10),
    backoff: {
        type: 'exponential' as const,
        delay: parseInt(process.env.JOB_BACKOFF_DELAY_MS || '1000', 10),
    },
    stalledInterval: 30000,
    lockDuration: 300000, // 5 min
    lockRenewTime: 150000, // 2.5 min (renew before expiry)
};
