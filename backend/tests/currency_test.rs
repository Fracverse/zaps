use blinks_backend::config::Config;
use blinks_backend::service::{Currency, CurrencyService, CacheService};
use blinks_backend::service::currency_service::UpdateExchangeRateRequest;
use blinks_backend::db;
use std::sync::Arc;

#[tokio::test]
async fn test_identity_conversion() {
    let config = Config::default();
    let cache = CacheService::new(config.clone()).await;
    let pool_res = db::create_pool("postgresql://invalid_host:5432/invalid_db").await;
    if let Ok(pool) = pool_res {
        let service = CurrencyService::new(Arc::new(pool), config, cache);
        let result = service.convert_with_fee(1000, Currency::USD, Currency::USD).await;
        assert!(result.is_ok());
        let res = result.unwrap();
        assert_eq!(res.from_amount, 1000);
        assert_eq!(res.raw_to_amount, 1000);
        assert_eq!(res.fee_amount, 0);
        assert_eq!(res.to_amount, 1000);
        assert_eq!(res.rate, 1.0);
    }
}

#[tokio::test]
#[ignore]
async fn test_currency_service_full() {
    let config = match Config::load() {
        Ok(c) => c,
        Err(_) => return,
    };

    // Try to connect to DB
    let pool = match db::create_pool(&config.database.url).await {
        Ok(p) => p,
        Err(_) => return,
    };

    // Test DB connection
    let client_res = pool.get().await;
    let client = match client_res {
        Ok(c) => c,
        Err(_) => return,
    };

    let cache = CacheService::new(config.clone()).await;
    let service = CurrencyService::new(Arc::new(pool), config.clone(), cache.clone());

    // Clean up or prepare test data
    let _ = client.execute("DELETE FROM exchange_rates WHERE from_currency = 'USD' AND to_currency = 'EUR'", &[]).await;

    // Create a new rate via update_exchange_rate
    let rate = service.update_exchange_rate(UpdateExchangeRateRequest {
        from_currency: "USD".to_string(),
        to_currency: "EUR".to_string(),
        rate: 0.9,
        source: Some("test".to_string()),
    }).await.expect("Failed to insert rate");

    assert_eq!(rate.rate, 0.9);

    // Test conversion fee calculation
    // 10000 USD * 0.9 = 9000 EUR
    // Fee = 9000 * 0.5% (50 bps) = 45 EUR
    // Converted = 8955 EUR
    let conv = service.convert_with_fee(10000, Currency::USD, Currency::EUR).await.expect("Conversion failed");
    assert_eq!(conv.raw_to_amount, 9000);
    assert_eq!(conv.fee_amount, 45);
    assert_eq!(conv.to_amount, 8955);

    // Test caching: update database rate directly to 0.95
    // Because it is cached at 0.9 in Redis, the service should still return 0.9
    client.execute("UPDATE exchange_rates SET rate = 0.95 WHERE from_currency = 'USD' AND to_currency = 'EUR'", &[]).await.unwrap();

    let rate_cached = service.get_exchange_rate(Currency::USD, Currency::EUR).await.unwrap();
    assert_eq!(rate_cached.rate, 0.9);

    // Test cache invalidation via update_exchange_rate: update rate to 0.96
    // This should invalidate the cache, so next fetch returns 0.96
    service.update_exchange_rate(UpdateExchangeRateRequest {
        from_currency: "USD".to_string(),
        to_currency: "EUR".to_string(),
        rate: 0.96,
        source: Some("test_update".to_string()),
    }).await.unwrap();

    let rate_updated = service.get_exchange_rate(Currency::USD, Currency::EUR).await.unwrap();
    assert_eq!(rate_updated.rate, 0.96);

    // Test staleness protection: update database rate last_updated to be old
    let old_time = chrono::Utc::now() - chrono::Duration::seconds(400); // age 400s > limit 300s
    client.execute("UPDATE exchange_rates SET last_updated = $1 WHERE from_currency = 'USD' AND to_currency = 'EUR'", &[&old_time]).await.unwrap();

    // Since we updated the database, the cached value 0.96 is still in cache. Let's invalidate cache to force DB reload
    let cache_key = format!("exchange_rate:USD:EUR");
    let _ = cache.invalidate(&cache_key).await;

    let rate_stale_res = service.get_exchange_rate(Currency::USD, Currency::EUR).await;
    assert!(rate_stale_res.is_err(), "Expected stale rate to fail");
}
