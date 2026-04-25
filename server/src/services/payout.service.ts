import prisma from '../utils/prisma';
import logger from '../utils/logger';
import { ApiError } from '../middleware/error.middleware';
import anchorService from './anchor.service';
import { redis } from '../utils/redis';

// ── Constants ──────────────────────────────────────────────────────────────

/** Minimum payout in stroops (e.g. 10 USDC = 10_000_000 stroops) */
const MIN_PAYOUT_STROOPS = 10_000_000n;

/** Platform payout fee in basis points (e.g. 50 bps = 0.5%) */
const PAYOUT_FEE_BPS = 50n;

/** Maximum retry attempts before marking a payout as permanently failed */
const MAX_RETRY_ATTEMPTS = 3;

/** Base retry delay in ms (doubles on each attempt) */
const BASE_RETRY_DELAY_MS = 60_000;

// ── Types ──────────────────────────────────────────────────────────────────

export interface RequestPayoutInput {
    merchantId: string;
    amount: string;        // stroops as string
    asset: string;
    bankAccountId: string;
    anchorId: string;
}

// ── Service ────────────────────────────────────────────────────────────────

/**
 * Merchant Payout System.
 *
 * Responsibilities:
 *   - Validate bank accounts before payout
 *   - Enforce minimum payout thresholds
 *   - Calculate and deduct platform fees
 *   - Submit withdrawals via the anchor service (SEP-24/31)
 *   - Retry failed payouts with exponential backoff
 *   - Maintain full payout history
 *   - Notify merchants via Redis pub/sub (picked up by notification service)
 */
class PayoutService {
    // ── Manual payout request ──────────────────────────────────────────────

    async requestPayout(input: RequestPayoutInput) {
        const { merchantId, amount, asset, bankAccountId, anchorId } = input;
        const amountBig = BigInt(amount);

        // Enforce minimum threshold
        if (amountBig < MIN_PAYOUT_STROOPS) {
            throw new ApiError(
                400,
                `Payout amount below minimum of ${MIN_PAYOUT_STROOPS} stroops`,
                'PAYOUT_BELOW_MINIMUM',
            );
        }

        // Verify bank account belongs to merchant and is verified
        const bankAccount = await prisma.bankAccount.findFirst({
            where: { id: bankAccountId, merchantId, verified: true },
        });
        if (!bankAccount) {
            throw new ApiError(404, 'Bank account not found or not verified', 'BANK_ACCOUNT_INVALID');
        }

        // Calculate fee
        const feeAmount = (amountBig * PAYOUT_FEE_BPS) / 10_000n;
        const netAmount = amountBig - feeAmount;

        // Create payout record
        const payout = await prisma.payout.create({
            data: {
                merchantId,
                bankAccountId,
                grossAmount: amountBig,
                feeAmount,
                netAmount,
                asset,
                anchorId,
                status: 'PENDING',
                attemptCount: 0,
            },
        });

        logger.info('[PayoutService] Payout requested', {
            id: payout.id,
            merchantId,
            netAmount: netAmount.toString(),
        });

        // Enqueue for processing
        await this.enqueuePayoutJob(payout.id);
        return payout;
    }

    // ── Payout processing ──────────────────────────────────────────────────

    async processPayoutJob(payoutId: string): Promise<void> {
        const payout = await prisma.payout.findUniqueOrThrow({ where: { id: payoutId } });

        if (payout.status === 'COMPLETED' || payout.status === 'CANCELLED') {
            logger.info('[PayoutService] Skipping already-settled payout', { payoutId });
            return;
        }

        const merchant = await prisma.merchant.findUniqueOrThrow({
            where: { id: payout.merchantId },
            include: { user: true },
        });

        try {
            await prisma.payout.update({
                where: { id: payoutId },
                data: { status: 'PROCESSING', attemptCount: { increment: 1 } },
            });

            // Submit via anchor SEP-24 withdrawal
            const bankAccount = await prisma.bankAccount.findUniqueOrThrow({
                where: { id: payout.bankAccountId },
            });

            const anchorTx = await anchorService.sep24InitiateWithdrawal(
                payout.anchorId,
                merchant.user.anchorJwt ?? '',
                payout.asset,
                merchant.stellarAddress,
            );

            await prisma.payout.update({
                where: { id: payoutId },
                data: {
                    status: 'PROCESSING',
                    anchorTransactionId: anchorTx.id,
                },
            });

            logger.info('[PayoutService] Payout submitted to anchor', {
                payoutId,
                anchorTxId: anchorTx.id,
            });

            await this.notifyMerchant(payout.merchantId, 'PAYOUT_PROCESSING', payoutId);
        } catch (err) {
            logger.error('[PayoutService] Payout attempt failed', { payoutId, err });
            await this.handlePayoutFailure(payoutId, payout.attemptCount + 1);
        }
    }

