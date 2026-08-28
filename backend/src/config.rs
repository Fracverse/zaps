pub struct Config {
    pub database_url: String,
    pub stellar_rpc_url: String,
    pub allbridge_api_url: String,
    pub jwt_secret: String,
    /// Redis connection URL for the yield cache. `None` disables caching.
    pub redis_url: Option<String>,
    /// Privy app ID; expected as the `aud` claim on Privy session JWTs.
    pub privy_app_id: String,
    /// JWKS endpoint used to verify Privy session JWTs. Defaults to Privy's
    /// per-app endpoint derived from `privy_app_id`; override with
    /// `PRIVY_JWKS_URL` (e.g. to point at a mock server in tests).
    pub privy_jwks_url: String,
    /// Comma-separated list of frontend origins allowed to call the API
    /// cross-origin. Used to build the Tower CORS middleware whitelist.
    pub cors_allowed_origins: Vec<String>,
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
            redis_url: std::env::var("REDIS_URL")
                .ok()
                .filter(|url| !url.trim().is_empty()),
            privy_app_id: {
                let app_id = std::env::var("PRIVY_APP_ID").unwrap_or_default();
                if app_id.is_empty() {
                    tracing::warn!(
                        "PRIVY_APP_ID not set; Privy JWT verification will reject all tokens \
                         (audience check can never match)"
                    );
                }
                app_id
            },
            privy_jwks_url: std::env::var("PRIVY_JWKS_URL").unwrap_or_else(|_| {
                let app_id = std::env::var("PRIVY_APP_ID").unwrap_or_default();
                format!("https://auth.privy.io/api/v1/apps/{app_id}/jwks.json")
            }),
            cors_allowed_origins: {
                let raw = std::env::var("CORS_ALLOWED_ORIGINS")
                    .map(|v| v.trim().to_string())
                    .ok()
                    .filter(|v| !v.is_empty());
                match raw {
                    Some(list) => list
                        .split(',')
                        .map(|origin| origin.trim().to_string())
                        .filter(|origin| !origin.is_empty())
                        .collect::<Vec<_>>(),
                    None => {
                        tracing::warn!(
                            "CORS_ALLOWED_ORIGINS not set; defaulting to http://localhost:3000"
                        );
                        vec!["http://localhost:3000".to_string()]
                    }
                }
            },
        }
    }
}
