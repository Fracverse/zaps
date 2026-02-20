import connection from '../utils/redis';
import logger from '../utils/logger';
import { ApiError } from '../middleware/error.middleware';

const DAY_SECONDS = 24 * 60 * 60;
const STROOPS_PER_UNIT = 10_000_000n;

const parseAmountToMinorUnits = (amount: string | bigint): bigint => {
    if (typeof amount === 'bigint') return amount;
    if (typeof amount !== 'string') throw new ApiError(400, 'Invalid amount', 'VALIDATION_ERROR');

    const normalized = amount.trim();
    if (!/^\d+(\.\d+)?$/.test(normalized)) throw new ApiError(400, 'Invalid amount format', 'VALIDATION_ERROR');

    const [whole, fractional = ''] = normalized.split('.');
    if (fractional.length > 7) throw new ApiError(400, 'Amount has too many decimals', 'VALIDATION_ERROR');

    const paddedFractional = (fractional + '0000000').slice(0, 7);
    return BigInt(whole) * STROOPS_PER_UNIT + BigInt(paddedFractional);
};

/**
 * Skeletal Blueprint for Risk & Compliance.
 * Implements velocity limits and sanctions screening interfaces.
 */
class ComplianceService {
    private readonly dailyLimit: bigint;
    private readonly sanctionsBlacklist: Set<string>;

    constructor() {
        const limitEnv = process.env.COMPLIANCE_DAILY_LIMIT_USD || '1000';
        this.dailyLimit = parseAmountToMinorUnits(limitEnv);

        const blacklist = process.env.COMPLIANCE_SANCTIONS_BLACKLIST || '';
        this.sanctionsBlacklist = new Set(
            blacklist
                .split(',')
                .map((entry) => entry.trim())
                .filter((entry) => entry.length > 0)
        );
    }

    /**
     * Checks if a user is on a sanctions blacklist (e.g., OFAC).
     */
    async checkSanctions(userId: string): Promise<boolean> {
        // Blueprint: Integrate with screening providers (Chainalysis, TRM, OFAC API).
        const normalized = userId.trim();
        return this.sanctionsBlacklist.has(normalized);
    }

    /**
     * Enforces rolling 24h volume limits using Redis.
     */
    async checkVelocity(userId: string, amount: string | bigint): Promise<void> {
        const amountUnits = parseAmountToMinorUnits(amount);
        if (amountUnits <= 0n) throw new ApiError(400, 'Amount must be positive', 'VALIDATION_ERROR');

        const now = Date.now();
        const cutoff = now - DAY_SECONDS * 1000;
        const key = `compliance:velocity:${userId}`;
        const member = `${now}:${amountUnits.toString()}`;

        try {
            const results = await connection
                .multi()
                .zremrangebyscore(key, 0, cutoff)
                .zadd(key, now, member)
                .zrangebyscore(key, cutoff, now)
                .expire(key, DAY_SECONDS + 3600)
                .exec();

            const rangeResult = results?.[2]?.[1] as string[] | undefined;
            const entries = rangeResult ?? [];

            let total = 0n;
            for (const entry of entries) {
                const [, amountStr] = entry.split(':');
                if (!amountStr) continue;
                total += BigInt(amountStr);
            }

            logger.info(`Compliance velocity check for user ${userId}: total=${total.toString()}`);

            if (total > this.dailyLimit) {
                throw new ApiError(403, 'Velocity limit exceeded', 'COMPLIANCE_VELOCITY');
            }
        } catch (error) {
            if (error instanceof ApiError) throw error;
            logger.error('Compliance velocity check failed:', { error });
            // Fail open to avoid blocking users on Redis failure
            return;
        }
    }
}

export default new ComplianceService();
