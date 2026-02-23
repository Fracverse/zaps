import sorobanService from './soroban.service';
import prisma from '../utils/prisma';
import logger from '../utils/logger';

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
        } catch (err: any) {
            logger.error('Error processing event:', { error: err.message, event });
        }
    }

    stop() {
        this.isRunning = false;
    }
}

export default new EventBridgeService();
