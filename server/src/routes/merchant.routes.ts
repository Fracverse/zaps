import { Router } from 'express';
import multer from 'multer';
import * as merchantController from '../controllers/merchant.controller';
import { adminOnly } from '../middleware/role.middleware';

const router = Router();
const upload = multer({ storage: multer.memoryStorage(), limits: { fileSize: 10 * 1024 * 1024 } });

router.post('/onboard',                                          merchantController.onboard);
router.get('/:id',                                               merchantController.getMerchant);
router.get('/:id/onboarding',                                    merchantController.getOnboardingStatus);
router.post('/:id/onboarding/documents', upload.single('file'), merchantController.uploadDocument);
router.post('/:id/onboarding/submit',                            merchantController.submitOnboarding);
router.post('/:id/onboarding/review',    adminOnly,              merchantController.reviewOnboarding);

export default router;
