import dotenv from 'dotenv';

dotenv.config();

const bridgeSupportedChainsEnv = process.env.BRIDGE_SUPPORTED_CHAINS || 'ethereum,polygon,bsc';
const bridgeSupportedAssetsEnv = process.env.BRIDGE_SUPPORTED_ASSETS || 'USDC,USDT';

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
    bridge: {
        supportedChains: bridgeSupportedChainsEnv.split(',').map(chain => chain.trim().toLowerCase()).filter(Boolean),
        supportedAssets: bridgeSupportedAssetsEnv.split(',').map(asset => asset.trim()).filter(Boolean),
    },
};
