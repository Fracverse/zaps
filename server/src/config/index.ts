import dotenv from 'dotenv';

dotenv.config();

export const eventBridgeConfig = {
    pollIntervalMs: parseInt(process.env.EVENT_BRIDGE_POLL_MS || '5000', 10),
    errorBackoffMs: parseInt(process.env.EVENT_BRIDGE_ERROR_BACKOFF_MS || '10000', 10),
    contractIds: (process.env.SOROBAN_CONTRACT_IDS || '')
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean),
};

export default {
    port: process.env.PORT || 3000,
    stellar: {
        network: process.env.STELLAR_NETWORK || 'TESTNET',
        rpcUrl: process.env.SOROBAN_RPC_URL || 'https://soroban-testnet.stellar.org',
        networkPassphrase: process.env.STELLAR_NETWORK_PASSPHRASE || 'Test SDF Network ; September 2015',
    },
    database: {
        url: process.env.DATABASE_URL,
    },
    redis: {
        host: process.env.REDIS_HOST || 'localhost',
        port: parseInt(process.env.REDIS_PORT || '6379', 10),
        password: process.env.REDIS_PASSWORD,
    },
    jwtSecret: process.env.JWT_SECRET || 'super-secret-key',
};
