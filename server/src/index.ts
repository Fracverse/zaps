import app from './app';
import config from './config';
import { startWorkers } from './workers';
import eventBridgeService from './services/event-bridge.service';
import evmBridgeMonitorService from './services/evm-bridge-monitor.service';
import logger from './utils/logger';

const PORT = config.port || 3001;

startWorkers();
eventBridgeService.start();
evmBridgeMonitorService.start();

const server = app.listen(PORT, () => {
    logger.info(`Server is running on port ${PORT}`);
});

const shutdown = async () => {
    logger.info('Shutting down server...');
    eventBridgeService.stop();
    evmBridgeMonitorService.stop();
    server.close(() => {
        logger.info('HTTP server closed.');
        process.exit(0);
    });
};

process.on('SIGINT', shutdown);
process.on('SIGTERM', shutdown);
