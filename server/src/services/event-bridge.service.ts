import sorobanService from './soroban.service';
import prisma from '../utils/prisma';
import logger from '../utils/logger';
import queueService, { JobType } from './queue.service';

class EventBridgeService {
    private isRunning: boolean = false;
    private lastLedger: number = 0;

    async start() {
        if (this.isRunning) return;
        this.isRunning = true;
        logger.info('Event Bridge started...');

        // Initialize lastLedger to latest if 0
        if (this.lastLedger === 0) {
            try {
                this.lastLedger = await sorobanService.getLatestLedger();
                logger.info(`Event Bridge initialized at ledger ${this.lastLedger}`);
            } catch (err: any) {
                logger.error('Failed to initialize Event Bridge ledger:', { error: err.message });
                this.lastLedger = 1; // Fallback
            }
        }

        this.poll();
    }

    private async poll() {
        while (this.isRunning) {
            try {
                const eventsResponse = await sorobanService.getEvents(this.lastLedger);
                const events = (eventsResponse as any).events || [];

                for (const event of events) {
                    await this.processEvent(event);
                }

                // Update lastLedger if events were found
                if (events.length > 0) {
                    const latestEventLedger = Math.max(...events.map((e: any) => parseInt(e.ledger, 10)));
                    this.lastLedger = latestEventLedger + 1;
                }

                await new Promise(resolve => setTimeout(resolve, 5000)); // Poll every 5s
            } catch (err: any) {
                logger.error('Event Bridge polling error:', { error: err.message });
                await new Promise(resolve => setTimeout(resolve, 10000));
            }
        }
    }

    private async processEvent(event: any) {
        try {
            // Port logic from indexer_service.rs
            // Checks if event is a contract event and has the expected structure
            if (event.type === 'contract' && event.topic) {
                // Topic decoding would happen here using scValToNative
                // For now assuming existing string topics for simplicity or raw match
                // In production, use scValToNative(xdr.ScVal.fromXDR(event.topic[0], 'base64'))

                // Simplified matching for topic strings if they are not XDR encoded in this mock/stub
                const topic = event.topic[0];

                if (topic === 'PAY_DONE' || topic === 'TRANSFER_DONE') {
                    const paymentId = event.value?.paymentId; // Need to decode value too

                    if (paymentId) {
                        await queueService.addJob({
                            type: JobType.SYNC,
                            data: {
                                syncType: 'ON_CHAIN_COMPLETION',
                                eventType: topic,
                                paymentId: paymentId,
                                rawEvent: event
                            }
                        });
                        logger.info(`Queued SYNC job for ${topic} event: ${paymentId}`);
                    }
                }
            }
        } catch (err: any) {
            logger.error('Error processing event:', { error: err.message, event });
        }
    }

    stop() {
        this.isRunning = false;
    }
}

export default new EventBridgeService();
