import dotenv from 'dotenv';

dotenv.config();

export default {
    port: process.env.PORT || 3000,
    stellar: {
        network: process.env.STELLAR_NETWORK || 'TESTNET',
        rpcUrl: process.env.SOROBAN_RPC_URL || 'https://soroban-testnet.stellar.org',
        horizonUrl: process.env.STELLAR_HORIZON_URL || 'https://horizon-testnet.stellar.org',
        networkPassphrase: process.env.STELLAR_NETWORK_PASSPHRASE || 'Test SDF Network ; September 2015',
        feePayerSecret: process.env.FEE_PAYER_SECRET || '',
        paymentRouterContract: process.env.PAYMENT_ROUTER_CONTRACT || '',
        registryContract: process.env.REGISTRY_CONTRACT || '',
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
