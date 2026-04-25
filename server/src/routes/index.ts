import { Router } from 'express';
import authRoutes from './auth.routes';
import userRoutes from './user.routes';
import merchantRoutes from './merchant.routes';
import paymentRoutes from './payment.routes';
import adminRoutes from './admin.routes';
import anchorRoutes from './anchor.routes';
import bridgeRoutes from './bridge.routes';
import auditRoutes from './audit.routes';
import notificationRoutes from './notification.routes';
import fileRoutes from './file.routes';
import payoutRoutes from './payout.routes';
import { authenticate } from '../middleware/auth.middleware';
import { auditLogging } from '../middleware/audit.middleware';
import { rateLimit } from '../middleware/rate-limit.middleware';

const router = Router();

// Public routes
router.use('/auth', authRoutes);

// Protected routes
router.use(authenticate);
router.use(auditLogging);
router.use(rateLimit(100));

router.use('/users', userRoutes);
router.use('/merchants', merchantRoutes);
router.use('/payments', paymentRoutes);
router.use('/anchor', anchorRoutes);
router.use('/bridge', bridgeRoutes);
router.use('/audit', auditRoutes);
router.use('/notifications', notificationRoutes);
router.use('/files', fileRoutes);
router.use('/payouts', payoutRoutes);
router.use('/admin', adminRoutes);

export default router;
