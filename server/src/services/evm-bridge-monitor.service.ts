import bridgeService from './bridge.service';
import logger from '../utils/logger';

type ChainName = 'ethereum' | 'polygon' | 'bsc';

type InboundTransferEvent = {
    id: string;
    chain: ChainName;
    txHash: string;
    fromAddress: string;
    amount: string;
    asset: string;
    destinationAddress: string;
};

class EvmBridgeMonitorService {
    private isRunning = false;

    async start() {
        if (this.isRunning) return;
        this.isRunning = true;
        this.loop();
    }

    stop() {
        this.isRunning = false;
    }

    private async loop() {
        while (this.isRunning) {
            try {
                const events = await this.fetchNewInboundTransfers();
                for (const event of events) {
                    await this.processInboundTransfer(event);
                }
            } catch (err: any) {
                logger.error('EVM bridge monitor loop error', { error: err.message });
            }

            await new Promise(resolve => setTimeout(resolve, 5000));
        }
    }

    private async fetchNewInboundTransfers(): Promise<InboundTransferEvent[]> {
        return [];
    }

    private async processInboundTransfer(event: InboundTransferEvent) {
        try {
            const bridgeTx = await bridgeService.initiateBridgeTransfer({
                fromChain: event.chain,
                asset: event.asset,
                amount: event.amount,
                fromAddress: event.fromAddress,
                toChain: 'stellar',
                destinationAddress: event.destinationAddress,
            });

            await bridgeService.confirmBridgeTransaction(bridgeTx.id, event.txHash);
        } catch (err: any) {
            logger.error('Failed to process inbound EVM transfer', {
                chain: event.chain,
                txHash: event.txHash,
                error: err.message,
            });
        }
    }
}

export default new EvmBridgeMonitorService();

