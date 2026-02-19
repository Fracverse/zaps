import { Request, Response, NextFunction } from 'express';
import paymentService from '../services/payment.service';
import { ApiError } from '../middleware/error.middleware';

/**
 * Skeletal Blueprint for Payment Endpoints.
 * Orchestrates transaction building and status retrieval.
 */
export const createPayment = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { merchantId, fromAddress, amount, assetCode, assetIssuer } = req.body;
        if (!merchantId || !fromAddress || !amount || !assetCode) throw new ApiError(400, 'Missing payment fields');

        const result = await paymentService.createPayment(merchantId, fromAddress, amount, assetCode, assetIssuer);
        res.status(201).json(result);
    } catch (error) {
        next(error);
    }
};

export const transfer = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { toUserId, amount, assetCode } = req.body;
        const fromUserId = (req as any).user.userId;

        const result = await paymentService.transfer(fromUserId, toUserId, amount, assetCode);
        res.status(201).json(result);
    } catch (error) {
        next(error);
    }
};

export const getPaymentStatus = async (req: Request, res: Response, next: NextFunction) => {
    // Blueprint for status retrieval logic
    res.status(200).json({ status: 'Skeletal retrieval by Hash or ID' });
};
