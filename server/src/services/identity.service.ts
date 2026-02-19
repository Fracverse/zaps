import prisma from '../utils/prisma';
import bcrypt from 'bcryptjs';
import { Role } from '@prisma/client';

/**
 * Skeletal Blueprint for Identity Management.
 * Maps application User IDs to Stellar Addresses.
 */
class IdentityService {
    /**
     * Creates a new user with a hashed PIN.
     * Logic: bcrypt(pin) -> store with stellarAddress -> create default profile.
     */
    async createUser(userId: string, stellarAddress: string, pin: string, role: Role = Role.USER) {
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

    /**
     * Resolves an internal UserId to their public Stellar address.
     */
    async resolveUserId(userId: string) {
        const user = await prisma.user.findUnique({
            where: { userId },
            select: { stellarAddress: true },
        });
        return user?.stellarAddress;
    }
}

export default new IdentityService();