    // ── Scheduled batch processing ─────────────────────────────────────────

    /**
     * Processes all pending payouts scheduled for automatic disbursement.
     * Called by a cron job (daily/weekly depending on merchant config).
     */
    async processScheduledPayouts(): Promise<void> {
        const pending = await prisma.payout.findMany({
            where: {
                status: 'PENDING',
                scheduledFor: { lte: new Date() },
            },
            take: 50,
        });

        logger.info('[PayoutService] Running scheduled payouts', { count: pending.length });

        for (const payout of pending) {
            try {
                await this.processPayoutJob(payout.id);
            } catch (err) {
                logger.error('[PayoutService] Scheduled payout failed', { id: payout.id, err });
            }
        }
    }

    // ── Retry failed payouts ───────────────────────────────────────────────

    async retryFailedPayouts(): Promise<void> {
        const now = new Date();
        const failed = await prisma.payout.findMany({
            where: {
                status: 'FAILED',
                attemptCount: { lt: MAX_RETRY_ATTEMPTS },
                nextRetryAt: { lte: now },
            },
            take: 20,
        });

        logger.info('[PayoutService] Retrying failed payouts', { count: failed.length });

        for (const payout of failed) {
            await this.processPayoutJob(payout.id);
        }
    }

    // ── Webhook handler ────────────────────────────────────────────────────

    /** Called when anchor confirms or rejects a payout via SEP-24 webhook. */
    async handleAnchorWebhook(anchorId: string, payload: Record<string, unknown>): Promise<void> {
        const anchorTxId = payload['id'] as string | undefined;
        const anchorStatus = payload['status'] as string | undefined;

        if (!anchorTxId || !anchorStatus) return;

        const payout = await prisma.payout.findFirst({
            where: { anchorTransactionId: anchorTxId },
        });
        if (!payout) return;

        const isCompleted = anchorStatus === 'completed';
        const isFailed = anchorStatus === 'error' || anchorStatus === 'refunded';

        if (isCompleted) {
            await prisma.payout.update({
                where: { id: payout.id },
                data: { status: 'COMPLETED', completedAt: new Date() },
            });
            await this.notifyMerchant(payout.merchantId, 'PAYOUT_COMPLETED', payout.id);
            logger.info('[PayoutService] Payout completed', { id: payout.id });
        } else if (isFailed) {
            await this.handlePayoutFailure(payout.id, payout.attemptCount);
        }
    }

    // ── History ────────────────────────────────────────────────────────────

    async getPayoutHistory(merchantId: string, limit = 20, offset = 0) {
        return prisma.payout.findMany({
            where: { merchantId },
            orderBy: { createdAt: 'desc' },
            take: limit,
            skip: offset,
        });
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    private async handlePayoutFailure(payoutId: string, currentAttempts: number): Promise<void> {
        const isPermanent = currentAttempts >= MAX_RETRY_ATTEMPTS;

        const delayMs = BASE_RETRY_DELAY_MS * Math.pow(2, currentAttempts - 1);
        const nextRetry = isPermanent ? null : new Date(Date.now() + delayMs);

        const payout = await prisma.payout.update({
            where: { id: payoutId },
            data: {
                status: isPermanent ? 'PERMANENTLY_FAILED' : 'FAILED',
                nextRetryAt: nextRetry,
            },
        });

        if (isPermanent) {
            logger.warn('[PayoutService] Payout permanently failed', { payoutId });
            await this.notifyMerchant(payout.merchantId, 'PAYOUT_FAILED', payoutId);
        } else {
            logger.info('[PayoutService] Payout scheduled for retry', { payoutId, nextRetry });
        }
    }

    private async enqueuePayoutJob(payoutId: string): Promise<void> {
        await redis.lpush('queue:payouts', payoutId);
    }

    private async notifyMerchant(merchantId: string, event: string, payoutId: string): Promise<void> {
        await redis.publish(
            'merchant:notification',
            JSON.stringify({ merchantId, event, payoutId, timestamp: Date.now() }),
        );
    }
}

export default new PayoutService();
