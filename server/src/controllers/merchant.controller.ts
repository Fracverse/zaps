import { Request, Response, NextFunction } from 'express';
import { z } from 'zod';
import { DocumentType } from '@prisma/client';
import prisma from '../utils/prisma';
import onboardingService from '../services/onboarding.service';
import { ApiError } from '../middleware/error.middleware';

const OnboardSchema = z.object({
    businessName: z.string().min(1),
    businessEmail: z.string().email(),
    country: z.string().min(2),
    vaultAddress: z.string().min(1),
    settlementAsset: z.string().min(1),
});

const ReviewSchema = z.union([
    z.object({ decision: z.literal('approve') }),
    z.object({ decision: z.literal('reject'), reason: z.string().min(1) }),
]);

export const onboard = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const body = OnboardSchema.safeParse(req.body);
        if (!body.success) throw new ApiError(400, body.error.errors[0].message, 'VALIDATION_ERROR');

        const onboarding = await onboardingService.initiate(body.data);
        res.status(201).json(onboarding);
    } catch (error) {
        next(error);
    }
};

export const getMerchant = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const merchant = await prisma.merchant.findUnique({
            where: { merchantId: req.params.id },
        });
        if (!merchant) throw new ApiError(404, 'Merchant not found', 'NOT_FOUND');
        res.status(200).json(merchant);
    } catch (error) {
        next(error);
    }
};

export const getOnboardingStatus = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const onboarding = await onboardingService.getStatus(req.params.id);
        res.status(200).json(onboarding);
    } catch (error) {
        next(error);
    }
};

export const uploadDocument = async (req: Request, res: Response, next: NextFunction) => {
    try {
        if (!req.file) throw new ApiError(400, 'No file provided', 'MISSING_FILE');

        const type = req.body.type as DocumentType;
        if (!Object.values(DocumentType).includes(type)) {
            throw new ApiError(400, `Invalid document type. Must be one of: ${Object.values(DocumentType).join(', ')}`, 'VALIDATION_ERROR');
        }

        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { merchantId: req.params.id },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');

        const doc = await onboardingService.uploadDocument(onboarding.id, req.file, type);
        res.status(201).json(doc);
    } catch (error) {
        next(error);
    }
};

export const submitOnboarding = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { merchantId: req.params.id },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');

        const updated = await onboardingService.submit(onboarding.id);
        res.status(200).json(updated);
    } catch (error) {
        next(error);
    }
};

export const reviewOnboarding = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const body = ReviewSchema.safeParse(req.body);
        if (!body.success) throw new ApiError(400, body.error.errors[0].message, 'VALIDATION_ERROR');

        const onboarding = await prisma.merchantOnboarding.findUnique({
            where: { merchantId: req.params.id },
        });
        if (!onboarding) throw new ApiError(404, 'Onboarding record not found', 'NOT_FOUND');

        const actorId = (req as any).user?.userId ?? 'admin';
        const updated = body.data.decision === 'approve'
            ? await onboardingService.approve(onboarding.id, actorId)
            : await onboardingService.reject(onboarding.id, actorId, body.data.reason);

        res.status(200).json(updated);
    } catch (error) {
        next(error);
    }
};
