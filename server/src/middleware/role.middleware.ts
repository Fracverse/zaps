import { Request, Response, NextFunction } from 'express';

type Role = 'USER' | 'MERCHANT' | 'ADMIN';

export const requireRole = (roles: Role[]) => {
    return (req: any, res: Response, next: NextFunction) => {
        const user = req.user;

        if (!user || !roles.includes(user.role)) {
            return res.status(403).json({ error: 'Access denied: insufficient permissions' });
        }

        next();
    };
};

export const adminOnly = requireRole(['ADMIN']);
export const merchantOnly = requireRole(['MERCHANT', 'ADMIN']);
export const userOnly = requireRole(['USER', 'ADMIN']);
