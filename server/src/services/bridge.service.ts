import prisma from '../utils/prisma';
import logger from '../utils/logger';
import config from '../config';
import { ApiError } from '../middleware/error.middleware';
import identityService from './identity.service';

class BridgeService {
    private validateChain(fromChain: string) {
        const supportedChains = config.bridge.supportedChains;
        const normalized = fromChain.toLowerCase();
        if (!supportedChains.includes(normalized)) {
            throw new ApiError(400, `Unsupported source chain: ${fromChain}`);
        }
    }

    private validateAsset(asset: string) {
        const supportedAssets = config.bridge.supportedAssets;
        if (!supportedAssets.includes(asset)) {
            throw new ApiError(400, `Asset ${asset} is not supported for bridging`);
        }
    }

    private parseAmount(amount: unknown) {
        if (typeof amount === 'string' || typeof amount === 'number' || typeof amount === 'bigint') {
            try {
                return BigInt(amount);
            } catch {
                throw new ApiError(400, 'Invalid amount for bridge transfer');
            }
        }
        throw new ApiError(400, 'Amount is required for bridge transfer');
    }

    async initiateBridgeTransfer(data: any) {
        const { fromChain, asset, amount, fromAddress, toChain, destinationAddress } = data;

        if (!fromChain || !asset || !fromAddress || !destinationAddress) {
            throw new ApiError(400, 'fromChain, asset, fromAddress, and destinationAddress are required');
        }

        this.validateChain(fromChain);
        this.validateAsset(asset);

        const resolvedUserId = await identityService.resolveUserIdFromExternalAddress(fromChain, fromAddress);
        if (!resolvedUserId) {
            throw new ApiError(404, 'No user mapping found for external address');
        }

        const normalizedAmount = this.parseAmount(amount);
        const targetChain = toChain || 'stellar';

        logger.info('Bridge: recording inbound transfer intent');

        return prisma.bridgeTransaction.create({
            data: {
                fromChain,
                toChain: targetChain,
                asset,
                amount: normalizedAmount,
                destinationAddress,
                userId: resolvedUserId,
                status: 'PENDING',
            },
        });
    }

    async confirmBridgeTransaction(id: string, txHash: string) {
        const existing = await prisma.bridgeTransaction.findUnique({ where: { id } });
        if (!existing) {
            throw new ApiError(404, 'Bridge transaction not found');
        }

        if (existing.status !== 'PENDING') {
            throw new ApiError(400, 'Only pending bridge transactions can be confirmed');
        }

        return prisma.bridgeTransaction.update({
            where: { id },
            data: { status: 'CONFIRMING', txHash },
        });
    }

    async completeBridgeTransaction(id: string) {
        const existing = await prisma.bridgeTransaction.findUnique({ where: { id } });
        if (!existing) {
            throw new ApiError(404, 'Bridge transaction not found');
        }

        if (existing.status !== 'CONFIRMING') {
            throw new ApiError(400, 'Only confirming bridge transactions can be completed');
        }

        return prisma.bridgeTransaction.update({
            where: { id },
            data: { status: 'COMPLETED' },
        });
    }
}

export default new BridgeService();
