import { Request, Response, NextFunction } from 'express';
import bridgeService from '../services/bridge.service';
import { ApiError } from '../middleware/error.middleware';

/**
 * Skeletal Blueprint for Bridge Endpoints.
 */
export const initiateTransfer = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const userId = (req as any).user.userId;
        const bridgeTx = await bridgeService.initiateBridgeTransfer({ ...req.body, userId });
        res.status(201).json(bridgeTx);
    } catch (error) {
        next(error);
    }
};

export const confirmTransfer = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { id } = req.params;
        const bridgeTx = await bridgeService.confirmBridgeTransaction(id, req.body.txHash);
        res.status(200).json(bridgeTx);
    } catch (error) {
        next(error);
    }
};
