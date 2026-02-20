import { TransactionBuilder, Account, Asset, Operation } from '@stellar/stellar-sdk';
import prisma from '../utils/prisma';
import config from '../config';
import { ApiError } from '../middleware/error.middleware';
import complianceService from './compliance.service';

/**
 * Skeletal Blueprint for Payment Processing.
 * Responsible for building unsigned/sponsored XDRs for client-side signing.
 */
class PaymentService {
    private networkPassphrase = config.stellar.networkPassphrase;

    /**
     * Builds an unsigned XDR for a merchant payment.
     * Flow: Validate Merchant -> Check Compliance -> Build Stellar Payment OP -> Sponsor Fees.
     */
    async createPayment(userId: string, merchantId: string, fromAddress: string, amount: string, assetCode: string, assetIssuer?: string) {
        if (await complianceService.checkSanctions(userId)) {
            throw new ApiError(403, 'User is sanctioned', 'COMPLIANCE_SANCTIONS');
        }
        await complianceService.checkVelocity(userId, amount);

        const merchant = await prisma.merchant.findUnique({ where: { merchantId } });
        if (!merchant) throw new ApiError(404, 'Merchant not found');

        // Asset Resolution (Blueprint: Handle both native and issued assets)
        const asset = assetCode === 'XLM' ? Asset.native() : new Asset(assetCode, assetIssuer!);

        // Transaction Construction (Blueprint: Use a temporary account with seq 0 for blueprinting)
        const tx = new TransactionBuilder(new Account(fromAddress, '0'), {
            fee: '100',
            networkPassphrase: this.networkPassphrase,
        })
            .addOperation(Operation.payment({ destination: merchant.vaultAddress, asset, amount }))
            .setTimeout(0)
            .build();

        return {
            xdr: tx.toXDR(),
            status: 'PENDING',
        };
    }

    /**
     * Skeletal blueprint for User-to-User transfers.
     */
    async transfer(fromUserId: string, toUserId: string, amount: string, assetCode: string, assetIssuer?: string) {
        if (await complianceService.checkSanctions(fromUserId)) {
            throw new ApiError(403, 'User is sanctioned', 'COMPLIANCE_SANCTIONS');
        }
        if (await complianceService.checkSanctions(toUserId)) {
            throw new ApiError(403, 'Recipient is sanctioned', 'COMPLIANCE_SANCTIONS');
        }
        await complianceService.checkVelocity(fromUserId, amount);

        // Implementation: Resolve addresses -> Build Payment XDR -> Return for signing.
        return { xdr: '...', status: 'PENDING' };
    }
}

export default new PaymentService();
