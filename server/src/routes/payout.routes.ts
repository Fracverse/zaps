import { Router } from 'express';
import * as payoutController from '../controllers/payout.controller';

const router = Router();

// POST /payouts — request a manual payout
router.post('/', payoutController.requestPayout);

// GET /payouts/history — payout history for authenticated merchant
router.get('/history', payoutController.getPayoutHistory);

// POST /payouts/webhook/:anchorId — receive anchor status callbacks (public, no auth)
router.post('/webhook/:anchorId', payoutController.anchorWebhook);

export default router;
