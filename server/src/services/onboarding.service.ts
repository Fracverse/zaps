import { DocumentType, MerchantDocument, MerchantOnboarding } from '@prisma/client';
import prisma from '../utils/prisma';
import storageService from './storage.service';
import complianceService from './compliance.service';
import queueService, { JobType } from './queue.service';
import { ApiError } from '../middleware/error.middleware';
import logger from '../utils/logger';
import { v4 as uuidv4 } from 'uuid';

const REQUIRED_DOCUMENT_TYPES: DocumentType[] = [
    DocumentType.BUSINESS_REGISTRATION,
    DocumentType.PROOF_OF_ADDRESS,
    DocumentType.OWNER_ID,
    DocumentType.BANK_STATEMENT,
];

class OnboardingService {
    /**
     * Step 1 — POST /merchants/onboard
     * Creates Merchant (active=false) + MerchantOnboarding atomically.
     */
    async initiate(data: {
        businessName: string;
        businessEmail: string;
        country: string;
        vaultAddress: string;
        settlementAsset: string;
    }): Promise<MerchantOnboarding> {
        const merchantId = uuidv4();

        const [, onboarding] = await prisma.$transaction([
            prisma.merchant.create({
                data: {
                    merchantId,
                    vaultAddress: data.vaultAddress,
                    settlementAsset: data.settlementAsset,
                    active: false,
                },
            }),
            prisma.merchantOnboarding.create({
                data: {
                    merchantId,
                    businessName: data.businessName,
                    businessEmail: data.businessEmail,
                    country: data.country,
                },
            }),
        ]);

        logger.info(`Onboarding initiated for merchant ${merchantId}`);
        return onboarding;
    }

    /**
     * Step 2 — POST /merchants/:id/onboarding/documents
     * Virus-scans, uploads, and records a document.
     * Advances step to DOCUMENTS_UPLOADED when all four types are present.
     */
    async uploadDocument(
        onboardingId: string,
        file: Express.Multer.File,
        type: DocumentType
    ): Promise<MerchantDocument> {
        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { id: onboardingId },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');
        if (onboarding.status !== 'PENDING') {
            throw new ApiError(400, 'Documents can only be uploaded while status is PENDING', 'INVALID_STATE');
        }

        const clean = await storageService.scanForViruses(file);
        if (!clean) throw new ApiError(400, 'File failed security scan', 'SECURITY_SCAN_FAILED');

        const stored = await storageService.uploadFile(file, 'merchant-docs');

        const doc = await prisma.merchantDocument.create({
            data: {
                onboardingId,
                type,
                storageId: stored.id,
                storageUrl: stored.url,
                mimeType: stored.mimeType,
                size: stored.size,
            },
        });

        if (await this.allRequiredDocsPresent(onboardingId)) {
            await prisma.merchantOnboarding.update({
                where: { id: onboardingId },
                data: { currentStep: 'DOCUMENTS_UPLOADED' },
            });
        }

        return doc;
    }

    /**
     * Step 3 — POST /merchants/:id/onboarding/submit
     * Fast-path sanctions check, then sets UNDER_REVIEW and dispatches job.
     */
    async submit(onboardingId: string): Promise<MerchantOnboarding> {
        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { id: onboardingId },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');
        if (onboarding.status !== 'PENDING') {
            throw new ApiError(400, 'Onboarding has already been submitted', 'INVALID_STATE');
        }
        if (onboarding.currentStep !== 'DOCUMENTS_UPLOADED') {
            throw new ApiError(400, 'All required documents must be uploaded before submitting', 'MISSING_DOCUMENTS');
        }

        const sanctioned = await complianceService.checkSanctions(onboarding.merchantId);
        if (sanctioned) {
            return this.reject(onboardingId, 'system', 'Sanctions screening match');
        }

        const updated = await prisma.merchantOnboarding.update({
            where: { id: onboardingId },
            data: {
                status: 'UNDER_REVIEW',
                currentStep: 'COMPLIANCE_CHECK',
                submittedAt: new Date(),
            },
        });

        await queueService.addJob({
            type: JobType.ONBOARDING,
            data: { onboardingId, merchantId: onboarding.merchantId },
        });

        logger.info(`Onboarding ${onboardingId} submitted for review`);
        return updated;
    }

    /**
     * Step 4a — Approve: activate merchant, verify documents, notify.
     * Called by the BullMQ processor (actorId='system') or admin endpoint.
     */
    async approve(onboardingId: string, actorId: string): Promise<MerchantOnboarding> {
        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { id: onboardingId },
            include: { documents: true },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');
        if (onboarding.status !== 'UNDER_REVIEW') {
            throw new ApiError(400, 'Onboarding is not under review', 'INVALID_STATE');
        }

        const now = new Date();
        const [updated] = await prisma.$transaction([
            prisma.merchantOnboarding.update({
                where: { id: onboardingId },
                data: {
                    status: 'APPROVED',
                    currentStep: 'COMPLETED',
                    reviewedAt: now,
                    reviewedBy: actorId,
                },
            }),
            prisma.merchant.update({
                where: { merchantId: onboarding.merchantId },
                data: { active: true },
            }),
            prisma.merchantDocument.updateMany({
                where: { onboardingId },
                data: { status: 'VERIFIED', verifiedAt: now },
            }),
        ]);

        await queueService.addJob({
            type: JobType.EMAIL,
            data: {
                to: onboarding.businessEmail,
                subject: 'Your merchant account has been approved',
                templateId: 'merchant_approved',
                templateData: { businessName: onboarding.businessName },
            },
        });

        logger.info(`Onboarding ${onboardingId} approved by ${actorId}`);
        return updated;
    }

    /**
     * Step 4b — Reject: record reason, notify merchant.
     */
    async reject(onboardingId: string, actorId: string, reason: string): Promise<MerchantOnboarding> {
        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { id: onboardingId },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');

        const updated = await prisma.merchantOnboarding.update({
            where: { id: onboardingId },
            data: {
                status: 'REJECTED',
                rejectionReason: reason,
                reviewedAt: new Date(),
                reviewedBy: actorId,
            },
        });

        await queueService.addJob({
            type: JobType.EMAIL,
            data: {
                to: onboarding.businessEmail,
                subject: 'Update on your merchant application',
                templateId: 'merchant_rejected',
                templateData: { businessName: onboarding.businessName, reason },
            },
        });

        logger.info(`Onboarding ${onboardingId} rejected by ${actorId}: ${reason}`);
        return updated;
    }

    /**
     * GET /merchants/:id/onboarding — read current status with documents.
     */
    async getStatus(merchantId: string): Promise<MerchantOnboarding & { documents: MerchantDocument[] }> {
        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { merchantId },
            include: { documents: true },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');
        return onboarding;
    }

    private async allRequiredDocsPresent(onboardingId: string): Promise<boolean> {
        const uploaded = await prisma.merchantDocument.findMany({
            where: { onboardingId },
            select: { type: true },
        });
        const uploadedTypes = new Set(uploaded.map((d) => d.type));
        return REQUIRED_DOCUMENT_TYPES.every((t) => uploadedTypes.has(t));
    }
}

export default new OnboardingService();
