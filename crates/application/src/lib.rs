//! Orchestration layer between the HTTP API and the `db`/`economy-core`
//! boundaries. Deliberately minimal for BACKEND-01: only the readiness
//! use-case exists so far. Business use-cases (auth, market, contracts)
//! are out of scope until a dedicated BACKEND task adds them.

mod health;

pub use db::DatabaseError;
pub use health::check_readiness;
