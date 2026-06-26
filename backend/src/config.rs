pub struct Config {
    pub database_url: String,
    pub stellar_rpc_url: String,
    pub allbridge_api_url: String,
    pub jwt_secret: String,
}

impl Config {
    pub fn from_env() -> Self {
        // Read configuration from environment variables (fallback to defaults for development)
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/zaps".into()),
            stellar_rpc_url: std::env::var("STELLAR_RPC_URL")
                .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".into()),
            allbridge_api_url: std::env::var("ALLBRIDGE_API_URL")
                .unwrap_or_else(|_| "https://core-api.allbridge.io".into()),
            jwt_secret: std::env::var("JWT_SECRET")
                .unwrap_or_else(|_| "zaps-jwt-secret-placeholder-very-long-key".into()),
        }
    }
}
