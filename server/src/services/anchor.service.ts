import axios, { AxiosInstance } from 'axios';
import jwt from 'jsonwebtoken';
import prisma from '../utils/prisma';
import logger from '../utils/logger';
import { ApiError } from '../middleware/error.middleware';
import complianceService from './compliance.service';
import config from '../config';

// ── Types ──────────────────────────────────────────────────────────────────

interface Sep10Challenge {
    transaction: string;
    network_passphrase: string;
}

interface Sep10Token {
    token: string;
}

interface Sep24DepositResponse {
    type: 'interactive_customer_info_needed';
    url: string;
    id: string;
}

interface Sep24TransactionStatus {
    transaction: {
        id: string;
        kind: 'deposit' | 'withdrawal';
        status: string;
        amount_in?: string;
        amount_out?: string;
        fee_charged?: string;
        message?: string;
    };
}

interface Sep31QuoteRequest {
    amount: string;
    asset_code: string;
    asset_issuer?: string;
    destination_asset?: string;
    source_asset?: string;
    country_code?: string;
}

interface Sep31SendRequest {
    amount: string;
    asset_code: string;
    asset_issuer?: string;
    sender_id: string;
    receiver_id: string;
    fields: {
        transaction: Record<string, string>;
    };
}

// ── Service ────────────────────────────────────────────────────────────────

/**
 * Full Stellar Anchor Integration: SEP-10, SEP-24, and SEP-31.
 *
 * SEP-10  — Stellar Web Authentication (JWT-based identity for anchor APIs)
 * SEP-24  — Hosted Deposit & Withdrawal (interactive iframes / deep links)
 * SEP-31  — Cross-Border Payments (direct API, no user interaction)
 *
 * Multi-anchor support: the anchor is resolved from config by its `anchorId`
 * so multiple anchors can be registered and selected at call time.
 */
class AnchorService {
    // ── SEP-10: Stellar Web Authentication ────────────────────────────────

    /**
     * Performs the full SEP-10 challenge/response handshake and returns a
     * signed JWT that can be used as a Bearer token for SEP-24/31 calls.
     */
    async sep10Authenticate(anchorId: string, userAddress: string, userSecret: string): Promise<string> {
        const anchor = this.getAnchorConfig(anchorId);
        const client = this.httpClient(anchor.url);

        // Step 1: Request the challenge transaction
        const challengeRes = await client.get<Sep10Challenge>('/auth', {
            params: { account: userAddress },
        });
        const { transaction, network_passphrase } = challengeRes.data;

        // Step 2: Sign the challenge with the user's secret key
        const { TransactionBuilder, Keypair, Networks } = await import('@stellar/stellar-sdk');
        const keypair = Keypair.fromSecret(userSecret);
        const tx = TransactionBuilder.fromXDR(
            transaction,
            network_passphrase ?? Networks.TESTNET,
        ) as any;
        tx.sign(keypair);
        const signedXdr = tx.toEnvelope().toXDR('base64');

        // Step 3: Submit the signed transaction to receive a JWT
        const tokenRes = await client.post<Sep10Token>('/auth', { transaction: signedXdr });
        const { token } = tokenRes.data;

        logger.info('[AnchorService] SEP-10 auth complete', { anchorId, userAddress });
        return token;
    }

    // ── SEP-24: Hosted Deposit & Withdrawal ───────────────────────────────

    /**
     * Returns the interactive URL for a SEP-24 deposit flow.
     * The frontend should open this URL in a popup / webview.
     */
    async sep24InitiateDeposit(
        anchorId: string,
        jwt: string,
        assetCode: string,
        account: string,
        amountHint?: string,
    ): Promise<Sep24DepositResponse> {
        const anchor = this.getAnchorConfig(anchorId);
        const client = this.httpClient(anchor.url, jwt);

        const res = await client.post<Sep24DepositResponse>('/transactions/deposit/interactive', {
            asset_code: assetCode,
            account,
            amount: amountHint,
        });

        logger.info('[AnchorService] SEP-24 deposit initiated', {
            anchorId,
            id: res.data.id,
            assetCode,
        });
        return res.data;
    }

    /**
     * Returns the interactive URL for a SEP-24 withdrawal flow.
     */
    async sep24InitiateWithdrawal(
        anchorId: string,
        jwtToken: string,
        assetCode: string,
        account: string,
    ): Promise<Sep24DepositResponse> {
        const anchor = this.getAnchorConfig(anchorId);
        const client = this.httpClient(anchor.url, jwtToken);

        const res = await client.post<Sep24DepositResponse>('/transactions/withdraw/interactive', {
            asset_code: assetCode,
            account,
        });

        logger.info('[AnchorService] SEP-24 withdrawal initiated', {
            anchorId,
            id: res.data.id,
            assetCode,
        });
        return res.data;
    }

