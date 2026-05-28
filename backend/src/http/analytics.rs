use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use std::sync::Arc;

use crate::models::Payment;
use crate::service::{
    CustomReportRequest, MerchantPerformance, PaymentAnalytics, ServiceContainer,
};

#[derive(Debug, Deserialize)]
pub struct AnalyticsQuery {
    pub merchant_id: Option<String>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
}

/// Get payment analytics
pub async fn get_payment_analytics(
    State(services): State<Arc<ServiceContainer>>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<PaymentAnalytics>, (StatusCode, String)> {
    let end_date = query.end_date.unwrap_or_else(Utc::now);
    let start_date = query.start_date.unwrap_or_else(|| end_date - Duration::days(30));

    let analytics = services
        .analytics
        .get_payment_analytics(query.merchant_id, start_date, end_date)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(analytics))
}

/// Get merchant performance dashboard
pub async fn get_merchant_performance(
    State(services): State<Arc<ServiceContainer>>,
    Path(merchant_id): Path<String>,
    Query(query): Query<MerchantPerformanceQuery>,
) -> Result<Json<MerchantPerformance>, (StatusCode, String)> {
    let days = query.days.unwrap_or(30);

    let performance = services
        .analytics
        .get_merchant_performance(merchant_id, days)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(performance))
}

#[derive(Debug, Deserialize)]
pub struct MerchantPerformanceQuery {
    pub days: Option<i64>,
}

/// Generate custom report
pub async fn generate_custom_report(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CustomReportRequest>,
) -> Result<Json<Vec<Payment>>, (StatusCode, String)> {
    let payments = services
        .analytics
        .generate_custom_report(request)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(payments))
}

/// Export data to CSV
pub async fn export_to_csv(
    State(services): State<Arc<ServiceContainer>>,
    Json(request): Json<CustomReportRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let csv = services
        .analytics
        .export_to_csv(request)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, "text/csv")],
        csv,
    ))
}
