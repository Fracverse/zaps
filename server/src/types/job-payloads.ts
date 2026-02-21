/**
 * Structured job payloads for QueueService.
 * Each job type has a strict interface for type-safe processing.
 */
import { JobType } from '../services/queue.service';

export interface EmailJobPayload {
    to: string;
    subject: string;
    body?: string;
    templateId?: string;
    templateData?: Record<string, string | number>;
}

export interface NotificationJobPayload {
    userId: string;
    title: string;
    message: string;
    type?: 'SYSTEM' | 'ACTION' | 'SECURITY';
    metadata?: Record<string, unknown>;
}

export type SyncType = 'user_data' | 'analytics' | 'backup' | 'on_chain_sync';

export interface SyncJobPayload {
    syncType: SyncType;
    userId?: string;
    resourceId?: string;
    metadata?: Record<string, unknown>;
}

export interface BlockchainTxJobPayload {
    network?: string;
    fromAddress: string;
    toAddress: string;
    amount: string;
    assetCode?: string;
    assetIssuer?: string;
    xdr?: string;
    paymentId?: string;
    transferId?: string;
}

export type TypedJobPayload =
    | { type: JobType.EMAIL; data: EmailJobPayload }
    | { type: JobType.NOTIFICATION; data: NotificationJobPayload }
    | { type: JobType.SYNC; data: SyncJobPayload }
    | { type: JobType.BLOCKCHAIN_TX; data: BlockchainTxJobPayload };
