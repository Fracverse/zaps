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
        return this.queue.add(payload.type, payload.data, {
            ...DEFAULT_OPTIONS,
            ...options,
        });
    }
}

export default new QueueService();
