import { Horizon } from '@stellar/stellar-sdk';
import config from '../config';
import logger from '../utils/logger';
import prisma from '../utils/prisma';
import { redis } from '../utils/redis';

const CURSOR_KEY = 'stellar:ledger:cursor';
const RECONNECT_DELAY_MS = 5_000;
const MAX_RECONNECT_DELAY_MS = 60_000;

/**
 * Monitors the Stellar Horizon ledger stream for relevant transactions.
 *
 * Design:
 *   - Streams ledger records via Horizon SSE, persisting the cursor in Redis
 *     so processing resumes from the correct ledger after a restart.
 *   - Each transaction is processed idempotently: if the stellar_tx_hash is
 *     already recorded the event is skipped.
 *   - Reconnects with exponential backoff when Horizon is unavailable.
 *   - Publishes `payment:received` and `payment:confirmed` events to Redis
 *     pub/sub so other services (webhook dispatcher, balance updater) react
 *     without polling.
 */
export class StellarMonitorService {
    private server: Horizon.Server;
    private stopStream: (() => void) | null = null;
    private running = false;
    private reconnectDelay = RECONNECT_DELAY_MS;

    constructor() {
        this.server = new Horizon.Server(config.stellar.horizonUrl, {
            allowHttp: config.stellar.horizonUrl.startsWith('http://'),
        });
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────

    async start(): Promise<void> {
        if (this.running) return;
        this.running = true;
        logger.info('[StellarMonitor] Starting ledger stream');
        await this.connect();
    }

    stop(): void {
        this.running = false;
        this.stopStream?.();
        this.stopStream = null;
        logger.info('[StellarMonitor] Stopped');
    }

    // ── Stream ─────────────────────────────────────────────────────────────

    private async connect(): Promise<void> {
        if (!this.running) return;

        const cursor = await this.loadCursor();
        logger.info('[StellarMonitor] Connecting', { cursor });

        try {
            const builder = this.server
                .transactions()
                .cursor(cursor)
                .limit(200)
                .order('asc');

            this.stopStream = builder.stream({
                onmessage: async (tx) => {
                    try {
                        await this.processTx(tx);
                        await this.saveCursor(tx.paging_token);
                        this.reconnectDelay = RECONNECT_DELAY_MS; // reset on success
                    } catch (err) {
                        logger.error('[StellarMonitor] Error processing tx', {
                            hash: (tx as any).hash,
                            err,
                        });
                    }
                },
                onerror: (err) => {
                    logger.warn('[StellarMonitor] Stream error', { err });
                    this.stopStream?.();
                    this.stopStream = null;
                    if (this.running) {
                        this.scheduleReconnect();
                    }
                },
            });
        } catch (err) {
            logger.error('[StellarMonitor] Failed to open stream', { err });
            if (this.running) {
                this.scheduleReconnect();
            }
        }
    }

    private scheduleReconnect(): void {
        const delay = this.reconnectDelay;
        logger.info('[StellarMonitor] Reconnecting', { delayMs: delay });
        this.reconnectDelay = Math.min(delay * 2, MAX_RECONNECT_DELAY_MS);
        setTimeout(() => void this.connect(), delay);
    }

    // ── Transaction processing ─────────────────────────────────────────────

    private async processTx(tx: Horizon.ServerApi.TransactionRecord): Promise<void> {
        const hash = tx.hash;

        // Idempotency: skip if already processed
        const alreadyProcessed = await redis.sismember('stellar:processed_txs', hash);
        if (alreadyProcessed) return;

        // Only process successful transactions
        if (!tx.successful) {
            await redis.sadd('stellar:processed_txs', hash);
            return;
        }

        logger.debug('[StellarMonitor] Processing tx', { hash });

        // Fetch the full operation list to identify payment operations
        const ops = await tx.operations();
        for (const op of ops.records) {
            if (op.type === 'payment') {
                await this.handlePaymentOp(op as Horizon.ServerApi.PaymentOperationRecord, hash);
            } else if (op.type === 'invoke_host_function') {
                await this.handleContractOp(op, hash);
            }
        }

        // Mark as processed and publish notification
        await redis.sadd('stellar:processed_txs', hash);
        await redis
            .pipeline()
            .expire('stellar:processed_txs', 7 * 24 * 3600) // 7-day TTL
            .publish('payment:confirmed', JSON.stringify({ hash, ledger: tx.ledger_attr }))
            .exec();
    }

    private async handlePaymentOp(
        op: Horizon.ServerApi.PaymentOperationRecord,
        txHash: string,
    ): Promise<void> {
        const { to, amount, asset_type, asset_code } = op;
        const assetCode = asset_type === 'native' ? 'XLM' : (asset_code ?? 'UNKNOWN');

        // Check if the recipient is a known user or merchant address
        const user = await prisma.user.findFirst({
            where: { stellarAddress: to },
            select: { id: true, stellarAddress: true },
        });

        if (!user) return; // Not our user — ignore

        logger.info('[StellarMonitor] Incoming payment detected', {
            to,
            amount,
            asset: assetCode,
            txHash,
        });

        // Record the inbound transaction
        await prisma.transaction.upsert({
            where: { stellarTxHash: txHash },
            update: {},
            create: {
                userId: user.id,
                stellarTxHash: txHash,
                type: 'RECEIVE',
                amount: Math.round(parseFloat(amount) * 1e7).toString(), // stroops
                asset: assetCode,
                status: 'CONFIRMED',
            },
        });

        // Notify downstream services via Redis pub/sub
        await redis.publish(
            'payment:received',
            JSON.stringify({ userId: user.id, amount, asset: assetCode, txHash }),
        );
    }

    private async handleContractOp(op: any, txHash: string): Promise<void> {
        // Contract invocations (Soroban) are logged for further processing
        // by the EventMonitoringService which handles PAY_DONE and similar events.
        logger.debug('[StellarMonitor] Contract invocation detected', { txHash, op: op.type });
    }

    // ── Cursor persistence ─────────────────────────────────────────────────

    private async loadCursor(): Promise<string> {
        const cursor = await redis.get(CURSOR_KEY);
        return cursor ?? 'now';
    }

    private async saveCursor(pagingToken: string): Promise<void> {
        await redis.set(CURSOR_KEY, pagingToken);
    }
}

export default new StellarMonitorService();
