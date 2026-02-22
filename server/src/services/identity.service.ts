import prisma from '../utils/prisma';
import bcrypt from 'bcryptjs';

type Role = 'USER' | 'MERCHANT' | 'ADMIN';

class IdentityService {
    async createUser(userId: string, stellarAddress: string, pin: string, role: Role = 'USER') {
        const pinHash = await bcrypt.hash(pin, 10);

        return prisma.user.create({
            data: {
                userId,
                stellarAddress,
                pinHash,
                role,
                profile: { create: { displayName: userId } },
            },
        });
    }

    async resolveUserId(userId: string) {
        const user = await prisma.user.findUnique({
            where: { userId },
            select: { stellarAddress: true },
        });
        return user?.stellarAddress;
    }

    async mapExternalAddressToUser(userId: string, chain: string, address: string) {
        const normalizedChain = chain.toLowerCase();
        const normalizedAddress = address.toLowerCase();

        return prisma.externalAddress.upsert({
            where: {
                chain_address: {
                    chain: normalizedChain,
                    address: normalizedAddress,
                },
            },
            update: {
                userId,
            },
            create: {
                chain: normalizedChain,
                address: normalizedAddress,
                userId,
            },
        });
    }

    async resolveUserIdFromExternalAddress(chain: string, address: string) {
        const normalizedChain = chain.toLowerCase();
        const normalizedAddress = address.toLowerCase();

        const mapping = await prisma.externalAddress.findUnique({
            where: {
                chain_address: {
                    chain: normalizedChain,
                    address: normalizedAddress,
                },
            },
            select: {
                userId: true,
            },
        });

        return mapping?.userId || null;
    }
}

export default new IdentityService();
