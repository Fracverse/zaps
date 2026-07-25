use axum::{
    routing::{delete, get, post},
    Router,
};

pub mod auth;
pub mod bridge;
pub mod feed;
pub mod social;
pub mod user;
pub mod r#yield;

pub fn auth_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/challenge", get(auth::get_challenge))
        .route("/verify", post(auth::verify_signature))
        .with_state(pool)
}

pub fn user_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route(
            "/profile",
            get(user::get_profile).post(user::update_profile),
        )
        .route("/search", get(user::search_users))
        .route("/friends", get(user::list_friends))
        .route("/friends/request", post(user::send_friend_request))
        .route("/friends/:id/accept", post(user::accept_friend_request))
        .route("/friends/:id/reject", post(user::reject_friend_request))
        .with_state(pool)
}

pub fn feed_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/public", get(feed::get_public_feed))
        .route("/friends", get(feed::get_friends_feed))
        .route("/private", get(feed::get_private_feed))
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

pub fn bridge_routes(state: bridge::BridgeState) -> Router {
    Router::new()
        .route("/quote", post(bridge::get_quote))
        .route("/tx", post(bridge::submit_bridge_tx))
        .route("/status/:id", get(bridge::get_bridge_status))
        .with_state(state)
}

pub fn yield_routes(pool: sqlx::PgPool) -> Router {
    Router::new()
        .route("/balance", get(r#yield::get_balance))
        .route("/history", get(r#yield::get_history))
        .route("/deposit", post(r#yield::deposit))
        .route("/withdraw", post(r#yield::withdraw))
        .route("/toggle-auto", post(r#yield::toggle_auto_earn))
        .with_state(pool)
}
