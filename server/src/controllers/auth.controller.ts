import { Request, Response, NextFunction } from 'express';
import authService from '../services/auth.service';
import identityService from '../services/identity.service';
import { ApiError } from '../middleware/error.middleware';

export const register = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { userId, stellarAddress, pin, externalChain, externalAddress } = req.body;
        if (!userId || !stellarAddress || !pin) throw new ApiError(400, 'Missing registration fields');

        const user = await identityService.createUser(userId, stellarAddress, pin);

        if (externalChain && externalAddress) {
            await identityService.mapExternalAddressToUser(user.userId, externalChain, externalAddress);
        }

        res.status(201).json({ message: 'User registered', userId: user.userId });
    } catch (error) {
        next(error);
    }
};

export const login = async (req: Request, res: Response, next: NextFunction) => {
    try {
        const { userId, pin } = req.body;
        const result = await authService.login(userId, pin);
        res.status(200).json(result);
    } catch (error) {
        next(error);
    }
};
