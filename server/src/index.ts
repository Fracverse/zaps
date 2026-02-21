import app from './app';
import config from './config';
import { startWorkers, stopWorkers } from './workers';
import eventBridgeService from './services/event-bridge.service';
import logger from './utils/logger';

const PORT = config.port || 3001;

startWorkers();
eventBridgeService.start();

const server = app.listen(PORT, () => {
    logger.info(`Server is running on port ${PORT}`);
});

const shutdown = async () => {
    logger.info('Shutting down server...');
    eventBridgeService.stop();
    await stopWorkers();
    server.close(() => {
        logger.info('HTTP server closed.');
        process.exit(0);
    });
};

process.on('SIGINT', () => void shutdown());
process.on('SIGTERM', () => void shutdown());
