use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;

use crate::service::{Currency, ServiceContainer};

#[derive(Debug, Deserialize)]
pub struct ConvertQuery {
    amount: i64,
    from: String,
    to: String,
}

#[derive(Debug, Serialize)]
pub struct ConvertResponse {
    original_amount: i64,
    original_currency: Currency,
    converted_amount: i64,
    converted_currency: Currency,
    rate: f64,
}

#[derive(Debug, Serialize)]
pub struct CurrenciesResponse {
    currencies: Vec<CurrencyInfo>,
}

#[derive(Debug, Serialize)]
pub struct CurrencyInfo {
    code: String,
    name: String,
}

/// Convert an amount from one currency to another
pub async fn convert_currency(
    State(services): State<Arc<ServiceContainer>>,
    Query(query): Query<ConvertQuery>,
) -> Result<Json<ConvertResponse>, (StatusCode, String)> {
    let from = Currency::from_str(&query.from)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid 'from' currency".to_string()))?;
    let to = Currency::from_str(&query.to)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid 'to' currency".to_string()))?;

    let converted_amount = services
        .currency
        .convert(query.amount, from, to)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let rate = services
        .currency
        .get_exchange_rate(from, to)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ConvertResponse {
        original_amount: query.amount,
        original_currency: from,
        converted_amount,
        converted_currency: to,
        rate: rate.rate,
    }))
}

/// Get exchange rate between two currencies
pub async fn get_exchange_rate(
    State(services): State<Arc<ServiceContainer>>,
    Path((from, to)): Path<(String, String)>,
) -> Result<Json<crate::service::ExchangeRate>, (StatusCode, String)> {
    let from_currency = Currency::from_str(&from)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid 'from' currency".to_string()))?;
    let to_currency = Currency::from_str(&to)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid 'to' currency".to_string()))?;

    let rate = services
        .currency
        .get_exchange_rate(from_currency, to_currency)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(rate))
}

/// Get list of supported currencies
pub async fn get_supported_currencies(
    State(services): State<Arc<ServiceContainer>>,
) -> Json<CurrenciesResponse> {
    let currencies = services.currency.get_supported_currencies();
    let currency_info = currencies
        .into_iter()
        .map(|c| CurrencyInfo {
            code: c.as_str().to_string(),
            name: match c {
                Currency::USD => "US Dollar".to_string(),
                Currency::EUR => "Euro".to_string(),
                Currency::GBP => "British Pound".to_string(),
                Currency::JPY => "Japanese Yen".to_string(),
            },
        })
        .collect();

    Json(CurrenciesResponse {
        currencies: currency_info,
    })
}
