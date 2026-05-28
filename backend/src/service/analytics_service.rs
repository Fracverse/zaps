use crate::config::Config;
use crate::models::Payment;
use chrono::{DateTime, Duration, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;

/// Payment analytics for a time period
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentAnalytics {
    pub total_payments: i64,
    pub total_amount: i64,
    pub successful_payments: i64,
    pub failed_payments: i64,
    pub success_rate: f64,
    pub average_payment_amount: i64,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
}

/// Merchant performance dashboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantPerformance {
    pub merchant_id: String,
    pub total_revenue: i64,
    pub total_payments: i64,
    pub success_rate: f64,
    pub avg_payment_value: i64,
    pub top_payment_methods: Vec<PaymentMethodStats>,
    pub daily_trends: Vec<DailyTrend>,
}

/// Payment method statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethodStats {
    pub method: String,
    pub count: i64,
    pub total_amount: i64,
}

/// Daily trend data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyTrend {
    pub date: DateTime<Utc>,
    pub payment_count: i64,
    pub total_amount: i64,
}

/// Custom report request
#[derive(Debug, Clone, Deserialize)]
pub struct CustomReportRequest {
    pub merchant_id: Option<String>,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub group_by: Option<String>,
    pub include_details: bool,
}

/// Analytics service for payment analytics
#[derive(Clone)]
#[allow(dead_code)]
pub struct AnalyticsService {
    db_pool: Arc<Pool>,
    config: Config,
}

impl AnalyticsService {
    pub fn new(db_pool: Pool, config: Config) -> Self {
        AnalyticsService {
            db_pool: Arc::new(db_pool),
            config,
        }
    }

    /// Get payment analytics for a time period
    pub async fn get_payment_analytics(
        &self,
        merchant_id: Option<String>,
        start_date: DateTime<Utc>,
        end_date: DateTime<Utc>,
    ) -> Result<PaymentAnalytics, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        let merchant_id_ref = merchant_id.as_ref();

        let query = if merchant_id_ref.is_some() {
            "SELECT COUNT(*) as total_payments,
                    COALESCE(SUM(send_amount), 0) as total_amount,
                    COUNT(*) FILTER (WHERE status = 'completed') as successful_payments,
                    COUNT(*) FILTER (WHERE status = 'failed') as failed_payments
             FROM payments
             WHERE merchant_id = $1 AND created_at BETWEEN $2 AND $3"
        } else {
            "SELECT COUNT(*) as total_payments,
                    COALESCE(SUM(send_amount), 0) as total_amount,
                    COUNT(*) FILTER (WHERE status = 'completed') as successful_payments,
                    COUNT(*) FILTER (WHERE status = 'failed') as failed_payments
             FROM payments
             WHERE created_at BETWEEN $1 AND $2"
        };

        let row = if let Some(mid) = merchant_id_ref {
            client.query_one(query, &[mid, &start_date, &end_date]).await?
        } else {
            client.query_one(query, &[&start_date, &end_date]).await?
        };

        let total_payments: i64 = row.get("total_payments");
        let total_amount: i64 = row.get("total_amount");
        let successful_payments: i64 = row.get("successful_payments");
        let failed_payments: i64 = row.get("failed_payments");

        let success_rate = if total_payments > 0 {
            (successful_payments as f64 / total_payments as f64) * 100.0
        } else {
            0.0
        };

        let average_payment_amount = if total_payments > 0 {
            total_amount / total_payments
        } else {
            0
        };

        Ok(PaymentAnalytics {
            total_payments,
            total_amount,
            successful_payments,
            failed_payments,
            success_rate,
            average_payment_amount,
            period_start: start_date,
            period_end: end_date,
        })
    }

    /// Get merchant performance dashboard
    pub async fn get_merchant_performance(
        &self,
        merchant_id: String,
        days: i64,
    ) -> Result<MerchantPerformance, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;
        let end_date = Utc::now();
        let start_date = end_date - Duration::days(days);

        // Get basic stats
        let analytics = self.get_payment_analytics(Some(merchant_id.clone()), start_date, end_date).await?;

        // Get top payment methods (placeholder - would need payment_method column)
        let top_payment_methods = vec![
            PaymentMethodStats {
                method: "Stellar".to_string(),
                count: analytics.total_payments,
                total_amount: analytics.total_amount,
            },
        ];

        // Get daily trends
        let daily_trends_query = "SELECT DATE_TRUNC('day', created_at) as date,
                                         COUNT(*) as payment_count,
                                         COALESCE(SUM(send_amount), 0) as total_amount
                                  FROM payments
                                  WHERE merchant_id = $1 AND created_at BETWEEN $2 AND $3
                                  GROUP BY DATE_TRUNC('day', created_at)
                                  ORDER BY date DESC";

        let rows = client.query(daily_trends_query, &[&merchant_id, &start_date, &end_date]).await?;
        let mut daily_trends = Vec::new();

        for row in rows {
            daily_trends.push(DailyTrend {
                date: row.get("date"),
                payment_count: row.get("payment_count"),
                total_amount: row.get("total_amount"),
            });
        }

        Ok(MerchantPerformance {
            merchant_id,
            total_revenue: analytics.total_amount,
            total_payments: analytics.total_payments,
            success_rate: analytics.success_rate,
            avg_payment_value: analytics.average_payment_amount,
            top_payment_methods,
            daily_trends,
        })
    }

    /// Generate custom report
    pub async fn generate_custom_report(
        &self,
        request: CustomReportRequest,
    ) -> Result<Vec<Payment>, Box<dyn std::error::Error>> {
        let client = self.db_pool.get().await?;

        let query = if let Some(_merchant_id) = &request.merchant_id {
            "SELECT id, tx_hash, from_address, merchant_id, send_asset, send_amount,
                    receive_amount, status, memo, created_at, updated_at
             FROM payments
             WHERE merchant_id = $1 AND created_at BETWEEN $2 AND $3
             ORDER BY created_at DESC"
        } else {
            "SELECT id, tx_hash, from_address, merchant_id, send_asset, send_amount,
                    receive_amount, status, memo, created_at, updated_at
             FROM payments
             WHERE created_at BETWEEN $1 AND $2
             ORDER BY created_at DESC"
        };

        let rows = if let Some(merchant_id) = &request.merchant_id {
            client.query(query, &[&merchant_id, &request.start_date, &request.end_date]).await?
        } else {
            client.query(query, &[&request.start_date, &request.end_date]).await?
        };

        let mut payments = Vec::new();

        for row in rows {
            payments.push(Payment {
                id: row.get("id"),
                tx_hash: row.get("tx_hash"),
                from_address: row.get("from_address"),
                merchant_id: row.get("merchant_id"),
                send_asset: row.get("send_asset"),
                send_amount: row.get("send_amount"),
                receive_amount: row.get("receive_amount"),
                status: crate::models::PaymentStatus::from_str(&row.get::<_, String>("status")).unwrap(),
                memo: row.get("memo"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(payments)
    }

    /// Export data to CSV (placeholder)
    pub async fn export_to_csv(
        &self,
        request: CustomReportRequest,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let payments = self.generate_custom_report(request).await?;

        let mut csv = String::from("id,merchant_id,amount,status,created_at\n");
        for payment in payments {
            csv.push_str(&format!(
                "{},{},{},{},{}\n",
                payment.id,
                payment.merchant_id,
                payment.send_amount,
                payment.status,
                payment.created_at.to_rfc3339()
            ));
        }

        Ok(csv)
    }
}
