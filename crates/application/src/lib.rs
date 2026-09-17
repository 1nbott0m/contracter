//! Orchestration layer between the HTTP API and the `db`/`economy-core`
//! boundaries. Holds the use-cases: readiness (BACKEND-01) and
//! authentication/account (BACKEND-02). Market and contract use-cases are
//! out of scope until a dedicated BACKEND task adds them.

pub mod auth;
mod health;

pub use health::{ReadinessError, check_readiness};
