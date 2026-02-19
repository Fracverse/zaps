import { Router } from 'express';
import * as paymentController from '../controllers/payment.controller';

const router = Router();

// POST /api/v1/payment/create — returns unsigned, sponsored XDR
router.post('/create', paymentController.createPayment);

// POST /api/v1/payment/transfer — P2P transfer, returns sponsored XDR
router.post('/transfer', paymentController.transfer);

// GET /api/v1/payment/:id — payment status lookup
router.get('/:id', paymentController.getPaymentStatus);

// POST /api/v1/payment/qr/generate — QR payment URI
router.post('/qr/generate', paymentController.generateQr);

// POST /api/v1/payment/nfc/generate — NFC tap-to-pay payload
router.post('/nfc/generate', paymentController.generateNfc);

export default router;
