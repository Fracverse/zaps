import {
    TransactionBuilder,
    Account,
    Asset,
    Operation,
    Address,
    nativeToScVal,
    xdr,
    BASE_FEE,
    Networks,
} from '@stellar/stellar-sdk';
import prisma from '../utils/prisma';
import config from '../config';
import { ApiError } from '../middleware/error.middleware';
import complianceService from './compliance.service';
import sorobanService from './soroban.service';
import logger from '../utils/logger';

const networkPassphrase =
    config.stellar.network === 'TESTNET' ? Networks.TESTNET : Networks.PUBLIC;

// ---------- Types ----------

interface CreatePaymentResult {
    paymentId: string;
    xdr: string;
    feePayerAddress: string;
    networkPassphrase: string;
    status: 'PENDING';
}

interface TransferResult {
    transferId: string;
    xdr: string;
    feePayerAddress: string;
    networkPassphrase: string;
    status: 'PENDING';
}

interface QrPayload {
    uri: string;
    expiresAt: number;
}

interface NfcPayload {
    uri: string;
    timestamp: number;
}

// ---------- Service ----------

class PaymentService {
    /**
     * Creates an unsigned, fee-sponsored XDR for a merchant payment via the
     * PaymentRouter Soroban contract.
     *
     * Flow:
     *   1. Validate merchant exists and is active.
     *   2. Run compliance checks (sanctions, velocity).
     *   3. Build the Soroban `pay` invocation XDR.
     *   4. Simulate to get footprints.
     *   5. Sponsor the XLM fee (server signs as fee payer).
     *   6. Persist a PENDING payment row.
     *   7. Return the half-signed XDR for the client to countersign.
     *
     * The server NEVER touches user private keys.
     */
    async createPayment(
        userId: string,
        merchantId: string,
        fromAddress: string,
        amount: string,
        assetCode: string,
        assetIssuer?: string,
        memo?: string,
        minReceive?: string,
    ): Promise<CreatePaymentResult> {
        // 1. Validate merchant
        const merchant = await prisma.merchant.findUnique({ where: { merchantId } });
        if (!merchant) throw new ApiError(404, 'Merchant not found', 'MERCHANT_NOT_FOUND');
        if (!merchant.active) throw new ApiError(400, 'Merchant is inactive', 'MERCHANT_INACTIVE');

        // 2. Compliance
        if (await complianceService.checkSanctions(userId)) {
            throw new ApiError(403, 'User is sanctioned', 'COMPLIANCE_SANCTIONS');
        }
        await complianceService.checkVelocity(userId, amount);

        // 3. Build the Soroban PaymentRouter.pay() invocation
        const routerContract = config.stellar.paymentRouterContract;
        if (!routerContract) {
            throw new ApiError(500, 'PAYMENT_ROUTER_CONTRACT is not configured', 'CONFIG_MISSING');
        }

        const sendAssetAddress = this.resolveAssetAddress(assetCode, assetIssuer);
        const minReceiveAmount = minReceive || amount; // default: 1:1

        const args: xdr.ScVal[] = [
            new Address(fromAddress).toScVal(),                     // from
            nativeToScVal(Buffer.from(merchantId), { type: 'bytes' }), // merchant_id
            new Address(sendAssetAddress).toScVal(),                // send_asset
            nativeToScVal(BigInt(amount), { type: 'i128' }),        // send_amount
            nativeToScVal(BigInt(minReceiveAmount), { type: 'i128' }), // min_receive
        ];

        const unsignedXdr = await sorobanService.buildContractCall(
            fromAddress,
            routerContract,
            'pay',
            args,
        );

        // 4 + 5. Simulate & Sponsor (fee payer signs outer envelope)
        const sponsored = await sorobanService.sponsorTransaction(unsignedXdr);

        // 6. Persist PENDING payment
        const payment = await prisma.payment.create({
            data: {
                fromAddress,
                merchantId,
                sendAsset: assetCode,
                sendAmount: BigInt(amount),
                receiveAmount: minReceive ? BigInt(minReceive) : null,
                status: 'PENDING',
                memo: memo ?? null,
                userAddress: fromAddress,
            },
        });

        logger.info('Payment created', {
            paymentId: payment.id,
            merchantId,
            fromAddress,
            amount,
            asset: assetCode,
        });

        // 7. Return half-signed XDR
        return {
            paymentId: payment.id,
            xdr: sponsored.sponsoredXdr,
            feePayerAddress: sponsored.feePayerAddress,
            networkPassphrase: sponsored.networkPassphrase,
            status: 'PENDING',
        };
    }

