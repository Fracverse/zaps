use crate::{api_error::ApiError, config::Config, service::CacheService};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Currency {
    USD,
    EUR,
    GBP,
    JPY,
}

impl Currency {
    pub fn as_str(&self) -> &'static str {
        match self {
            Currency::USD => "USD",
            Currency::EUR => "EUR",
            Currency::GBP => "GBP",
            Currency::JPY => "JPY",
        }
    }
}

impl FromStr for Currency {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "USD" => Ok(Currency::USD),
            "EUR" => Ok(Currency::EUR),
            "GBP" => Ok(Currency::GBP),
            "JPY" => Ok(Currency::JPY),
            _ => Err(anyhow::anyhow!("Invalid currency: {}", s)),
        }
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct CurrencyService {
    db_pool: Arc<Pool>,
    config: Config,
    cache_service: CacheService,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionResult {
    pub from_currency: Currency,
    pub to_currency: Currency,
    pub from_amount: i64,
    pub raw_to_amount: i64,
    pub fee_amount: i64,
    pub to_amount: i64,
    pub rate: f64,
}

impl CurrencyService {
    pub fn new(db_pool: Arc<Pool>, config: Config, cache_service: CacheService) -> Self {
        Self {
            db_pool,
            config,
            cache_service,
        }
    }

    /// Get exchange rate between two currencies
    pub async fn get_exchange_rate(
        &self,
        from_currency: Currency,
        to_currency: Currency,
    ) -> Result<ExchangeRate, ApiError> {
        let from = from_currency.to_string();
        let to = to_currency.to_string();

        if from == to {
            return Ok(ExchangeRate {
                id: Uuid::new_v4().to_string(),
                from_currency: from,
                to_currency: to,
                rate: 1.0,
                source: Some("identity".to_string()),
                last_updated: chrono::Utc::now(),
            });
        }

        let cache_key = format!("exchange_rate:{}:{}", from, to);
        if let Ok(Some(cached_rate)) = self.cache_service.get_json::<ExchangeRate>(&cache_key).await {
            let now = chrono::Utc::now();
            let age_secs = now.signed_duration_since(cached_rate.last_updated).num_seconds();
            if age_secs > self.config.currency.max_rate_age_seconds as i64 {
                return Err(ApiError::BadRequest(format!(
                    "Exchange rate for {}/{} is stale (last updated {} seconds ago, limit is {} seconds)",
                    from, to, age_secs, self.config.currency.max_rate_age_seconds
                )));
            }
            return Ok(cached_rate);
        }

        let client = self.db_pool.get().await?;

        let row = client
            .query_opt(
                r#"
                SELECT id, from_currency, to_currency, rate, source, last_updated
                FROM exchange_rates
                WHERE from_currency = $1 AND to_currency = $2
                "#,
                &[&from, &to],
            )
            .await?
            .ok_or_else(|| {
                ApiError::NotFound(format!(
                    "Exchange rate not found for {}/{}",
                    from, to
                ))
            })?;

        let rate = ExchangeRate {
            id: row.get(0),
            from_currency: row.get(1),
            to_currency: row.get(2),
            rate: row.get(3),
            source: row.get(4),
            last_updated: row.get(5),
        };

        let now = chrono::Utc::now();
        let age_secs = now.signed_duration_since(rate.last_updated).num_seconds();
        if age_secs > self.config.currency.max_rate_age_seconds as i64 {
            return Err(ApiError::BadRequest(format!(
                "Exchange rate for {}/{} is stale in database (last updated {} seconds ago, limit is {} seconds)",
                from, to, age_secs, self.config.currency.max_rate_age_seconds
            )));
        }

        let _ = self
            .cache_service
            .set_json(
                &cache_key,
                &rate,
                Some(self.config.currency.max_rate_age_seconds),
            )
            .await;

        Ok(rate)
    }

    /// Convert amount with fee calculation
    pub async fn convert_with_fee(
        &self,
        amount: i64,
        from: Currency,
        to: Currency,
    ) -> Result<ConversionResult, ApiError> {
        let rate = self.get_exchange_rate(from, to).await?;
        let raw_to_amount = ((amount as f64 * rate.rate).round()) as i64;
        let fee_amount = if from == to {
            0
        } else {
            ((raw_to_amount as f64 * (self.config.currency.conversion_fee_bps as f64 / 10000.0)).round()) as i64
        };
        let to_amount = raw_to_amount - fee_amount;

        Ok(ConversionResult {
            from_currency: from,
            to_currency: to,
            from_amount: amount,
            raw_to_amount,
            fee_amount,
            to_amount,
            rate: rate.rate,
        })
    }

    /// Convert amount from one currency to another
    pub async fn convert(
        &self,
        amount: i64,
        from: Currency,
        to: Currency,
    ) -> Result<i64, ApiError> {
        let result = self.convert_with_fee(amount, from, to).await?;
        Ok(result.to_amount)
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

        let rate = ExchangeRate {
            id: row.get(0),
            from_currency: row.get(1),
            to_currency: row.get(2),
            rate: row.get(3),
            source: row.get(4),
            last_updated: row.get(5),
        };

        // Invalidate the cache key
        let cache_key = format!("exchange_rate:{}:{}", rate.from_currency, rate.to_currency);
        let _ = self.cache_service.invalidate(&cache_key).await;

        Ok(rate)
    }

    /// Get all supported currencies
    pub fn get_supported_currencies(&self) -> Vec<Currency> {
        vec![Currency::USD, Currency::EUR, Currency::GBP, Currency::JPY]
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
