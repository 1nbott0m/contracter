//! Orchestration layer between the HTTP API and the `db`/`economy-core`
//! boundaries. Holds the use-cases: readiness (BACKEND-01),
//! authentication/account (BACKEND-02), and catalog/inventory reads
//! (BACKEND-03). Market and contract use-cases are out of scope until a
//! dedicated BACKEND task adds them.

pub mod auth;
pub mod catalog;
mod health;
pub mod inventory;
pub mod market;
pub mod pagination;
pub mod quote;
pub mod quote_signing;

pub use health::{ReadinessError, check_readiness};
