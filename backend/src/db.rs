use deadpool_postgres::{Config, Pool, Runtime};
use tokio_postgres::NoTls;

pub type DbPool = Pool;

pub async fn create_pool(database_url: &str) -> Result<DbPool, Box<dyn std::error::Error>> {
    let mut cfg = Config::new();
    // Parse the URL to set config fields
    if let Ok(config) = database_url.parse::<tokio_postgres::Config>() {
            cfg.user = config.get_user().map(|s| s.to_string());
            cfg.password = config.get_password().map(|s| String::from_utf8_lossy(s).to_string());
            cfg.dbname = config.get_dbname().map(|s| s.to_string());
        cfg.host = config.get_hosts().first().map(|h| match h {
            tokio_postgres::config::Host::Tcp(s) => s.to_string(),
            tokio_postgres::config::Host::Unix(s) => s.to_string_lossy().to_string(),
        });
        cfg.port = config.get_ports().first().copied();
    }
    
    let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
    Ok(pool)
}

pub async fn run_migrations(pool: &DbPool) -> Result<(), Box<dyn std::error::Error>> {
    let client = pool.get().await?;

    // Create users table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS users (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            user_id VARCHAR(255) UNIQUE NOT NULL,
            stellar_address VARCHAR(56) UNIQUE NOT NULL,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
            &[],
        )
        .await?;

    // Create merchants table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS merchants (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            merchant_id VARCHAR(255) UNIQUE NOT NULL,
            vault_address VARCHAR(56) NOT NULL,
            settlement_asset VARCHAR(56) NOT NULL,
            active BOOLEAN DEFAULT true,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
            &[],
        )
        .await?;

    // Create payments table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS payments (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tx_hash VARCHAR(64) UNIQUE,
            from_address VARCHAR(56) NOT NULL,
            merchant_id VARCHAR(255) NOT NULL,
            send_asset VARCHAR(56) NOT NULL,
            send_amount BIGINT NOT NULL,
            receive_amount BIGINT,
            status VARCHAR(50) DEFAULT 'pending',
            memo TEXT,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            FOREIGN KEY (merchant_id) REFERENCES merchants(merchant_id)
        )
        "#,
            &[],
        )
        .await?;

    // Create transfers table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS transfers (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tx_hash VARCHAR(64) UNIQUE,
            from_user_id VARCHAR(255) NOT NULL,
            to_user_id VARCHAR(255) NOT NULL,
            amount BIGINT NOT NULL,
            asset VARCHAR(56) NOT NULL,
            status VARCHAR(50) DEFAULT 'pending',
            memo TEXT,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            FOREIGN KEY (from_user_id) REFERENCES users(user_id),
            FOREIGN KEY (to_user_id) REFERENCES users(user_id)
        )
        "#,
            &[],
        )
        .await?;

    // Create withdrawals table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS withdrawals (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            tx_hash VARCHAR(64) UNIQUE,
            user_id VARCHAR(255) NOT NULL,
            destination_address VARCHAR(100) NOT NULL,
            amount BIGINT NOT NULL,
            asset VARCHAR(56) NOT NULL,
            status VARCHAR(50) DEFAULT 'pending',
            anchor_tx_id VARCHAR(255),
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            FOREIGN KEY (user_id) REFERENCES users(user_id)
        )
        "#,
            &[],
        )
        .await?;

    // Create balances table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS balances (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            owner_id VARCHAR(255) NOT NULL,
            asset VARCHAR(56) NOT NULL,
            amount BIGINT NOT NULL DEFAULT 0,
            last_updated TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            UNIQUE(owner_id, asset)
        )
        "#,
            &[],
        )
        .await?;

    // Create audit_logs table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS audit_logs (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            event_type VARCHAR(100) NOT NULL,
            ref_id UUID NOT NULL,
            user_id VARCHAR(255),
            details JSONB,
            ip_address INET,
            user_agent TEXT,
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
        )
        "#,
            &[],
        )
        .await?;

    // Create indexes for better performance
    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_payments_merchant_id ON payments(merchant_id)",
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_payments_status ON payments(status)",
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_transfers_from_user ON transfers(from_user_id)",
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_transfers_to_user ON transfers(to_user_id)",
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_logs_event_type ON audit_logs(event_type)",
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at ON audit_logs(created_at)",
            &[],
        )
        .await?;

    // Create bridge_transactions table
    client
        .execute(
            r#"
        CREATE TABLE IF NOT EXISTS bridge_transactions (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            from_chain VARCHAR(50) NOT NULL,
            to_chain VARCHAR(50) NOT NULL,
            asset VARCHAR(20) NOT NULL,
            amount BIGINT NOT NULL,
            destination_address VARCHAR(100) NOT NULL,
            user_id VARCHAR(255) NOT NULL,
            status VARCHAR(50) DEFAULT 'pending',
            tx_hash VARCHAR(100),
            created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
            FOREIGN KEY (user_id) REFERENCES users(user_id)
        )
        "#,
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_bridge_transactions_user_id ON bridge_transactions(user_id)",
            &[],
        )
        .await?;

    client
        .execute(
            "CREATE INDEX IF NOT EXISTS idx_bridge_transactions_status ON bridge_transactions(status)",
            &[],
        )
        .await?;

    Ok(())
}