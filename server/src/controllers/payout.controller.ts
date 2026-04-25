import { Request, Response, NextFunction } from 'express';
import { z } from 'zod';
import payoutService from '../services/payout.service';

const requestPayoutSchema = z.object({
    amount: z.string().regex(/^\d+$/, 'amount must be a positive integer string (stroops)'),
    asset: z.string().min(1),
    bankAccountId: z.string().uuid(),
    anchorId: z.string().min(1),
});

export const requestPayout = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const merchantId = (req as any).user?.merchantId as string;
        if (!merchantId) {
            return res.status(403).json({ error: 'Merchant access required' });
        }

        const body = requestPayoutSchema.parse(req.body);
        const payout = await payoutService.requestPayout({ merchantId, ...body });
        res.status(202).json({ payout });
    } catch (err) {
        next(err);
    }
};

export const getPayoutHistory = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const merchantId = (req as any).user?.merchantId as string;
        const limit = Math.min(Number(req.query.limit) || 20, 100);
        const offset = Number(req.query.offset) || 0;
        const history = await payoutService.getPayoutHistory(merchantId, limit, offset);
        res.json({ payouts: history });
    } catch (err) {
        next(err);
    }
};

export const anchorWebhook = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { anchorId } = req.params;
        await payoutService.handleAnchorWebhook(anchorId, req.body as Record<string, unknown>);
        res.status(200).json({ received: true });
    } catch (err) {
        next(err);
    }
};
