import { Request, Response, NextFunction } from 'express';
import bridgeService from '../services/bridge.service';

export const initiateTransfer = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const bridgeTx = await bridgeService.initiateBridgeTransfer(req.body);
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

export const completeTransfer = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { id } = req.params;
        const bridgeTx = await bridgeService.completeBridgeTransaction(id);
        res.status(200).json(bridgeTx);
    } catch (error) {
        next(error);
    }
};
