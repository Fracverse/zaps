import sorobanService from './soroban.service';
import prisma from '../utils/prisma';
import logger from '../utils/logger';

interface SorobanEvent {
    id?: string;
    ledger?: string;
    type?: string;
    contractId?: string;
    topic?: string[];
    value?: Record<string, unknown>;
}

class EventBridgeService {
    private isRunning = false;
    private lastLedger = 0;
    private pollHandle: ReturnType<typeof setInterval> | null = null;

    async start() {
        if (this.isRunning) return;
        this.isRunning = true;
        logger.info('EventBridgeService started', { component: 'event-bridge' });

        if (this.lastLedger === 0) {
            try {
                this.lastLedger = await sorobanService.getLatestLedger();
                logger.info('EventBridge initialized at ledger', {
                    component: 'event-bridge',
                    lastLedger: this.lastLedger,
                });
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error('Failed to initialize EventBridge ledger', {
                    component: 'event-bridge',
                    error: msg,
                });
                this.lastLedger = 1;
            }
        }

        this.schedulePoll();
    }

    private schedulePoll() {
        const poll = async () => {
            if (!this.isRunning) return;
            try {
                await this.poll();
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error('EventBridge poll error', {
                    component: 'event-bridge',
                    error: msg,
                });
                await this.delay(eventBridgeConfig.errorBackoffMs);
            }
            if (this.isRunning) {
                this.schedulePoll();
            }
        };
        this.pollHandle = setTimeout(
            poll,
            eventBridgeConfig.pollIntervalMs,
        ) as unknown as ReturnType<typeof setInterval>;
    }

    private delay(ms: number) {
        return new Promise((r) => setTimeout(r, ms));
    }

    private async poll() {
        const filters: { type: 'contract'; contractIds?: string[] }[] = [
            { type: 'contract' },
        ];
        if (eventBridgeConfig.contractIds.length > 0) {
            filters[0].contractIds = eventBridgeConfig.contractIds;
        }

        const startLedger = this.lastLedger;
        const eventsResponse = await sorobanService.getEvents(startLedger, filters);
        const rawEvents = (eventsResponse as unknown as { events?: Array<{ topic?: unknown[]; [k: string]: unknown }> }).events ?? [];
        const events = rawEvents.map((e) => {
            const topic = extractTopicStrings(e.topic);
            return {
                ...e,
                ledger: e.ledger != null ? String(e.ledger) : undefined,
                topic,
                value: (typeof e.value === 'object' && e.value !== null ? e.value : {}) as Record<string, unknown>,
            } as SorobanEvent;
        });

        const seenIds = new Set<string>();

        for (const event of events) {
            const id = event.id ?? `${event.ledger}-${event.contractId}-${JSON.stringify(event.topic)}`;
            if (seenIds.has(id)) continue;
            seenIds.add(id);

            await this.processEvent(event);
        }

        if (events.length > 0) {
            const maxLedger = events.reduce((acc, e) => {
                const seq = parseInt(String(e.ledger ?? 0), 10);
                return Math.max(acc, seq);
            }, 0);
            this.lastLedger = maxLedger + 1;
        }

        await this.delay(eventBridgeConfig.pollIntervalMs);
    }

    private async processEvent(event: SorobanEvent) {
        const topic0 = event.topic?.[0];
        const topic1 = event.topic?.[1];
        const value = event.value ?? {};

        try {
            // Port logic from indexer_service.rs
            // Example: Handle PAY_DONE event (this depends on the contract event schema)
            if (event.type === 'contract' && event.topic?.[0] === 'PAY_DONE') {
                const paymentId = event.value?.paymentId;
                if (!paymentId) return;

                await prisma.payment.update({
                    where: { id: paymentId },
                    data: { status: 'COMPLETED' },
                });
                logger.info(`Payment ${paymentId} completed on-chain via Event Bridge`);
            }
        } else {
            logger.warn('PaymentSettled: no matching pending payment', { merchantId, payer });
        }
    }

    private async handlePaymentFailed(event: SorobanEvent, value: Record<string, unknown>) {
        const payer = value.payer as string | undefined;
        const merchantIdBytes = value.merchant_id;
        const merchantId = this.decodeMerchantId(merchantIdBytes);
        if (!merchantId) return;

        const payment = await prisma.payment.findFirst({
            where: {
                merchantId,
                fromAddress: payer ?? undefined,
                status: { in: [PaymentStatus.PENDING, PaymentStatus.PROCESSING] },
            },
            orderBy: { createdAt: 'desc' },
        });

        if (payment) {
            await prisma.payment.update({
                where: { id: payment.id },
                data: { status: PaymentStatus.FAILED },
            });
            logger.info('Payment failed on-chain via EventBridge', {
                component: 'event-bridge',
                paymentId: payment.id,
            });
        }
    }

    private async handlePaymentInitiated(_event: SorobanEvent, value: Record<string, unknown>) {
        const payer = value.payer as string | undefined;
        const merchantIdBytes = value.merchant_id;
        const merchantId = this.decodeMerchantId(merchantIdBytes);
        if (!merchantId) return;
        logger.debug('PaymentInitiated event', {
            component: 'event-bridge',
            merchantId,
            payer,
        });
    }

    private decodeMerchantId(bytes: unknown): string | null {
        if (bytes == null) return null;
        if (typeof bytes === 'string') return bytes;
        if (Buffer.isBuffer(bytes)) return bytes.toString('utf8');
        if (typeof bytes === 'object' && 'xdr' in (bytes as object)) return null;
        return String(bytes);
    }

    stop() {
        this.isRunning = false;
        if (this.pollHandle) {
            clearTimeout(this.pollHandle);
            this.pollHandle = null;
        }
        logger.info('EventBridgeService stopped', { component: 'event-bridge' });
    }
}

export default new EventBridgeService();
