/**
 * JobProcessorRegistry - maps JobType to processor functions.
 * Manages background job processing and provides a central registry.
 */
import { JobType } from '../services/queue.service';
import { processEmail } from './email.processor';
import { processNotification } from './notification.processor';
import { processSync } from './sync.processor';
import { processBlockchainTx } from './blockchain-tx.processor';
import type {
    EmailJobPayload,
    NotificationJobPayload,
    SyncJobPayload,
    BlockchainTxJobPayload,
} from '../types/job-payloads';

export type ProcessorFn = (data: unknown) => Promise<void>;

const registry: Map<JobType, ProcessorFn> = new Map([
    [JobType.EMAIL, (d) => processEmail(d as EmailJobPayload)],
    [JobType.NOTIFICATION, (d) => processNotification(d as NotificationJobPayload)],
    [JobType.SYNC, (d) => processSync(d as SyncJobPayload)],
    [JobType.BLOCKCHAIN_TX, (d) => processBlockchainTx(d as BlockchainTxJobPayload)],
]);

export function getProcessor(jobType: JobType): ProcessorFn | undefined {
    return registry.get(jobType);
}

export function getAllJobTypes(): JobType[] {
    return Array.from(registry.keys());
}

export { processEmail, processNotification, processSync, processBlockchainTx };
