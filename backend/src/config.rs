use crate::models::{RateLimitConfig, RateLimitScope};
use config::{Config as ConfigBuilder, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    pub server: ServerConfig,
    pub jwt: JwtConfig,
    #[serde(rename = "stellar")]
    pub stellar_network: StellarNetwork,
    #[serde(rename = "anchor")]
    pub anchor_config: AnchorConfig,
    #[serde(rename = "bridge")]
    pub bridge_config: BridgeConfig,
    #[serde(rename = "compliance")]
    pub compliance_config: ComplianceConfig,
    #[serde(rename = "queue")]
    pub queue_config: QueueConfig,
    pub environment: EnvironmentType,
    pub rate_limit: RateLimitConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub expiration_hours: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentType {
    Development,
    Staging,
    Production,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StellarNetwork {
    pub passphrase: String,
    pub horizon_url: String,
    pub rpc_url: String,
    pub network_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchorConfig {
    pub sep24_url: String,
    pub sep31_url: String,
    pub webhook_secret: String,
    pub kyc_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    pub ethereum_rpc_url: String,
    pub polygon_rpc_url: String,
    pub bsc_rpc_url: String,
    pub supported_assets: Vec<String>,
    pub min_bridge_amount: u64,
    pub max_bridge_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    pub sanctions_api_url: String,
    pub sanctions_api_key: String,
    pub velocity_limits: VelocityLimits,
    pub risk_thresholds: RiskThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityLimits {
    pub daily_transaction_limit: u64,
    pub monthly_transaction_limit: u64,
    pub max_transaction_amount: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskThresholds {
    pub high_risk_amount: u64,
    pub medium_risk_amount: u64,
    pub suspicious_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConfig {
    pub redis_url: String,
    pub max_retries: u32,
    pub visibility_timeout_seconds: u64,
    pub backoff_multiplier: f64,
    pub max_backoff_seconds: u64,
    pub dead_letter_max_size: usize,
    pub worker_count: usize,
    pub reclaim_interval_seconds: u64,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let mut builder = ConfigBuilder::builder()
            .add_source(File::with_name("config/default").required(false))
            .add_source(
                Environment::with_prefix("ZAPS")
                    .prefix_separator("_")
                    .separator("__"),
            );

        // Add environment-specific config file
        if let Ok(env) = env::var("RUN_ENV") {
            builder =
                builder.add_source(File::with_name(&format!("config/{}", env)).required(false));
        }

        let config = builder.build()?;
        config.try_deserialize()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig {
                url: "postgres://localhost/zaps".to_string(),
            },
            server: ServerConfig {
                port: 3000,
            },
            jwt: JwtConfig {
                secret: "change-this-in-production".to_string(),
                expiration_hours: 24,
            },
            stellar_network: StellarNetwork {
                passphrase: "Test SDF Network ; September 2015".to_string(),
                horizon_url: "https://horizon-testnet.stellar.org".to_string(),
                rpc_url: "https://soroban-testnet.stellar.org".to_string(),
                network_id: "Test SDF Network ; September 2015".to_string(),
            },
            anchor_config: AnchorConfig {
                sep24_url: "https://anchor.example.com/sep24".to_string(),
                sep31_url: "https://anchor.example.com/sep31".to_string(),
                webhook_secret: "webhook-secret".to_string(),
                kyc_required: true,
            },
            bridge_config: BridgeConfig {
                ethereum_rpc_url: "https://mainnet.infura.io/v3/YOUR_PROJECT_ID".to_string(),
                polygon_rpc_url: "https://polygon-rpc.com".to_string(),
                bsc_rpc_url: "https://bsc-dataseed.binance.org".to_string(),
                supported_assets: vec!["USDC".to_string(), "USDT".to_string()],
                min_bridge_amount: 1_000_000,   // 1 USD in cents
                max_bridge_amount: 100_000_000, // 1000 USD in cents
            },
            compliance_config: ComplianceConfig {
                sanctions_api_url: "https://api.sanctions.example.com".to_string(),
                sanctions_api_key: "api-key".to_string(),
                velocity_limits: VelocityLimits {
                    daily_transaction_limit: 10_000_000,    // 10,000 USD
                    monthly_transaction_limit: 100_000_000, // 100,000 USD
                    max_transaction_amount: 5_000_000,      // 5,000 USD
                },
                risk_thresholds: RiskThresholds {
                    high_risk_amount: 10_000_000,  // 10,000 USD
                    medium_risk_amount: 1_000_000, // 1,000 USD
                    suspicious_patterns: vec![],
                },
            },
            environment: EnvironmentType::Development,
            queue_config: QueueConfig {
                redis_url: "redis://localhost:6379".to_string(),
                max_retries: 3,
                visibility_timeout_seconds: 300,
                backoff_multiplier: 2.0,
                max_backoff_seconds: 3600,
                dead_letter_max_size: 10000,
                worker_count: 4,
                reclaim_interval_seconds: 60,
            rate_limit: RateLimitConfig {
                window_ms: 60000, // 1 minute
                max_requests: 100,
                scope: RateLimitScope::Ip,
            },
        }
    }
}
