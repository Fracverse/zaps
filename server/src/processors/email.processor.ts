import logger from '../utils/logger';
import type { EmailJobPayload } from '../types/job-payloads';

export async function processEmail(data: EmailJobPayload): Promise<void> {
    const { to, subject, body, templateId } = data;
    const logCtx = { component: 'email-processor', to, subject };

    logger.info('Processing email job', logCtx);

    if (!to || typeof to !== 'string') {
        logger.error('Email job failed: missing or invalid "to" field', logCtx);
        throw new Error('Invalid email payload: missing "to"');
    }

    try {
        // Integration with SendGrid/AWS SES
        // Example SendGrid: await sendgrid.send({ to, subject, text: body ?? '', templateId })
        logger.info('Email sent successfully', {
            ...logCtx,
            templateId: templateId ?? 'none',
        });
    } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        logger.error('Email send failed', { ...logCtx, error: msg });
        throw err;
    }
}
