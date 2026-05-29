import type { OnboardingJobPayload } from '../types/job-payloads';
import onboardingService from '../services/onboarding.service';
import complianceService from '../services/compliance.service';
import prisma from '../utils/prisma';
import logger from '../utils/logger';

export async function processOnboarding(data: OnboardingJobPayload): Promise<void> {
    const { onboardingId, merchantId } = data;

    const onboarding = await prisma.merchantOnboarding.findUnique({
        where: { id: onboardingId },
    });

    if (!onboarding || onboarding.status !== 'UNDER_REVIEW') {
        logger.warn(`Onboarding job skipped: ${onboardingId} not in UNDER_REVIEW state`);
        return;
    }

    const sanctioned = await complianceService.checkSanctions(merchantId);
    if (sanctioned) {
        await onboardingService.reject(onboardingId, 'system', 'Automated compliance check: sanctions match');
        logger.warn(`Onboarding ${onboardingId} auto-rejected: sanctions match`);
        return;
    }

    await onboardingService.approve(onboardingId, 'system');
    logger.info(`Onboarding ${onboardingId} auto-approved`);
}
