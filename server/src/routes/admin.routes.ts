import { Router } from 'express';
import * as adminController from '../controllers/admin.controller';

const router = Router();

// Only Admins should access these
router.get('/dashboard', adminController.getDashboardStats);
router.get('/health', adminController.getSystemHealth);

// Contract monitoring & alerting
router.get('/contracts/health', adminController.getContractHealth);
router.get('/contracts/metrics', adminController.getContractMetrics);
router.get('/contracts/alerts', adminController.getContractAlerts);
router.get('/contracts/metrics/prometheus', adminController.getContractPrometheusMetrics);

export default router;
