use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::{
    api_error::ApiError,
    service::{
        currency_service::{ConversionRequest, ConversionResponse, UpdateExchangeRateRequest},
        ServiceContainer,
    },
};

#[derive(Debug, Serialize)]
pub struct ExchangeRateResponse {
    pub from_currency: String,
    pub to_currency: String,
    pub rate: f64,
    pub source: Option<String>,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct SupportedCurrenciesResponse {
    pub currencies: Vec<String>,
}

/// Get exchange rate between two currencies
pub async fn get_exchange_rate(
    State(services): State<Arc<ServiceContainer>>,
    Path((from, to)): Path<(String, String)>,
) -> Result<Json<ExchangeRateResponse>, ApiError> {
    let rate = services
        .currency
        .get_exchange_rate(&from.to_uppercase(), &to.to_uppercase())
        .await?;

    Ok(Json(ExchangeRateResponse {
        from_currency: rate.from_currency,
        to_currency: rate.to_currency,
        rate: rate.rate,
        source: rate.source,
        last_updated: rate.last_updated,
    }))
}

/// Update exchange rate
pub async fn update_exchange_rate(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<UpdateExchangeRateRequest>,
) -> Result<Json<ExchangeRateResponse>, ApiError> {
    let rate = services
        .currency
        .update_exchange_rate(UpdateExchangeRateRequest {
            from_currency: request.from_currency.to_uppercase(),
            to_currency: request.to_currency.to_uppercase(),
            rate: request.rate,
            source: request.source,
        })
        .await?;

    // Log audit event
    let _ = services
        .audit
        .log_admin_action(
            "system",
            "exchange_rate_updated",
            "exchange_rate",
            Some(format!("{}_{}", rate.from_currency, rate.to_currency)),
            Some(serde_json::json!({
                "rate": rate.rate,
                "source": rate.source
            })),
            None,
            None,
        )
        .await;

    Ok(Json(ExchangeRateResponse {
        from_currency: rate.from_currency,
        to_currency: rate.to_currency,
        rate: rate.rate,
        source: rate.source,
        last_updated: rate.last_updated,
    }))
}

/// Convert amount from one currency to another
pub async fn convert_currency(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<ConversionRequest>,
) -> Result<Json<ConversionResponse>, ApiError> {
    let result = services
        .currency
        .convert_currency(ConversionRequest {
            from_currency: request.from_currency.to_uppercase(),
            to_currency: request.to_currency.to_uppercase(),
            amount: request.amount,
        })
        .await?;

    Ok(Json(result))
}

/// Get all supported currencies
pub async fn get_supported_currencies(
    State(services): State<Arc<ServiceContainer>>,
) -> Result<Json<SupportedCurrenciesResponse>, ApiError> {
    let currencies = services.currency.get_supported_currencies().await?;

    Ok(Json(SupportedCurrenciesResponse { currencies }))
}

/// Get all exchange rates
pub async fn get_all_exchange_rates(
    State(services): State<Arc<ServiceContainer>>,
) -> Result<Json<Vec<ExchangeRateResponse>>, ApiError> {
    let rates = services.currency.get_all_exchange_rates().await?;

    Ok(Json(
        rates
            .into_iter()
            .map(|r| ExchangeRateResponse {
                from_currency: r.from_currency,
                to_currency: r.to_currency,
                rate: r.rate,
                source: r.source,
                last_updated: r.last_updated,
            })
            .collect(),
    ))
}
