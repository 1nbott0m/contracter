//! Axum HTTP layer for Contracter. Routing, extractors, responses,
//! middleware, and HTTP-level error mapping live here. No PostgreSQL or
//! SQLx types are defined here -- `db` stays the only PostgreSQL
//! boundary, referenced only through `application` (business logic) or,
//! for infrastructure wiring like `AppState`, directly for its plain
//! `Database` handle.

mod error;
mod extract;
mod rate_limit;
mod request_id;
mod router;
mod routes;
mod session_cookie;
mod state;

pub use error::{ApiError, ErrorBody, ErrorEnvelope};
pub use extract::{CurrentUser, PeerAddress};
pub use rate_limit::{RateLimitConfig, RateLimitKey, RateLimiter};
pub use request_id::{REQUEST_ID_HEADER, RequestId};
pub use router::{RouterConfig, build_router};
pub use session_cookie::{INSECURE_SESSION_COOKIE, SECURE_SESSION_COOKIE, SessionCookiePolicy};
pub use state::AppState;