    /**
     * Builds an unsigned, fee-sponsored XDR for a P2P (user-to-user) transfer.
     *
     * Flow:
     *   1. Resolve sender and recipient Stellar addresses.
     *   2. Run compliance checks.
     *   3. Build a classic Stellar payment operation XDR.
     *   4. Sponsor the XLM fee.
     *   5. Persist a PENDING transfer row.
     *   6. Return the half-signed XDR.
     */
    async transfer(
        fromUserId: string,
        toUserId: string,
        amount: string,
        assetCode: string,
        assetIssuer?: string,
        memo?: string,
    ): Promise<TransferResult> {
        // 1. Resolve addresses
        const [sender, recipient] = await Promise.all([
            prisma.user.findUnique({ where: { userId: fromUserId } }),
            prisma.user.findUnique({ where: { userId: toUserId } }),
        ]);

        if (!sender) throw new ApiError(404, 'Sender not found', 'SENDER_NOT_FOUND');
        if (!recipient) throw new ApiError(404, 'Recipient not found', 'RECIPIENT_NOT_FOUND');

        // 2. Compliance
        const sanctioned = await complianceService.checkSanctions(fromUserId);
        if (sanctioned) throw new ApiError(403, 'Sender is sanctioned', 'SANCTIONED');
        const recipientSanctioned = await complianceService.checkSanctions(toUserId);
        if (recipientSanctioned) throw new ApiError(403, 'Recipient is sanctioned', 'SANCTIONED');
        await complianceService.checkVelocity(fromUserId, BigInt(amount));

        // 3. Build classic Stellar payment XDR
        const asset = assetCode === 'XLM' ? Asset.native() : new Asset(assetCode, assetIssuer!);

        const source = new Account(sender.stellarAddress, '0');
        const tx = new TransactionBuilder(source, {
            fee: BASE_FEE,
            networkPassphrase,
        })
            .addOperation(
                Operation.payment({
                    destination: recipient.stellarAddress,
                    asset,
                    amount,
                }),
            )
            .setTimeout(300)
            .build();

        // 4. Sponsor
        const sponsored = await sorobanService.sponsorTransaction(tx.toXDR());

        // 5. Persist PENDING transfer
        const transfer = await prisma.transfer.create({
            data: {
                fromUserId,
                toUserId,
                amount: BigInt(amount),
                asset: assetCode,
                status: 'PENDING',
                memo: memo ?? null,
            },
        });

        logger.info('Transfer created', {
            transferId: transfer.id,
            from: fromUserId,
            to: toUserId,
            amount,
            asset: assetCode,
        });

        // 6. Return half-signed XDR
        return {
            transferId: transfer.id,
            xdr: sponsored.sponsoredXdr,
            feePayerAddress: sponsored.feePayerAddress,
            networkPassphrase: sponsored.networkPassphrase,
            status: 'PENDING',
        };
    }

    /**
     * Retrieves payment status by ID.
     */
    async getPaymentStatus(paymentId: string) {
        const payment = await prisma.payment.findUnique({ where: { id: paymentId } });
        if (!payment) throw new ApiError(404, 'Payment not found', 'PAYMENT_NOT_FOUND');
        return {
            id: payment.id,
            txHash: payment.txHash,
            status: payment.status,
            merchantId: payment.merchantId,
            sendAsset: payment.sendAsset,
            sendAmount: payment.sendAmount.toString(),
            receiveAmount: payment.receiveAmount?.toString() ?? null,
            createdAt: payment.createdAt,
        };
    }

    // ---------- QR & NFC ----------

    /**
     * Generates a BLINKS:// QR payment URI.
     * The QR encodes all info needed for the payer's wallet to build the payment.
     */
    generateQrPayload(
        merchantId: string,
        amount: string,
        assetCode: string,
        memo?: string,
        ttlSeconds: number = 600, // 10-minute default
    ): QrPayload {
        const expiresAt = Math.floor(Date.now() / 1000) + ttlSeconds;

        const params = new URLSearchParams({
            merchant: merchantId,
            amount,
            asset: assetCode,
            expiry: expiresAt.toString(),
        });
        if (memo) params.set('memo', memo);

        return {
            uri: `BLINKS://pay?${params.toString()}`,
            expiresAt,
        };
    }

    /**
     * Generates a BLINKS:// NFC tap-to-pay URI payload.
     * Includes a timestamp for freshness validation (5-minute window).
     */
    generateNfcPayload(
        merchantId: string,
        amount: string,
        assetCode: string,
        memo?: string,
    ): NfcPayload {
        const timestamp = Math.floor(Date.now() / 1000);

        const params = new URLSearchParams({
            merchant: merchantId,
            amount,
            asset: assetCode,
            ts: timestamp.toString(),
        });
        if (memo) params.set('memo', memo);

        return {
            uri: `BLINKS://pay?${params.toString()}`,
            timestamp,
        };
    }

    /**
     * Validates that an NFC payload timestamp is within the freshness window.
     */
    validateNfcTimestamp(timestamp: number, windowSeconds: number = 300): boolean {
        const now = Math.floor(Date.now() / 1000);
        return Math.abs(now - timestamp) <= windowSeconds;
    }

    // ---------- Helpers ----------

    /**
     * Resolves a human-readable asset code to a Soroban contract address.
     * For native XLM on Soroban, uses the SAC (Stellar Asset Contract) address.
     */
    private resolveAssetAddress(assetCode: string, assetIssuer?: string): string {
        if (assetCode === 'XLM') {
            // The Stellar Asset Contract (SAC) address for native XLM.
            // This is derived deterministically from the network passphrase.
            const nativeAsset = Asset.native();
            return nativeAsset.contractId(networkPassphrase);
        }
        if (!assetIssuer) {
            throw new ApiError(400, 'assetIssuer is required for non-native assets', 'MISSING_ISSUER');
        }
        const asset = new Asset(assetCode, assetIssuer);
        return asset.contractId(networkPassphrase);
    }
}

export default new PaymentService();
