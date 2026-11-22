//! #1221 — Secure circle share links and mobile deep links.
//!
//! Raw tokens are high-entropy, single-use-at-creation values. Only their
//! SHA-256 hash is persisted, so a database leak cannot be replayed as a link.

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// 244 bits of entropy from two UUIDv4 values (OS-backed RNG).
pub fn generate_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.trim().as_bytes()))
}

/// Append an access-log entry. Logging failures are surfaced to the caller via
/// `Result` so a denied/allowed access is never silently unlogged.
pub async fn log_access(
    pool: &PgPool,
    link_id: Uuid,
    accessor: Option<&str>,
    user_agent: Option<&str>,
    ip_address: Option<&str>,
    outcome: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO circle_share_access_logs
            (link_id, accessor, user_agent, ip_address, outcome)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(link_id)
    .bind(accessor)
    .bind(user_agent)
    .bind(ip_address)
    .bind(outcome)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_hashing_is_stable_and_does_not_return_the_token() {
        let token = generate_token();
        assert_eq!(token.len(), 64);
        assert_ne!(token, hash_token(&token));
        assert_eq!(hash_token(&token), hash_token(&token));
        assert_ne!(generate_token(), token);
    }
}
