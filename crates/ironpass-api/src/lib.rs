mod db;
mod error;
mod routes;
mod state;

pub mod server;

pub use ironpass_api_client::models;
pub use server::{app, default_state, serve};

/// Test helpers for integration tests. Not part of the stable public API.
#[cfg(any(test, feature = "test-helpers"))]
pub mod test_helpers {
    pub use crate::db::DbPool;
    pub use crate::state::AppState;
}
