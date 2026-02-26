import { Queue, JobsOptions } from 'bullmq';
import { connection } from '../utils/redis';
import { workerConfig } from '../config/worker.config';
import type {
    EmailJobPayload,
    NotificationJobPayload,
    SyncJobPayload,
    BlockchainTxJobPayload,
} from '../types/job-payloads';

export enum JobType {
    EMAIL = 'EMAIL',
    NOTIFICATION = 'NOTIFICATION',
    SYNC = 'SYNC',
    BLOCKCHAIN_TX = 'BLOCKCHAIN_TX',
}

export type JobPayload =
    | { type: JobType.EMAIL; data: EmailJobPayload }
    | { type: JobType.NOTIFICATION; data: NotificationJobPayload }
    | { type: JobType.SYNC; data: SyncJobPayload }
    | { type: JobType.BLOCKCHAIN_TX; data: BlockchainTxJobPayload };

const DEFAULT_OPTIONS: JobsOptions = {
    attempts: workerConfig.maxRetries,
    backoff: {
        type: workerConfig.backoff.type,
        delay: workerConfig.backoff.delay,
    },
    removeOnComplete: { count: 1000 },
    removeOnFail: false,
};

class QueueService {
    private emailQueue: Queue;
    private pushQueue: Queue;
    private syncQueue: Queue;
    private blockchainTxQueue: Queue;

    constructor() {
        this.emailQueue = new Queue('email-queue', { connection: connection as any });
        this.pushQueue = new Queue('push-queue', { connection: connection as any });
        this.syncQueue = new Queue('sync-queue', { connection: connection as any });
        this.blockchainTxQueue = new Queue('blockchain-tx-queue', { connection: connection as any });
    }

    public getEmailQueue(): Queue {
        return this.emailQueue;
    }

    public getPushQueue(): Queue {
        return this.pushQueue;
    }

    public getSyncQueue(): Queue {
        return this.syncQueue;
    }

    public getBlockchainTxQueue(): Queue {
        return this.blockchainTxQueue;
    private queue: Queue;

    constructor() {
        this.queue = new Queue(workerConfig.defaultQueue, {
            connection: connection as any,
            defaultJobOptions: DEFAULT_OPTIONS,
        });
    }

    public getQueue(): Queue {
        return this.queue;
    }

    public async addEmailJob(data: EmailJobPayload, options?: JobsOptions) {
        return this.queue.add(JobType.EMAIL, data, {
            ...DEFAULT_OPTIONS,
            ...options,
            jobId: options?.jobId,
        });
    }

    public async addNotificationJob(data: NotificationJobPayload, options?: JobsOptions) {
        return this.queue.add(JobType.NOTIFICATION, data, {
            ...DEFAULT_OPTIONS,
            ...options,
            jobId: options?.jobId,
        });
    }

    public async addSyncJob(data: SyncJobPayload, options?: JobsOptions) {
        return this.queue.add(JobType.SYNC, data, {
            ...DEFAULT_OPTIONS,
            ...options,
            jobId: options?.jobId,
        });
    }

    public async addBlockchainTxJob(data: BlockchainTxJobPayload, options?: JobsOptions) {
        return this.queue.add(JobType.BLOCKCHAIN_TX, data, {
            ...DEFAULT_OPTIONS,
            ...options,
            jobId: options?.jobId,
        });
    }

    /** Generic add for backwards compatibility - validates payload structure */
    public async addJob(payload: JobPayload, options?: JobsOptions) {
        const defaultOptions: JobsOptions = {
            attempts: 5,
            backoff: {
                type: 'exponential',
                delay: 1000,
            },
            removeOnComplete: true, // Auto-remove completed jobs to save Redis space
            removeOnFail: false,    // Keep failed jobs for inspection
        };
        const finalOptions = { ...defaultOptions, ...options };

        switch (payload.type) {
            case JobType.EMAIL:
                return this.emailQueue.add(JobType.EMAIL, payload.data, finalOptions);
            case JobType.NOTIFICATION:
                return this.pushQueue.add(JobType.NOTIFICATION, payload.data, finalOptions);
            case JobType.SYNC:
                return this.syncQueue.add(JobType.SYNC, payload.data, finalOptions);
            case JobType.BLOCKCHAIN_TX:
                return this.blockchainTxQueue.add(JobType.BLOCKCHAIN_TX, payload.data, finalOptions);
            default:
                throw new Error(`Unknown job type: ${payload.type}`);
        }
        return this.queue.add(payload.type, payload.data, {
            ...DEFAULT_OPTIONS,
            ...options,
        });
    }
}

export default new QueueService();
