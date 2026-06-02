pub mod anchor_service;
pub mod analytics_service;
pub mod audit_service;
pub mod batch_service;
pub mod bridge_service;
pub mod cache_service;
pub mod compliance_service;
pub mod currency_service;
pub mod dispute_service;
pub mod identity_service;
pub mod indexer_service;
pub mod metrics_service;
pub mod notification_service;
pub mod payment_service;
pub mod payout_service;
pub mod profile_service;
pub mod rate_limit_service;
pub mod reconciliation_service;
pub mod session_service;
pub mod soroban_service;
pub mod storage_service;
pub mod webhook_service;
pub mod schedule_service;

pub use anchor_service::AnchorService;
pub use analytics_service::{
    AnalyticsService, CustomReportRequest, DailyTrend, MerchantPerformance, PaymentAnalytics,
    PaymentMethodStats,
};
pub use audit_service::AuditService;
pub use batch_service::BatchService;
pub use bridge_service::BridgeService;
pub use cache_service::CacheService;
pub use compliance_service::ComplianceService;
pub use currency_service::{Currency, CurrencyService, ExchangeRate};
pub use dispute_service::DisputeService;
pub use identity_service::IdentityService;
pub use indexer_service::IndexerService;
pub use metrics_service::{
    AlertPayload, AlertSeverity, DetailedMetrics, MetricsPayload, MetricsService,
};
pub use notification_service::NotificationService;
pub use payment_service::PaymentService;
pub use payout_service::PayoutService;
pub use profile_service::ProfileService;
pub use rate_limit_service::RateLimitService;
pub use reconciliation_service::{
    Discrepancy, ExternalRecord, ReconciliationRequest, ReconciliationResult, ReconciliationService,
};
pub use session_service::SessionService;
pub use soroban_service::SorobanService;
pub use storage_service::StorageService;
pub use webhook_service::{WebhookDelivery, WebhookEndpoint, WebhookService};

use crate::config::Config;
use deadpool_postgres::Pool;
use std::sync::Arc;

#[derive(Clone)]
pub struct ServiceContainer {
    pub identity: IdentityService,
    pub payment: PaymentService,
    pub payout: PayoutService,
    pub dispute: DisputeService,
    pub bridge: BridgeService,
    pub anchor: AnchorService,
    pub compliance: ComplianceService,
    pub currency: CurrencyService,
    pub analytics: AnalyticsService,
    pub reconciliation: ReconciliationService,
    pub audit: AuditService,
    pub indexer: IndexerService,
    pub notification: NotificationService,
    pub rate_limit: RateLimitService,
    pub cache: CacheService,
    pub profile: ProfileService,
    pub soroban: SorobanService,
    pub storage: StorageService,
    pub batch: BatchService,
    pub session: SessionService,
    pub webhook: WebhookService,
    pub config: Config,
    pub db_pool: Arc<Pool>,
}

impl ServiceContainer {
    pub async fn new(db_pool: Pool, config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let db_pool = Arc::new(db_pool);

        let cache = CacheService::new(config.clone()).await;
        let identity = IdentityService::new(db_pool.clone(), config.clone());
        let payment = PaymentService::new(db_pool.clone(), config.clone());
        let payout = PayoutService::new(db_pool.clone(), config.clone());
        let dispute = DisputeService::new(db_pool.clone(), config.clone());
        let bridge = BridgeService::new(db_pool.clone(), config.clone());
        let anchor = AnchorService::new(db_pool.clone(), config.clone());
        let compliance = ComplianceService::new(db_pool.clone(), config.clone());
        let currency = CurrencyService::new(db_pool.clone(), config.clone(), cache.clone());
        let analytics = AnalyticsService::new((*db_pool).clone(), config.clone());
        let reconciliation = ReconciliationService::new((*db_pool).clone(), config.clone());
        let audit = AuditService::new(db_pool.clone(), config.clone());
        let indexer = IndexerService::new(db_pool.clone(), config.clone());
        let notification = NotificationService::new(db_pool.clone(), config.clone());
        let rate_limit = RateLimitService::new(config.clone()).await;
        let profile = ProfileService::new(db_pool.clone(), config.clone());
        let soroban = SorobanService::new(config.clone());
        let storage = StorageService::new(config.clone());
        let batch = BatchService::new(db_pool.clone(), config.clone());
        let session = SessionService::new(db_pool.clone(), config.clone());
        let webhook = WebhookService::new(db_pool.clone(), config.clone());

        Ok(Self {
            identity,
            payment,
            payout,
            dispute,
            bridge,
            anchor,
            compliance,
            currency,
            analytics,
            reconciliation,
            audit,
            indexer,
            notification,
            rate_limit,
            cache,
            profile,
            soroban,
            storage,
            batch,
            session,
            webhook,
            config,
            db_pool,
        })
    }
}