    /**
     * Polls the anchor for the current status of a SEP-24 transaction.
     */
    async sep24GetTransactionStatus(
        anchorId: string,
        jwtToken: string,
        transactionId: string,
    ): Promise<Sep24TransactionStatus> {
        const anchor = this.getAnchorConfig(anchorId);
        const client = this.httpClient(anchor.url, jwtToken);

        const res = await client.get<Sep24TransactionStatus>('/transaction', {
            params: { id: transactionId },
        });
        return res.data;
    }

    // ── SEP-31: Cross-Border Payments ─────────────────────────────────────

    /**
     * Requests a fee quote for a SEP-31 cross-border payment.
     */
    async sep31GetQuote(anchorId: string, jwtToken: string, req: Sep31QuoteRequest) {
        const anchor = this.getAnchorConfig(anchorId);
        const client = this.httpClient(anchor.url, jwtToken);

        const res = await client.get('/rates', { params: req });
        logger.info('[AnchorService] SEP-31 quote received', { anchorId });
        return res.data;
    }

    /**
     * Initiates a SEP-31 direct cross-border payment.
     */
    async sep31Send(anchorId: string, jwtToken: string, req: Sep31SendRequest) {
        const anchor = this.getAnchorConfig(anchorId);
        const client = this.httpClient(anchor.url, jwtToken);

        const res = await client.post('/transactions', req);
        logger.info('[AnchorService] SEP-31 transaction created', {
            anchorId,
            id: res.data?.id,
        });
        return res.data;
    }

    // ── Existing withdrawal helpers (enhanced) ────────────────────────────

    async createWithdrawal(
        userId: string,
        destinationAddress: string,
        amount: string,
        asset: string,
    ) {
        if (await complianceService.checkSanctions(userId)) {
            throw new ApiError(403, 'User is sanctioned', 'COMPLIANCE_SANCTIONS');
        }
        await complianceService.checkVelocity(userId, amount);

        const withdrawal = await prisma.withdrawal.create({
            data: {
                userId,
                destinationAddress,
                amount: BigInt(amount),
                asset,
                status: 'PENDING',
            },
        });

        logger.info('[AnchorService] Withdrawal created', { id: withdrawal.id, userId });
        return withdrawal;
    }

    async getWithdrawalStatus(id: string) {
        return prisma.withdrawal.findUnique({ where: { id } });
    }

    /** Process anchor webhook callback for SEP-24/31 status updates. */
    async processWebhook(anchorId: string, payload: Record<string, unknown>): Promise<void> {
        logger.info('[AnchorService] Webhook received', { anchorId, payload });

        const transactionId = payload['id'] as string | undefined;
        const status = payload['status'] as string | undefined;

        if (!transactionId || !status) {
            logger.warn('[AnchorService] Webhook missing id or status — ignoring');
            return;
        }

        // Map anchor status to our internal withdrawal status
        const internalStatus = this.mapAnchorStatus(status);

        await prisma.withdrawal.updateMany({
            where: { anchorTransactionId: transactionId },
            data: { status: internalStatus },
        });

        logger.info('[AnchorService] Withdrawal status updated from webhook', {
            transactionId,
            status: internalStatus,
        });
    }

    // ── Helpers ────────────────────────────────────────────────────────────

    private getAnchorConfig(anchorId: string) {
        const anchors: Record<string, { url: string }> = (config as any).anchors ?? {};
        const anchor = anchors[anchorId];
        if (!anchor) {
            throw new ApiError(400, `Unknown anchor: ${anchorId}`, 'ANCHOR_NOT_FOUND');
        }
        return anchor;
    }

    private httpClient(baseURL: string, jwtToken?: string): AxiosInstance {
        return axios.create({
            baseURL,
            timeout: 15_000,
            headers: jwtToken ? { Authorization: `Bearer ${jwtToken}` } : {},
        });
    }

    private mapAnchorStatus(anchorStatus: string): string {
        const map: Record<string, string> = {
            completed: 'COMPLETED',
            pending_external: 'PROCESSING',
            pending_anchor: 'PROCESSING',
            pending_stellar: 'PROCESSING',
            pending_user: 'PENDING',
            error: 'FAILED',
            refunded: 'REFUNDED',
        };
        return map[anchorStatus] ?? 'PENDING';
    }
}

export default new AnchorService();
