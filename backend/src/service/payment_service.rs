use crate::{
    api_error::ApiError,
    config::Config,
    models::{Merchant, Payment, PaymentStatus},
};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct PaymentService {
    db_pool: Arc<Pool>,
    config: Config,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePaymentRequest {
    pub merchant_id: String,
    pub send_asset: String,
    pub send_amount: i64,
    pub min_receive: Option<i64>,
    pub memo: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct QrPaymentPayload {
    pub merchant_id: String,
    pub amount: i64,
    pub asset: String,
    pub memo: Option<String>,
    pub expiry: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NfcPaymentPayload {
    pub merchant_id: String,
    pub amount: i64,
    pub asset: String,
    pub memo: Option<String>,
    pub timestamp: i64,
}

impl PaymentService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    pub async fn create_payment(
        &self,
        from_address: String,
        request: CreatePaymentRequest,
    ) -> Result<Payment, ApiError> {
        let client = self.db_pool.get().await?;

        // Validate merchant exists and is active
        let _merchant = self.get_merchant(&request.merchant_id).await?;

        // Generate transaction hash (in production, this would be from Stellar)
        let tx_hash = format!("tx_{}", Uuid::new_v4().simple());
        let payment_id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                r#"
                INSERT INTO payments (
                    id, tx_hash, from_address, merchant_id, send_asset,
                    send_amount, receive_amount, status, memo
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING id, tx_hash, from_address, merchant_id, send_asset,
                         send_amount, receive_amount, status, memo, created_at, updated_at
                "#,
                &[
                    &payment_id,
                    &tx_hash,
                    &from_address,
                    &request.merchant_id,
                    &request.send_asset,
                    &request.send_amount,
                    &request.min_receive,
                    &"pending".to_string(),
                    &request.memo,
                ],
            )
            .await?;

        Ok(Payment {
            id: row.get(0),
            tx_hash: row.get(1),
            from_address: row.get(2),
            merchant_id: row.get(3),
            send_asset: row.get(4),
            send_amount: row.get(5),
            receive_amount: row.get(6),
            status: PaymentStatus::Pending,
            memo: row.get(8),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(9),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(10),
        })
    }

    pub async fn get_payment(&self, payment_id: Uuid) -> Result<Payment, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                r#"
                SELECT id, tx_hash, from_address, merchant_id, send_asset,
                       send_amount, receive_amount, status, memo, created_at, updated_at
                FROM payments WHERE id = $1
                "#,
                &[&payment_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Payment not found".to_string()))?;

        Ok(Payment {
            id: row.get(0),
            tx_hash: row.get(1),
            from_address: row.get(2),
            merchant_id: row.get(3),
            send_asset: row.get(4),
            send_amount: row.get(5),
            receive_amount: row.get(6),
            status: PaymentStatus::from_str(row.get(7)).unwrap(),
            memo: row.get(8),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(9),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(10),
        })
    }

    pub async fn update_payment_status(
        &self,
        payment_id: Uuid,
        status: PaymentStatus,
        tx_hash: Option<String>,
    ) -> Result<(), ApiError> {
        let client = self.db_pool.get().await?;

        if let Some(hash) = tx_hash {
            client
                .execute(
                    "UPDATE payments SET status = $1, tx_hash = $2, updated_at = NOW() WHERE id = $3",
                    &[&status.to_string(), &hash, &payment_id],
                )
                .await?;
        } else {
            client
                .execute(
                    "UPDATE payments SET status = $1, updated_at = NOW() WHERE id = $2",
                    &[&status.to_string(), &payment_id],
                )
                .await?;
        }

        Ok(())
    }

    pub async fn generate_qr_payment(
        &self,
        payload: crate::http::payments::QrPaymentRequest,
    ) -> Result<String, ApiError> {
        // Validate merchant exists
        self.get_merchant(&payload.merchant_id).await?;

        // In production, this would generate a proper SEP-7 URI
        let qr_data = format!(
            "zaps://pay?merchant={}&amount={}&asset={}&expiry={}&memo={}",
            payload.merchant_id,
            payload.amount,
            payload.asset,
            payload.expiry,
            payload.memo.unwrap_or_default()
        );

        Ok(qr_data)
    }

    pub async fn validate_nfc_payment(
        &self,
        payload: crate::http::payments::NfcPaymentRequest,
    ) -> Result<bool, ApiError> {
        // Validate merchant exists
        self.get_merchant(&payload.merchant_id).await?;

        // Basic validation - in production, check expiry, signature, etc.
        let current_time = chrono::Utc::now().timestamp();
        if payload.timestamp > current_time + 300 {
            // 5 minutes grace
            return Ok(false);
        }

        Ok(true)
    }

    async fn get_merchant(&self, merchant_id: &str) -> Result<Merchant, ApiError> {
        let client = self.db_pool.get().await?;

        let row = client
            .query_one(
                "SELECT id, merchant_id, vault_address, settlement_asset, active, created_at, updated_at FROM merchants WHERE merchant_id = $1 AND active = true",
                &[&merchant_id],
            )
            .await
            .map_err(|_| ApiError::NotFound("Merchant not found or inactive".to_string()))?;

        Ok(Merchant {
            id: row.get(0),
            merchant_id: row.get(1),
            vault_address: row.get(2),
            settlement_asset: row.get(3),
            active: row.get(4),
            created_at: row.get::<_, chrono::DateTime<chrono::Utc>>(5),
            updated_at: row.get::<_, chrono::DateTime<chrono::Utc>>(6),
        })
    }
}
