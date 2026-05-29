use crate::{api_error::ApiError, config::Config, models::UserProfile};
use deadpool_postgres::Pool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProfileService {
    db_pool: Arc<Pool>,
    _config: Config,
}

impl ProfileService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self {
            db_pool,
            _config: config,
        }
    }

    pub async fn create_profile(
        &self,
        user_id: Uuid,
        display_name: String,
        avatar_url: Option<String>,
        bio: Option<String>,
        country: Option<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<UserProfile, ApiError> {
        let client = self.db_pool.get().await?;

        // Check if profile already exists to return a cleaner error?
        // Or let database constraint handle it and map the error.
        // For now, let's just run the INSERT and catch uniqueness violation if possible, or rely on calling check before.
        // But http handler does check.

        let stmt = client
            .prepare(
                "INSERT INTO user_profiles (user_id, display_name, avatar_url, bio, country, metadata) 
                 VALUES ($1, $2, $3, $4, $5, $6) 
                 RETURNING id, user_id, display_name, avatar_url, bio, country, metadata, verification_status, created_at, updated_at",
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
            .await
            .map_err(|e| {
                if let Some(db_error) = e.as_db_error() {
                    if db_error.code()
                        == &deadpool_postgres::tokio_postgres::error::SqlState::UNIQUE_VIOLATION
                    {
                        return ApiError::Conflict("Profile already exists".into());
                    }
                }
                ApiError::Database(e)
            })?;

        Ok(UserProfile {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            display_name: row.get(2),
            avatar_url: row.get(3),
            bio: row.get(4),
            country: row.get(5),
            metadata: row.get(6),
            verification_status: row.get(7),
            created_at: row.get(8),
            updated_at: row.get(9),
        })
    }

    pub async fn get_profile(&self, user_id: Uuid) -> Result<Option<UserProfile>, ApiError> {
        let client = self.db_pool.get().await?;

        let stmt = client
            .prepare("SELECT id, user_id, display_name, avatar_url, bio, country, metadata, verification_status, created_at, updated_at FROM user_profiles WHERE user_id = $1")
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
                verification_status: row.get(7),
                created_at: row.get(8),
                updated_at: row.get(9),
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
    ) -> Result<UserProfile, ApiError> {
        let client = self.db_pool.get().await?;

        // Build dynamic query
        let mut query = String::from("UPDATE user_profiles SET ");
        let mut params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = Vec::new();
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
            // Nothing to update, just return the profile
            return self
                .get_profile(user_id)
                .await?
                .ok_or(ApiError::NotFound("Profile not found".into()));
        }

        query.push_str(&format!(" WHERE user_id = ${} RETURNING id, user_id, display_name, avatar_url, bio, country, metadata, verification_status, created_at, updated_at", param_idx));
        params.push(Box::new(user_id));

        let stmt = client.prepare(&query).await?;

        // Convert params to slice of references
        let params_refs: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();

        let row = client
            .query_one(&stmt, &params_refs)
            .await
            .map_err(ApiError::Database)?;

        Ok(UserProfile {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            display_name: row.get(2),
            avatar_url: row.get(3),
            bio: row.get(4),
            country: row.get(5),
            metadata: row.get(6),
            verification_status: row.get(7),
            created_at: row.get(8),
            updated_at: row.get(9),
        })
    }

    pub async fn delete_profile(&self, user_id: Uuid) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;
        let stmt = client
            .prepare("DELETE FROM user_profiles WHERE user_id = $1")
            .await?;
        client.execute(&stmt, &[&user_id]).await?;
        Ok(())
    }
    /// Upload/update avatar for a user profile
    pub async fn update_avatar(
        &self,
        user_id: Uuid,
        avatar_url: String,
    ) -> Result<UserProfile, ApiError> {
        let client = self.db_pool.get().await?;
        let stmt = client
            .prepare(
                "UPDATE user_profiles SET avatar_url = 
}
 WHERE user_id = $2
                 RETURNING id, user_id, display_name, avatar_url, bio, country, metadata, verification_status, created_at, updated_at",
            )
            .await?;

        let row = client
            .query_one(&stmt, &[&avatar_url, &user_id])
            .await
            .map_err(|e| {
                ApiError::Database(e)
            })?;

        Ok(UserProfile {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            display_name: row.get(2),
            avatar_url: row.get(3),
            bio: row.get(4),
            country: row.get(5),
            metadata: row.get(6),
            verification_status: row.get(7),
            created_at: row.get(8),
            updated_at: row.get(9),
        })
    }

    /// Update profile verification status (admin only)
    pub async fn update_verification_status(
        &self,
        user_id: Uuid,
        status: &str,
    ) -> Result<UserProfile, ApiError> {
        let client = self.db_pool.get().await?;
        let stmt = client
            .prepare(
                "UPDATE user_profiles SET verification_status = 
}
 WHERE user_id = $2
                 RETURNING id, user_id, display_name, avatar_url, bio, country, metadata, verification_status, created_at, updated_at",
            )
            .await?;

        let row = client
            .query_one(&stmt, &[&status, &user_id])
            .await
            .map_err(|e| {
                ApiError::Database(e)
            })?;

        Ok(UserProfile {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            display_name: row.get(2),
            avatar_url: row.get(3),
            bio: row.get(4),
            country: row.get(5),
            metadata: row.get(6),
            verification_status: row.get(7),
            created_at: row.get(8),
            updated_at: row.get(9),
        })
    }

    /// Get user preferences
    pub async fn get_preferences(&self, user_id: Uuid) -> Result<Option<UserPreferences>, ApiError> {
        let client = self.db_pool.get().await?;
        let stmt = client
            .prepare(
                "SELECT id, user_id, preferences, created_at, updated_at FROM user_preferences WHERE user_id = 
}
",
            )
            .await?;

        let row = client.query_opt(&stmt, &[&user_id]).await?;

        match row {
            Some(row) => Ok(Some(UserPreferences {
                id: row.get::<_, Uuid>(0).to_string(),
                user_id: row.get::<_, Uuid>(1).to_string(),
                preferences: row.get(2),
                created_at: row.get(3),
                updated_at: row.get(4),
            })),
            None => Ok(None),
        }
    }

    /// Create or update user preferences
    pub async fn upsert_preferences(
        &self,
        user_id: Uuid,
        preferences: serde_json::Value,
    ) -> Result<UserPreferences, ApiError> {
        let client = self.db_pool.get().await?;
        let stmt = client
            .prepare(
                "INSERT INTO user_preferences (user_id, preferences)
                 VALUES (
}
, $2)
                 ON CONFLICT (user_id)
                 DO UPDATE SET preferences = EXCLUDED.preferences, updated_at = NOW()
                 RETURNING id, user_id, preferences, created_at, updated_at",
            )
            .await?;

        let row = client
            .query_one(&stmt, &[&user_id, &preferences])
            .await
            .map_err(|e| ApiError::Database(e))?;

        Ok(UserPreferences {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            preferences: row.get(2),
            created_at: row.get(3),
            updated_at: row.get(4),
        })
    }

    /// Get profile activity history (paginated)
    pub async fn get_activity(
        &self,
        user_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<ProfileActivity>, i64), ApiError> {
        let client = self.db_pool.get().await?;

        let count_stmt = client
            .prepare("SELECT COUNT(*) FROM profile_activity WHERE user_id = 
}
")
            .await?;
        let total: i64 = client
            .query_one(&count_stmt, &[&user_id])
            .await?
            .get(0);

        let stmt = client
            .prepare(
                "SELECT id, user_id, activity_type, description, metadata, created_at
                 FROM profile_activity WHERE user_id = 
}

                 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
            )
            .await?;

        let rows = client.query(&stmt, &[&user_id, &limit, &offset]).await?;

        let activities = rows
            .iter()
            .map(|row| ProfileActivity {
                id: row.get::<_, Uuid>(0).to_string(),
                user_id: row.get::<_, Uuid>(1).to_string(),
                activity_type: row.get(2),
                description: row.get(3),
                metadata: row.get(4),
                created_at: row.get(5),
            })
            .collect();

        Ok((activities, total))
    }

    /// Log a profile activity event
    pub async fn log_activity(
        &self,
        user_id: Uuid,
        activity_type: &str,
        description: Option<&str>,
        metadata: Option<serde_json::Value>,
    ) -> Result<ProfileActivity, ApiError> {
        let client = self.db_pool.get().await?;
        let stmt = client
            .prepare(
                "INSERT INTO profile_activity (user_id, activity_type, description, metadata)
                 VALUES (
}
, $2, $3, $4)
                 RETURNING id, user_id, activity_type, description, metadata, created_at",
            )
            .await?;

        let row = client
            .query_one(&stmt, &[&user_id, &activity_type, &description, &metadata])
            .await
            .map_err(|e| ApiError::Database(e))?;

        Ok(ProfileActivity {
            id: row.get::<_, Uuid>(0).to_string(),
            user_id: row.get::<_, Uuid>(1).to_string(),
            activity_type: row.get(2),
            description: row.get(3),
            metadata: row.get(4),
            created_at: row.get(5),
        })
    }
}