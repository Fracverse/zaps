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
    #[serde(default)]
    pub observability: ObservabilityConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_database_pool_size")]
    pub max_pool_size: usize,
    #[serde(default = "default_database_min_pool_size")]
    pub min_pool_size: usize,
    #[serde(default = "default_pool_resize_threshold_high")]
    pub resize_threshold_high: f64,
    #[serde(default = "default_pool_resize_threshold_low")]
    pub resize_threshold_low: f64,
    #[serde(default = "default_pool_resize_step")]
    pub pool_resize_step: usize,
}

fn default_database_pool_size() -> usize {
    16
}

fn default_database_min_pool_size() -> usize {
    4
}

fn default_pool_resize_threshold_high() -> f64 {
    75.0
}

fn default_pool_resize_threshold_low() -> f64 {
    25.0
}

fn default_pool_resize_step() -> usize {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub expiration_hours: i64,
    pub refresh_expiration_hours: i64,
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
    // Optional server-side secret used to sign as fee-payer (fee sponsorship / account abstraction)
    #[serde(default)]
    pub fee_payer_secret: Option<String>,
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
    #[serde(default = "default_compliance_alert_webhook")]
    pub alert_webhook_url: Option<String>,
    #[serde(default)]
    pub ml_config: MLComplianceConfig,
    #[serde(default)]
    pub behavioral_config: BehavioralAnalysisConfig,
    #[serde(default)]
    pub case_management_enabled: bool,
    #[serde(default)]
    pub multiple_sanctions_providers: Vec<SanctionsProviderConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MLComplianceConfig {
    pub enabled: bool,
    pub model_version: String,
    pub model_endpoint: Option<String>,
    pub confidence_threshold: f64, // 0-1.0, minimum confidence to trust ML predictions
    pub behavioral_weight: f64,    // weight in final risk score calculation
    pub network_weight: f64,
    pub geographic_weight: f64,
    pub temporal_weight: f64,
    pub device_weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BehavioralAnalysisConfig {
    pub enabled: bool,
    pub transaction_frequency_threshold: f64, // transactions per day
    pub amount_deviation_threshold: f64,      // standard deviations
    pub geographic_anomaly_threshold: f64,    // 0-1.0
    pub time_pattern_threshold: f64,          // 0-1.0
    pub device_anomaly_threshold: f64,        // 0-1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SanctionsProviderConfig {
    pub provider_type: String, // "ofac", "un", "eu", "fca"
    pub enabled: bool,
    pub api_url: String,
    pub api_key: String,
    pub priority: i32,
    pub timeout_seconds: u32,
}

fn default_compliance_alert_webhook() -> Option<String> {
    None
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservabilityConfig {
    pub service_name: String,
    pub sentry_dsn: Option<String>,
    pub alert_webhook_url: Option<String>,
    pub log_retention_days: u16,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            service_name: "blinks-backend".to_string(),
            sentry_dsn: None,
            alert_webhook_url: None,
            log_retention_days: 30,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub redis_url: String,
    pub default_ttl_seconds: u64,
    pub hot_data_ttl_seconds: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            redis_url: "redis://localhost:6379".to_string(),
            default_ttl_seconds: 300,
            hot_data_ttl_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub backend: StorageBackend,
    pub local_path: Option<String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            local_path: Some("./uploads".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
    #[default]
    Local,
    S3,
    Ipfs,
}

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let mut builder = ConfigBuilder::builder()
            .add_source(File::with_name("config/default").required(false))
            .add_source(
                Environment::with_prefix("BLINKS")
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
                url: "postgres://localhost/BLINKS".to_string(),
                max_pool_size: 16,
                min_pool_size: 4,
                resize_threshold_high: 75.0,
                resize_threshold_low: 25.0,
                pool_resize_step: 2,
            },
            server: ServerConfig { port: 3000 },
            jwt: JwtConfig {
                secret: "change-this-in-production".to_string(),
                expiration_hours: 1,
                refresh_expiration_hours: 168, // 7 days
            },
            stellar_network: StellarNetwork {
                passphrase: "Test SDF Network ; September 2015".to_string(),
                horizon_url: "https://horizon-testnet.stellar.org".to_string(),
                rpc_url: "https://soroban-testnet.stellar.org".to_string(),
                network_id: "Test SDF Network ; September 2015".to_string(),
                fee_payer_secret: None,
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
                alert_webhook_url: None,
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
                ml_config: MLComplianceConfig {
                    enabled: true,
                    model_version: "v1.0".to_string(),
                    model_endpoint: None,
                    confidence_threshold: 0.7,
                    behavioral_weight: 0.25,
                    network_weight: 0.20,
                    geographic_weight: 0.20,
                    temporal_weight: 0.15,
                    device_weight: 0.20,
                },
                behavioral_config: BehavioralAnalysisConfig {
                    enabled: true,
                    transaction_frequency_threshold: 10.0, // transactions per day
                    amount_deviation_threshold: 3.0,       // 3 standard deviations
                    geographic_anomaly_threshold: 0.7,
                    time_pattern_threshold: 0.7,
                    device_anomaly_threshold: 0.7,
                },
                case_management_enabled: true,
                multiple_sanctions_providers: vec![
                    SanctionsProviderConfig {
                        provider_type: "ofac".to_string(),
                        enabled: true,
                        api_url: "https://api.ofac.example.com".to_string(),
                        api_key: "ofac-key".to_string(),
                        priority: 1,
                        timeout_seconds: 5,
                    },
                    SanctionsProviderConfig {
                        provider_type: "un".to_string(),
                        enabled: true,
                        api_url: "https://api.un-sanctions.example.com".to_string(),
                        api_key: "un-key".to_string(),
                        priority: 2,
                        timeout_seconds: 5,
                    },
                ],
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
            },
            rate_limit: RateLimitConfig {
                window_ms: 60000, // 1 minute
                max_requests: 100,
                scope: RateLimitScope::Ip,
                endpoint_limits: vec![],
                bypass_admin: true,
            },
            observability: ObservabilityConfig::default(),
            cache: CacheConfig::default(),
            storage: StorageConfig::default(),
        }
    }
}
