use axum::{
    middleware,
    routing::{delete, get, post, put},
    Router,
};

pub mod admin;
pub use admin::admin_routes;
pub mod auth;
pub mod bank_accounts;
pub mod circles;
pub mod circle_comments;
pub mod circle_contributions;
pub mod circle_announcements;
pub mod circle_rate_limit;
pub mod circle_idempotency;
pub mod circle_events;
pub mod circle_invitations;
pub mod auth_middleware;
pub mod bridge;
pub mod circles;
pub mod feed;
pub mod payouts;
pub mod push_notifications;
pub mod privy_jwks;
pub mod social;
pub mod transactions;
pub mod user;
pub use user as users;
pub mod r#yield;

// Re-export the middleware types used in main.rs so callers can import them
// from `zaps_backend::api::*` without needing the full module path.
pub use auth_middleware::{
    auth_middleware, spawn_cache_sweep, AuthMiddlewareState, AuthTokenCache, AuthenticatedUser,
};

/// Builds the auth router from an already-assembled `AuthState` (pool +
/// Privy JWKS client). Prefer this when the caller wants control over the
/// JWKS URL / app ID (e.g. tests pointing at a mock JWKS server).
///
/// Uses an in-process rate limiter; see `auth_routes_with_limiter` for the
/// Redis-backed one.
pub fn auth_routes_with_state(state: auth::AuthState) -> Router {
    auth_routes_with_limiter(state, auth::AuthRateLimiter::new())
}

/// #949: Auth router rate-limited by `limiter` (e.g. the Redis sliding
/// window from `AuthRateLimiter::from_redis_url`), with session refresh (#943).
pub fn auth_routes_with_limiter(state: auth::AuthState, limiter: auth::AuthRateLimiter) -> Router {
    Router::new()
        .route("/challenge", get(auth::get_challenge))
        .route("/verify", post(auth::verify_signature))
        .route("/privy", post(auth::privy_auth))
        .route("/refresh", post(auth::refresh_session))
        .with_state(state)
        .layer(middleware::from_fn_with_state(
            limiter,
            auth::auth_rate_limit,
        ))
}

/// Convenience wrapper that builds `AuthState` from `PRIVY_APP_ID` /
/// `PRIVY_JWKS_URL` env vars and the rate limiter from `REDIS_URL` (see
/// `config::Config`).
pub fn auth_routes(pool: sqlx::PgPool) -> Router {
    let config = crate::config::Config::from_env();
    auth_routes_with_limiter(
        auth::AuthState {
            pool,
            privy: std::sync::Arc::new(privy_jwks::PrivyJwksClient::new(config.privy_jwks_url)),
            privy_app_id: config.privy_app_id,
            cache: None,
        },
        auth::AuthRateLimiter::from_redis_url(config.redis_url.as_deref()),
    )
}

/// #561 — Session Refresh & Auth Middleware
///
/// Wraps the supplied `router` with `auth_middleware` so that every route it
/// contains requires a valid `Authorization: Bearer <token>` header.
///
/// Validated tokens are cached for up to 5 minutes in `cache` to avoid a DB
/// round-trip on every request.  See `auth_middleware::AuthTokenCache` for the
/// TTL and eviction semantics.
///
/// Bearer tokens are verified against dynamically fetched Privy JWKS, with
/// fallback to local JWT validation for legacy tokens.
///
/// # Usage (in main.rs)
/// ```rust
/// let auth_cache = api::AuthTokenCache::new();
/// let privy_jwks = api::privy_jwks::PrivyJwksClient::new(jwks_url);
/// let protected = api::protected_routes(
///     Router::new()
///         .nest("/api/feed",   api::feed_routes(pool.clone()))
///         .nest("/api/social", api::social_routes(pool.clone())),
///     pool.clone(),
///     auth_cache,
///     privy_jwks,
///     privy_app_id,
/// );
/// ```
pub fn protected_routes(
    router: Router,
    pool: sqlx::PgPool,
    cache: AuthTokenCache,
    privy: std::sync::Arc<privy_jwks::PrivyJwksClient>,
    privy_app_id: String,
) -> Router {
    router.layer(middleware::from_fn_with_state(
        AuthMiddlewareState {
            pool,
            cache,
            privy,
            privy_app_id,
        },
        auth_middleware,
    ))
}

/// User routes without a Redis cache; username resolution falls through to Postgres.
pub fn user_routes(pool: sqlx::PgPool) -> Router {
    user_routes_with_state(user::UserState::new(pool, None))
}

/// #544: User routes wired to the Redis username->address cache.
pub fn user_routes_with_state(state: user::UserState) -> Router {
    Router::new()
        .route(
            "/profile",
            get(user::get_profile)
                .post(user::update_profile)
                .put(user::update_profile),
        )
        .route("/profile/avatar", post(user::upload_avatar))
        .route("/search", get(user::search_users))
        .route("/suggestions", get(user::suggest_usernames))
        .route("/autocomplete", get(user::autocomplete))
        .route("/friends", get(user::list_friends))
        .route("/friends/request", post(user::send_friend_request))
        .route("/friends/:id/accept", post(user::accept_friend_request))
        .route("/friends/:id/reject", post(user::reject_friend_request))
        .route("/resolve/:username", get(user::resolve_address))
        .route("/did/:did", get(user::get_user_by_did))
        .with_state(state)
}

