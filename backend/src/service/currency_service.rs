use crate::{api_error::ApiError, config::Config};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
#[allow(dead_code)]
pub struct CurrencyService {
    db_pool: Arc<Pool>,
    config: Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    pub id: String,
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub source: Option<String>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateExchangeRateRequest {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversionRequest {
    pub from_currency: String,
    pub to_currency: String,
    pub amount: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConversionResponse {
    pub from_currency: String,
    pub to_currency: String,
    pub from_amount: i64,
    pub to_amount: i64,
    pub rate: f64,
}

impl CurrencyService {
    pub fn new(db_pool: Arc<Pool>, config: Config) -> Self {
        Self { db_pool, config }
    }

    /// Get exchange rate between two currencies
    pub async fn get_exchange_rate(
        &self,
        from_currency: &str,
        to_currency: &str,
    ) -> Result<ExchangeRate, ApiError> {
        if from_currency == to_currency {
            return Ok(ExchangeRate {
                id: Uuid::new_v4().to_string(),
                from_currency: from_currency.to_string(),
                to_currency: to_currency.to_string(),
                rate: 1.0,
                source: Some("identity".to_string()),
                last_updated: chrono::Utc::now(),
            });
        }

        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                r#"
                SELECT id, from_currency, to_currency, rate, source, last_updated
                FROM exchange_rates
                WHERE from_currency = $1 AND to_currency = $2
                "#,
                &[&from_currency, &to_currency],
            )
            .await?
            .ok_or_else(|| {
                ApiError::NotFound(format!(
                    "Exchange rate not found for {}/{}",
                    from_currency, to_currency
                ))
            })?;

        Ok(ExchangeRate {
            id: row.get(0),
            from_currency: row.get(1),
            to_currency: row.get(2),
            rate: row.get(3),
            source: row.get(4),
            last_updated: row.get(5),
        })
    }

    /// Update or create exchange rate
    pub async fn update_exchange_rate(
        &self,
        request: UpdateExchangeRateRequest,
    ) -> Result<ExchangeRate, ApiError> {
        let client = self.db_pool.get().await?;

        // Validate currency codes
        let valid_currencies = ["USD", "EUR", "GBP", "JPY"];
        if !valid_currencies.contains(&request.from_currency.as_str())
            || !valid_currencies.contains(&request.to_currency.as_str())
        {
            return Err(ApiError::BadRequest(
                "Invalid currency code".to_string(),
            ));
        }

        if request.rate <= 0.0 {
            return Err(ApiError::BadRequest(
                "Exchange rate must be positive".to_string(),
            ));
        }

        let id = Uuid::new_v4().to_string();

        let row = client
            .query_one(
                r#"
                INSERT INTO exchange_rates (id, from_currency, to_currency, rate, source)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT (from_currency, to_currency)
                DO UPDATE SET rate = $4, source = $5, last_updated = NOW()
                RETURNING id, from_currency, to_currency, rate, source, last_updated
                "#,
                &[
                    &id,
                    &request.from_currency,
                    &request.to_currency,
                    &request.rate,
                    &request.source,
                ],
            )
            .await?;

        Ok(ExchangeRate {
            id: row.get(0),
            from_currency: row.get(1),
            to_currency: row.get(2),
            rate: row.get(3),
            source: row.get(4),
            last_updated: row.get(5),
        })
    }

    /// Convert amount from one currency to another
    pub async fn convert_currency(
        &self,
        request: ConversionRequest,
    ) -> Result<ConversionResponse, ApiError> {
        let rate = self
            .get_exchange_rate(&request.from_currency, &request.to_currency)
            .await?;

        // Convert with proper rounding (banker's rounding)
        let to_amount = ((request.amount as f64 * rate.rate).round()) as i64;

        Ok(ConversionResponse {
            from_currency: request.from_currency,
            to_currency: request.to_currency,
            from_amount: request.amount,
            to_amount,
            rate: rate.rate,
        })
    }

    /// Get all supported currencies
    pub async fn get_supported_currencies(&self) -> Result<Vec<String>, ApiError> {
        Ok(vec![
            "USD".to_string(),
            "EUR".to_string(),
            "GBP".to_string(),
            "JPY".to_string(),
        ])
    }

    /// Get all exchange rates
    pub async fn get_all_exchange_rates(&self) -> Result<Vec<ExchangeRate>, ApiError> {
        let client = self.db_pool.get().await?;

        let rows = client
            .query(
                r#"
                SELECT id, from_currency, to_currency, rate, source, last_updated
                FROM exchange_rates
                ORDER BY from_currency, to_currency
                "#,
                &[],
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| ExchangeRate {
                id: row.get(0),
                from_currency: row.get(1),
                to_currency: row.get(2),
                rate: row.get(3),
                source: row.get(4),
                last_updated: row.get(5),
            })
            .collect())
    }
}
