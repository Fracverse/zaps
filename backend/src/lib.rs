pub mod api_error;
pub mod app;
pub mod auth;
pub mod config;
pub mod db;
pub mod http;
pub mod middleware;
pub mod models;
// pub mod realtime; // TODO: Implement when needed
pub mod service;
pub mod telemetry;

pub use api_error::ApiError;
pub use app::create_app;
pub use config::Config;