pub fn feed_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/public", get(feed::get_public_feed))
        .route("/friends", get(feed::get_friends_feed))
        .route("/private", get(feed::get_private_feed))
        .with_state(pool)
}

pub fn circle_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/:id", get(circles::get_circle))
        .route("/:id/join", post(circles::join_circle))
        .with_state(pool)
}

pub fn registry_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/claims", get(user::get_registry_claims))
        .route("/stats", get(user::get_registry_stats))
        .with_state(pool)
}

/// #543 / #935
///
/// Takes `BridgeState` (rather than a bare pool) so the batch-upload handlers
/// — which need the shared bridge state — can live on the same router as the
/// rest of the payout endpoints; `payouts::*` handlers keep extracting a
/// plain `State<PgPool>` via the `FromRef<BridgeState> for PgPool` impl in
/// `bridge.rs`.
pub fn payout_routes(state: bridge::BridgeState) -> Router {
    Router::new()
        .route("/username", post(feed::payout_by_username))
        .route("/batches", get(payouts::list_batches))
        // #935: POST /api/payouts/batch accepts a JSON array of payouts;
        // /batch/csv accepts the same data as a multipart CSV upload.
        .route("/batch", post(bridge::batch_upload))
        .route("/batch/csv", post(bridge::batch_upload_csv))
        .route("/batch/:id", get(payouts::get_batch_detail))
        .route("/batch/:id/export", get(payouts::export_batch))
        // #728 — block transfers to sanctioned addresses before processing.
        .layer(middleware::from_fn(
            auth_middleware::compliance_sanitize_middleware,
        ))
        .with_state(state)
}

/// #957 — Public SDP webhook receiver route (authenticated via HMAC signature, not user JWT)
pub fn payout_webhook_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/sdp/webhook", post(payouts::sdp_reconciliation_webhook))
        .with_state(pool)
}

pub fn social_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/like", post(social::like_payment))
        .route("/unlike", delete(social::unlike_payment))
        .route("/comment", post(social::add_comment))
        .route("/comment/:id", delete(social::delete_comment))
        .with_state(pool)
}

pub fn transaction_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/history", get(transactions::get_history))
        .route("/history/:id", get(transactions::get_history_detail))
        .with_state(pool)
}

pub fn bridge_routes(state: bridge::BridgeState) -> Router {
    Router::new()
        .route("/quote", post(bridge::get_quote))
        .route("/tx", post(bridge::submit_bridge_tx))
        .route("/status/:id", get(bridge::get_bridge_status))
        .with_state(state)
}

