// lib.rs — re-exports the internal modules so that `tests/` integration
// tests can access them via `zaps_backend::api::feed::*`.
#![allow(dead_code, unused_variables, unused_imports)]

pub mod api;
pub mod config;
pub mod db;
pub mod indexer;
pub mod services;
