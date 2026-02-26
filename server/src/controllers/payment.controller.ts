import { Request, Response, NextFunction } from 'express';
import paymentService from '../services/payment.service';
import { ApiError } from '../middleware/error.middleware';

/**
 * POST /api/v1/payment/create
 * Builds an unsigned, fee-sponsored XDR for a merchant payment.
 * Returns the half-signed XDR for the client to countersign.
 */
export const createPayment = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { merchantId, fromAddress, amount, assetCode, assetIssuer, memo, minReceive } = req.body;
        const userId = (req as any).user?.userId;

        if (!merchantId || !fromAddress || !amount || !assetCode) {
            throw new ApiError(400, 'Missing required fields: merchantId, fromAddress, amount, assetCode', 'VALIDATION_ERROR');
        }
        if (!userId) throw new ApiError(401, 'Authentication required', 'AUTH_REQUIRED');

        const result = await paymentService.createPayment(
            userId,
            merchantId,
            fromAddress,
            amount,
            assetCode,
            assetIssuer,
            memo,
            minReceive,
        );
        res.status(201).json(result);
    } catch (error) {
        next(error);
    }
};

/**
 * POST /api/v1/payment/transfer
 * Builds an unsigned, fee-sponsored XDR for a P2P transfer.
 */
export const transfer = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { toUserId, amount, assetCode, assetIssuer, memo } = req.body;
        const fromUserId = (req as any).user?.userId;

        if (!fromUserId) throw new ApiError(401, 'Authentication required', 'AUTH_REQUIRED');
        if (!toUserId || !amount || !assetCode) {
            throw new ApiError(400, 'Missing required fields: toUserId, amount, assetCode', 'VALIDATION_ERROR');
        }

        const result = await paymentService.transfer(fromUserId, toUserId, amount, assetCode, assetIssuer, memo);
        res.status(201).json(result);
    } catch (error) {
        next(error);
    }
};

/**
 * GET /api/v1/payment/:id
 * Retrieves payment status by ID.
 */
export const getPaymentStatus = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { id } = req.params;
        if (!id) throw new ApiError(400, 'Payment ID is required', 'VALIDATION_ERROR');

        const result = await paymentService.getPaymentStatus(id);
        res.status(200).json(result);
    } catch (error) {
        next(error);
    }
};

/**
 * POST /api/v1/payment/qr/generate
 * Generates a QR code payment URI for a merchant.
 */
export const generateQr = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { merchantId, amount, assetCode, memo, ttlSeconds } = req.body;

        if (!merchantId || !amount || !assetCode) {
            throw new ApiError(400, 'Missing required fields: merchantId, amount, assetCode', 'VALIDATION_ERROR');
        }

        const result = paymentService.generateQrPayload(merchantId, amount, assetCode, memo, ttlSeconds);
        res.status(200).json(result);
    } catch (error) {
        next(error);
    }
};

/**
 * POST /api/v1/payment/nfc/generate
 * Generates an NFC tap-to-pay URI payload for a merchant.
 */
export const generateNfc = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { merchantId, amount, assetCode, memo } = req.body;

        if (!merchantId || !amount || !assetCode) {
            throw new ApiError(400, 'Missing required fields: merchantId, amount, assetCode', 'VALIDATION_ERROR');
        }

        const result = paymentService.generateNfcPayload(merchantId, amount, assetCode, memo);
        res.status(200).json(result);
    } catch (error) {
        next(error);
    }
};
