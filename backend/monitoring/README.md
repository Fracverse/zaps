# BLINKS Backend Monitoring

This folder contains production monitoring primitives for the Rust backend.

- `prometheus.yml` scrapes `GET /metrics`.
- `alerts.yml` defines critical service, error-rate, rate-limit, and compliance alerts.
- `grafana-dashboard.json` provides an importable dashboard for HTTP, business, cache, rate-limit, compliance, and database pool metrics.

Recommended log aggregation settings:

- Run with `LOG_FORMAT=json` and `RUST_LOG=blinks_backend=info,tower_http=info`.
- Ship stdout to Elasticsearch, Loki, Datadog, or another JSON log backend.
- Retain application logs for `BLINKS_OBSERVABILITY__LOG_RETENTION_DAYS` days, defaulting to 30.

Recommended error tracking:

- Configure `BLINKS_OBSERVABILITY__SENTRY_DSN` in production.
- Forward structured `error`, `warn`, and alert events from the log pipeline to Sentry or PagerDuty.
