use std::sync::Arc;
use deadpool_postgres::Pool;
use uuid::Uuid;
use crate::{config::Config, models::UserProfile};

#[derive(Clone)]
pub struct ProfileService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl ProfileService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    pub async fn create_profile(
        &self,
        user_id: Uuid,
        display_name: String,
        avatar_url: Option<String>,
        bio: Option<String>,
        country: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<UserProfile, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        
        let stmt = client
            .prepare(
                "INSERT INTO user_profiles (user_id, display_name, avatar_url, bio, country, metadata) 
                 VALUES ($1, $2, $3, $4, $5, $6) 
                 RETURNING id, user_id, display_name, avatar_url, bio, country, metadata, created_at, updated_at",
            )
            .await?;

        let row = client
            .query_one(
                &stmt,
                &[
                    &user_id,
                    &display_name,
                    &avatar_url,
                    &bio,
                    &country,
                    &metadata,
                ],
            )
            .await?;

        Ok(UserProfile {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            display_name: row.get(2),
            avatar_url: row.get(3),
            bio: row.get(4),
            country: row.get(5),
            metadata: row.get(6),
            created_at: row.get(7),
            updated_at: row.get(8),
        })
    }

    pub async fn get_profile(&self, user_id: Uuid) -> Result<Option<UserProfile>, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        
        let stmt = client
            .prepare("SELECT id, user_id, display_name, avatar_url, bio, country, metadata, created_at, updated_at FROM user_profiles WHERE user_id = $1")
            .await?;

        let row = client.query_opt(&stmt, &[&user_id]).await?;

        match row {
            Some(row) => Ok(Some(UserProfile {
                id: row.get::<_, Uuid>(0).to_string(),
                user_id: row.get::<_, Uuid>(1).to_string(),
                display_name: row.get(2),
                avatar_url: row.get(3),
                bio: row.get(4),
                country: row.get(5),
                metadata: row.get(6),
                created_at: row.get(7),
                updated_at: row.get(8),
            })),
            None => Ok(None),
        }
    }

    pub async fn update_profile(
        &self,
        user_id: Uuid,
        display_name: Option<String>,
        avatar_url: Option<String>,
        bio: Option<String>,
        country: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<UserProfile, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        
        // Build dynamic query
        let mut query = String::from("UPDATE user_profiles SET ");
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync>> = Vec::new();
        let mut param_idx = 1;

        if let Some(dn) = display_name {
            query.push_str(&format!("display_name = ${}, ", param_idx));
            params.push(Box::new(dn));
            param_idx += 1;
        }

        if let Some(au) = avatar_url {
            query.push_str(&format!("avatar_url = ${}, ", param_idx));
            params.push(Box::new(au));
            param_idx += 1;
        }

        if let Some(b) = bio {
            query.push_str(&format!("bio = ${}, ", param_idx));
            params.push(Box::new(b));
            param_idx += 1;
        }

        if let Some(c) = country {
            query.push_str(&format!("country = ${}, ", param_idx));
            params.push(Box::new(c));
            param_idx += 1;
        }

        if let Some(m) = metadata {
            query.push_str(&format!("metadata = ${}, ", param_idx));
            params.push(Box::new(m));
            param_idx += 1;
        }

        // Remove trailing comma and space
        if query.ends_with(", ") {
            query.truncate(query.len() - 2);
        } else {
            // Nothing to update
            return self.get_profile(user_id).await.map(|opt| opt.ok_or("Profile not found".into())).transpose().unwrap();
        }

        query.push_str(&format!(" WHERE user_id = ${} RETURNING id, user_id, display_name, avatar_url, bio, country, metadata, created_at, updated_at", param_idx));
        params.push(Box::new(user_id));

        let stmt = client.prepare(&query).await?;
        
        // Convert params to slice of references
        let params_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params.iter().map(|p| p.as_ref()).collect();

        let row = client.query_one(&stmt, &params_refs).await?;

        Ok(UserProfile {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            display_name: row.get(2),
            avatar_url: row.get(3),
            bio: row.get(4),
            country: row.get(5),
            metadata: row.get(6),
            created_at: row.get(7),
            updated_at: row.get(8),
        })
    }

    pub async fn delete_profile(&self, user_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        let stmt = client.prepare("DELETE FROM user_profiles WHERE user_id = $1").await?;
        client.execute(&stmt, &[&user_id]).await?;
        Ok(())
    }
}
