use crate::{
    api_error::ApiError,
    config::Config,
    models::{User, Wallet},
    role::Role,
};
use deadpool_postgres::Pool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct IdentityService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl IdentityService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    pub async fn create_user(&self, user_id: String) -> Result<User, ApiError> {
        let client = self.db_pool.get().await?;

        // Generate a unique Stellar address (in production, this would be generated properly)
        let stellar_address = format!("G{}", Uuid::new_v4().simple().to_string().to_uppercase());

        let role_str = Role::User.as_str();
        let row = client
            .query_one(
                "INSERT INTO users (user_id, stellar_address, role) VALUES ($1, $2, $3) RETURNING id, user_id, stellar_address, role, created_at, updated_at",
                &[&user_id, &stellar_address, &role_str],
            )
            .await?;

        Ok(User {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get(1),
            stellar_address: row.get(2),
            role: Role::from_str(row.get::<_, &str>(3)),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(4),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(5),
        })
    }

    pub async fn get_user_by_id(&self, user_id: &str) -> Result<User, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                "SELECT id, user_id, stellar_address, role, created_at, updated_at FROM users WHERE user_id = $1",
                &[&user_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("User not found".to_string()))?;

        Ok(User {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get(1),
            stellar_address: row.get(2),
            role: Role::from_str(row.get::<_, &str>(3)),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(4),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(5),
        })
    }

    pub async fn get_user_wallet(&self, user_id: &str) -> Result<Wallet, ApiError> {
        let user = self.get_user_by_id(user_id).await?;

        Ok(Wallet {
            user_id: user.user_id,
            address: user.stellar_address,
        })
    }

    pub async fn resolve_user_id(&self, user_id: &str) -> Result<String, ApiError> {
        let user = self.get_user_by_id(user_id).await?;
        Ok(user.stellar_address)
    }

    pub async fn user_exists(&self, user_id: &str) -> Result<bool, ApiError> {
        let client = self.db_pool.get().await?;

        let count: i64 = client
            .query_one("SELECT COUNT(*) FROM users WHERE user_id = $1", &[&user_id])
            .await?
            .get(0);

        Ok(count > 0)
    }
}
