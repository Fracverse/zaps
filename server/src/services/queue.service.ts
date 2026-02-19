import { Queue, JobsOptions } from 'bullmq';
import { connection } from '../utils/redis';

export enum JobType {
    EMAIL = 'EMAIL',
    NOTIFICATION = 'NOTIFICATION',
    SYNC = 'SYNC',
    BLOCKCHAIN_TX = 'BLOCKCHAIN_TX',
}

export interface JobPayload {
    type: JobType;
    data: any;
}

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
    }

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
    }
}

export default new QueueService();
