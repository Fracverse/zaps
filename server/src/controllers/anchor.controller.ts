import { Request, Response, NextFunction } from 'express';
import anchorService from '../services/anchor.service';
import { ApiError } from '../middleware/error.middleware';

/**
 * Skeletal Blueprint for Anchor (SEP-24/31) Endpoints.
 */
export const createWithdrawal = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { destinationAddress, amount, asset } = req.body;
        const userId = (req as any).user.userId;

        const withdrawal = await anchorService.createWithdrawal(userId, destinationAddress, amount, asset);
        res.status(201).json(withdrawal);
    } catch (error) {
        next(error);
    }
};

export const getStatus = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const status = await anchorService.getWithdrawalStatus(req.params.id);
        if (!status) throw new ApiError(404, 'Withdrawal not found');
        res.status(200).json(status);
    } catch (error) {
        next(error);
    }
};
