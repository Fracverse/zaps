import { Queue, JobsOptions } from 'bullmq';
import { connection } from '../utils/redis';
import { workerConfig } from '../config/worker.config';
import type {
    EmailJobPayload,
    NotificationJobPayload,
    SyncJobPayload,
    BlockchainTxJobPayload,
    OnboardingJobPayload,
} from '../types/job-payloads';

export enum JobType {
    EMAIL = 'EMAIL',
    NOTIFICATION = 'NOTIFICATION',
    SYNC = 'SYNC',
    BLOCKCHAIN_TX = 'BLOCKCHAIN_TX',
    ONBOARDING = 'ONBOARDING',
}

export type JobPayload =
    | { type: JobType.EMAIL; data: EmailJobPayload }
    | { type: JobType.NOTIFICATION; data: NotificationJobPayload }
    | { type: JobType.SYNC; data: SyncJobPayload }
    | { type: JobType.BLOCKCHAIN_TX; data: BlockchainTxJobPayload }
    | { type: JobType.ONBOARDING; data: OnboardingJobPayload };

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
    private onboardingQueue: Queue;

    constructor() {
        this.emailQueue = new Queue('email-queue', { connection: connection as any });
        this.pushQueue = new Queue('push-queue', { connection: connection as any });
        this.syncQueue = new Queue('sync-queue', { connection: connection as any });
        this.blockchainTxQueue = new Queue('blockchain-tx-queue', { connection: connection as any });
        this.onboardingQueue = new Queue('onboarding-queue', { connection: connection as any });
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
            case JobType.ONBOARDING:
                return this.onboardingQueue.add(JobType.ONBOARDING, payload.data, finalOptions);
            default:
                throw new Error(`Unknown job type: ${(payload as JobPayload).type}`);
        }
    }
}

export default new QueueService();
