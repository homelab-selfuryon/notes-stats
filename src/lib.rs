//! notes-stats: statistics over notes-api.
//!
//! The app calls notes-api with its own projected ServiceAccount token (audience `notes-api`),
//! re-read from disk on every call because the kubelet rotates it. There are no shared API keys.
//!
//! - [`config`]: non-secret settings from the environment
//! - [`notes_api`]: the client
//! - [`routes`]: `/v1/stats`; `/healthz` and `/readyz` for the kubelet

pub mod config;
pub mod notes_api;
pub mod routes;
pub mod state;

pub use state::AppState;
