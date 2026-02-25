import { Worker } from 'bullmq';
import { connection } from '../utils/redis';
import { JobType } from '../services/queue.service';
import { workerConfig } from '../config/worker.config';
import { getProcessor } from '../processors';
import logger from '../utils/logger';

let worker: Worker | null = null;

export function startWorkers(): Worker {
    if (worker) {
        logger.warn('Workers already started', { component: 'worker' });
        return worker;
    }

    worker = new Worker(
        workerConfig.defaultQueue,
        async (job) => {
            const { id, name, data, attemptsMade } = job;
            const logCtx = { component: 'worker', jobId: id, jobType: name, attempt: attemptsMade + 1 };

            logger.info('Processing job', logCtx);

            const processor = getProcessor(name as JobType);
            if (!processor) {
                logger.warn('Unknown job type, skipping', { ...logCtx, jobType: name });
                return;
            }

            try {
                await processor(data);
                logger.info('Job completed', logCtx);
            } catch (err) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.error('Job processing failed', { ...logCtx, error: msg });
                throw err;
            }
        },
        {
            connection: connection as any,
            concurrency: workerConfig.concurrency,
            limiter: {
                max: 50,
                duration: 1000,
            },
            lockDuration: workerConfig.lockDuration,
            stalledInterval: workerConfig.stalledInterval,
        }
    );

    worker.on('completed', (job) => {
        logger.debug('Job completed', { component: 'worker', jobId: job.id, jobType: job.name });
    });

    worker.on('failed', (job, err) => {
        logger.error('Job failed', {
            component: 'worker',
            jobId: job?.id,
            jobType: job?.name,
            error: err?.message ?? String(err),
            attemptsMade: job?.attemptsMade,
        });
    });

    worker.on('error', (err) => {
        logger.error('Worker error', { component: 'worker', error: err.message });
    });

    logger.info('Background workers started', {
        component: 'worker',
        concurrency: workerConfig.concurrency,
        queue: workerConfig.defaultQueue,
    });

    return worker;
}

export async function stopWorkers(): Promise<void> {
    if (worker) {
        logger.info('Stopping workers gracefully', { component: 'worker' });
        await worker.close();
        worker = null;
        logger.info('Workers stopped', { component: 'worker' });
    }
}

export function getWorker(): Worker | null {
    return worker;
}