/// Yield routes without a Redis cache; reads fall through to Postgres.
pub fn yield_routes(pool: sqlx::PgPool) -> Router {
    yield_routes_with_state(r#yield::YieldState::new(pool, None))
}

/// BE-061: Yield routes wired to the Redis cache the indexer evicts from.
pub fn yield_routes_with_state(state: r#yield::YieldState) -> Router {
    Router::new()
        .route("/balance", get(r#yield::get_balance))
        .route("/metrics", get(r#yield::get_metrics))
        .route("/history", get(r#yield::get_history))
        .route("/rates/history", get(r#yield::get_rate_history))
        .route("/deposit", post(r#yield::deposit))
        .route("/withdraw", post(r#yield::withdraw))
        .route("/toggle-auto", post(r#yield::toggle_auto_earn))
        // #549 — Unsigned Transaction XDR Generator
        // These routes construct unsigned Soroban transaction envelopes for
        // client-side signing without touching the database.
        .route("/build/deposit", post(r#yield::build_unsigned_deposit))
        .route("/build/withdraw", post(r#yield::build_unsigned_withdraw))
        .with_state(state)
}

/// Circles API routes (#1193-#1202, #1204, #1219).
pub fn circle_routes(pool: sqlx::PgPool) -> Router {
    let state = circles::CirclesState::new(pool.clone());
    // #1223: user-scoped, configurable limits with stricter financial buckets.
    let limiter = circle_rate_limit::CircleRateLimiter::from_env();
    Router::new()
        .route("/", post(circles::create_circle).get(circles::list_circles))
        // Static endpoints registered before /:id to prevent UUID parse collisions
        .route("/discover", get(circles::discover_circles))
        .route("/search", get(circles::search_circles))
        // #1220 — authorized user search for circle invitations
        .route("/users/search", get(circles::search_users_for_circle))
        // #1221 — secure share links / deep links (static before /:id)
        .route("/share", post(circles::create_share_link))
        .route("/share/:token", get(circles::resolve_share_link))
        .route("/share/:id/revoke", post(circles::revoke_share_link))
        // #1217 — notification inbox (static paths before /:id)
        .route("/notifications", get(circles::list_notifications))
        .route(
            "/notifications/unread-count",
            get(circles::notification_unread_count),
        )
        .route(
            "/notifications/read-all",
            post(circles::mark_all_notifications_read),
        )
        .route(
            "/notifications/:id/read",
            post(circles::mark_notification_read),
        )
        // #1199 / #1200 — invitation inbox and responses (static before /:id)
        .route("/invitations/mine", get(circle_invitations::my_invitations))
        .route(
            "/invitations/redeem",
            post(circle_invitations::redeem_invitation),
        )
        .route(
            "/invitations/:inv_id/accept",
            post(circle_invitations::accept_invitation),
        )
        .route(
            "/invitations/:inv_id/decline",
            post(circle_invitations::decline_invitation),
        )
        .route("/:id", get(circles::get_circle))
        .route("/:id/join", post(circles::join_circle))
        .route("/:id/leave", post(circles::leave_circle))
        // #1225: authenticated SSE subscription for circle members.
        .route("/:id/events", get(circle_events::circle_event_stream))
        // #1214 — activity comments (member write, author/moderator moderation).
        .route(
            "/:id/activities/:activity_id/comments",
            get(circle_comments::list_comments).post(circle_comments::create_comment),
        )
        .route(
            "/:id/comments/:comment_id",
            put(circle_comments::update_comment).delete(circle_comments::delete_comment),
        )
        .route("/:id/eligibility", get(circles::check_eligibility))
        .route(
            "/:id/membership/history",
            get(circles::get_membership_history),
        )
        .route(
            "/:id/members/requests/:req_id/review",
            post(circles::review_membership_request),
        )
        .route("/:id/roles", get(circles::list_roles))
        // #1216 — circle announcements (moderator publish, member history).
        .route(
            "/:id/announcements",
            get(circle_announcements::list_announcements)
                .post(circle_announcements::create_announcement),
        )
        .route(
            "/:id/announcements/:announcement_id",
            get(circle_announcements::get_announcement)
                .put(circle_announcements::update_announcement)
                .delete(circle_announcements::delete_announcement),
        )
        // #1222 — permission-controlled, append-only audit trail
        .route("/:id/audit", get(circles::list_audit_log))
        .route("/:id/audit/verify", get(circles::verify_audit_log))
        // #1203 — versioned contribution rules; #1205 / #1207 / #1208 —
        // initiate, verify and list contributions.
        .route(
            "/:id/contribution-rules",
            get(circle_contributions::get_contribution_rules)
                .put(circle_contributions::update_contribution_rules),
        )
        .route(
            "/:id/contribution-rules/versions",
            get(circle_contributions::list_contribution_rule_versions),
        )
        .route(
            "/:id/contributions",
            post(circle_contributions::initiate_contribution)
                .get(circle_contributions::list_contributions),
        )
        .route(
            "/:id/contributions/:contribution_id/verify",
            post(circle_contributions::verify_contribution),
        )
        .route("/:id/payout/:req_id/execute", post(circles::execute_payout))
        .route(
            "/:id/members/:address/role",
            put(circles::update_member_role),
        )
        .route(
            "/:id/reminders/preferences",
            get(circles::get_reminder_preferences).put(circles::update_reminder_preferences),
        )
        .route(
            "/:id/reminders/trigger",
            post(circles::trigger_contribution_reminders),
        )
        .route("/:id/contribute", post(circles::contribute))
        .route("/:id/payout", post(circles::request_payout))
        .route("/:id/payout/:req_id", get(circles::get_payout_status))
        .route("/:id/payout/:req_id/approve", post(circles::approve_payout))
        .route(
            "/:id/members",
            get(circles::list_members).post(circles::add_member),
        )
        .route("/:id/members/:address", delete(circles::remove_member))
        .route(
            "/:id/invitations",
            get(circle_invitations::list_invitations).post(circle_invitations::create_invitation),
        )
        .route(
            "/:id/invitations/:inv_id",
            delete(circle_invitations::revoke_invitation),
        )
        .route(
            "/:id/settings",
            get(circles::get_circle_settings).patch(circles::update_circle_settings),
        )
        .route("/:id/goals", get(circles::list_goals).post(circles::create_goal))
        .route(
            "/:id/goals/:goal_id",
            get(circles::get_goal).patch(circles::update_goal),
        )
        .route("/:id/goals/:goal_id/complete", post(circles::complete_goal))
        // #1224: idempotent financial operations (inner layer, so the rate
        // limiter still sees every attempt).
        .layer(middleware::from_fn_with_state(
            pool.clone(),
            circle_idempotency::circle_idempotency,
        ))
        .layer(middleware::from_fn_with_state(
            limiter,
            circle_rate_limit::circle_rate_limit,
        ))
        .with_state(state)
}
