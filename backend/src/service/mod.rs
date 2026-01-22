pub mod anchor_service;
pub mod audit_service;
pub mod bridge_service;
pub mod compliance_service;
pub mod identity_service;
pub mod indexer_service;
pub mod payment_service;

pub use anchor_service::AnchorService;
pub use audit_service::AuditService;
pub use bridge_service::BridgeService;
pub use compliance_service::ComplianceService;
pub use identity_service::IdentityService;
pub use indexer_service::IndexerService;
pub use payment_service::PaymentService;

use crate::config::Config;
use deadpool_postgres::Pool;
use std::sync::Arc;

#[derive(Clone)]
pub struct ServiceContainer {
    pub identity: IdentityService,
    pub payment: PaymentService,
    pub bridge: BridgeService,
    pub anchor: AnchorService,
    pub compliance: ComplianceService,
    pub audit: AuditService,
    pub indexer: IndexerService,
    pub config: Config,
    pub db_pool: Arc<Pool>,
}

impl ServiceContainer {
    pub async fn new(db_pool: Pool, config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let db_pool = Arc::new(db_pool);

        let identity = IdentityService::new(db_pool.clone(), config.clone());
        let payment = PaymentService::new(db_pool.clone(), config.clone());
        let bridge = BridgeService::new(db_pool.clone(), config.clone());
        let anchor = AnchorService::new(db_pool.clone(), config.clone());
        let compliance = ComplianceService::new(db_pool.clone(), config.clone());
        let audit = AuditService::new(db_pool.clone(), config.clone());
        let indexer = IndexerService::new(db_pool.clone(), config.clone());

        Ok(Self {
            identity,
            payment,
            bridge,
            anchor,
            compliance,
            audit,
            indexer,
            config,
            db_pool,
        })
    }
}
