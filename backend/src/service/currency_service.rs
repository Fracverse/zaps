use crate::config::Config;
use deadpool_postgres::Pool;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Supported currencies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug)]
pub struct ParseCurrencyError;

impl std::fmt::Display for ParseCurrencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "invalid currency code")
    }
}

impl std::error::Error for ParseCurrencyError {}

impl FromStr for Currency {
    type Err = ParseCurrencyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "USD" => Ok(Currency::USD),
            "EUR" => Ok(Currency::EUR),
            "GBP" => Ok(Currency::GBP),
            "JPY" => Ok(Currency::JPY),
            _ => Err(ParseCurrencyError),
        }
    }
}

/// Exchange rate between two currencies
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExchangeRate {
    pub from_currency: Currency,
    pub to_currency: Currency,
    pub rate: f64,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

/// Currency service for managing exchange rates and conversions
#[derive(Clone)]
#[allow(dead_code)]
pub struct CurrencyService {
    db_pool: Arc<Pool>,
    config: Config,
    exchange_rates: Arc<RwLock<HashMap<(Currency, Currency), ExchangeRate>>>,
}

impl CurrencyService {
    pub fn new(db_pool: Pool, config: Config) -> Self {
        let exchange_rates = Arc::new(RwLock::new(HashMap::new()));
        CurrencyService {
            db_pool: Arc::new(db_pool),
            config,
            exchange_rates,
        }
    }

    /// Convert amount from one currency to another
    pub async fn convert(
        &self,
        amount: i64,
        from: Currency,
        to: Currency,
    ) -> Result<i64, Box<dyn std::error::Error>> {
        if from == to {
            return Ok(amount);
        }

        let rate = self.get_exchange_rate(from, to).await?;
        let converted = (amount as f64) * rate.rate;
        Ok(converted.round() as i64)
    }

    /// Get exchange rate between two currencies
    pub async fn get_exchange_rate(
        &self,
        from: Currency,
        to: Currency,
    ) -> Result<ExchangeRate, Box<dyn std::error::Error>> {
        // First check in-memory cache
        {
            let rates = self.exchange_rates.read().await;
            if let Some(rate) = rates.get(&(from, to)) {
                // Check if rate is fresh (less than 1 hour old)
                if rate.last_updated > chrono::Utc::now() - chrono::Duration::hours(1) {
                    return Ok(rate.clone());
                }
            }
        }

        // If not in cache or stale, fetch from database or external provider
        let rate = self.fetch_exchange_rate(from, to).await?;

        // Update cache
        {
            let mut rates = self.exchange_rates.write().await;
            rates.insert((from, to), rate.clone());
        }

        Ok(rate)
    }

    /// Fetch exchange rate from database or external provider (placeholder)
    async fn fetch_exchange_rate(
        &self,
        from: Currency,
        to: Currency,
    ) -> Result<ExchangeRate, Box<dyn std::error::Error>> {
        // TODO: Implement actual exchange rate provider integration
        // For now, return mock rates
        let mock_rates = vec![
            (Currency::USD, Currency::EUR, 0.92),
            (Currency::USD, Currency::GBP, 0.79),
            (Currency::USD, Currency::JPY, 150.0),
            (Currency::EUR, Currency::USD, 1.09),
            (Currency::EUR, Currency::GBP, 0.86),
            (Currency::EUR, Currency::JPY, 163.0),
            (Currency::GBP, Currency::USD, 1.27),
            (Currency::GBP, Currency::EUR, 1.16),
            (Currency::GBP, Currency::JPY, 190.0),
            (Currency::JPY, Currency::USD, 0.0067),
            (Currency::JPY, Currency::EUR, 0.0061),
            (Currency::JPY, Currency::GBP, 0.0053),
        ];

        let rate = mock_rates
            .into_iter()
            .find(|(f, t, _)| *f == from && *t == to)
            .ok_or_else(|| format!("Exchange rate not found for {from:?} to {to:?}"))?;

        Ok(ExchangeRate {
            from_currency: from,
            to_currency: to,
            rate: rate.2,
            last_updated: chrono::Utc::now(),
        })
    }

    /// Get all supported currencies
    pub fn get_supported_currencies(&self) -> Vec<Currency> {
        vec![Currency::USD, Currency::EUR, Currency::GBP, Currency::JPY]
    }
}
