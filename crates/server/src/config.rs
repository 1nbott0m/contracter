use std::{fmt, net::SocketAddr, time::Duration};

use api::RouterConfig;

/// Typed server configuration, read once at startup. Fails fast (returns
/// `Err`, never panics) on missing or malformed values -- there are no
/// invented production defaults for `DATABASE_URL`, and `Debug`/`Display`
/// never expose it.
pub struct ServerConfig {
    database_url: String,
    bind_addr: SocketAddr,
    log_filter: String,
    insecure_cookies: bool,
    cors_allowed_origins: Vec<String>,
    request_timeout: Duration,
    max_body_bytes: usize,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = required_env("DATABASE_URL")?;

        let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_owned());
        let port_value = std::env::var("PORT").unwrap_or_else(|_| "8080".to_owned());
        let port: u16 = port_value
            .parse()
            .map_err(|_| ConfigError::InvalidPort(port_value))?;
        let bind_addr = format!("{host}:{port}")
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddress { host, port })?;

        let log_filter = std::env::var("LOG_FILTER").unwrap_or_else(|_| "info".to_owned());
        // Opt-out only, and only by an exact value: any typo leaves the
        // Secure attribute on rather than silently dropping it.
        let insecure_cookies =
            std::env::var("INSECURE_COOKIES").is_ok_and(|value| value.eq_ignore_ascii_case("true"));

        // Comma-separated, empty entries ignored, so a trailing comma or
        // a blank value is not a malformed origin. Each surviving entry is
        // validated in `build_cors_layer`, which drops what it cannot use
        // rather than failing startup.
        let cors_allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(str::to_owned)
            .collect();

        let defaults = RouterConfig::default();
        let request_timeout =
            optional_duration_secs("REQUEST_TIMEOUT_SECS")?.unwrap_or(defaults.request_timeout);
        let max_body_bytes = optional_usize("MAX_BODY_BYTES")?.unwrap_or(defaults.max_body_bytes);

        Ok(Self {
            database_url,
            bind_addr,
            log_filter,
            insecure_cookies,
            cors_allowed_origins,
            request_timeout,
            max_body_bytes,
        })
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub const fn bind_addr(&self) -> SocketAddr {
        self.bind_addr
    }

    pub fn log_filter(&self) -> &str {
        &self.log_filter
    }

    pub const fn insecure_cookies(&self) -> bool {
        self.insecure_cookies
    }

    /// The router knobs this environment asks for.
    ///
    /// These used to be `RouterConfig::default()` at the call site, which
    /// meant `CORS_ALLOWED_ORIGINS`, the request timeout and the body
    /// limit were unreachable in the shipped binary however they were set
    /// -- the whole CORS path was dead outside tests, and the warning the
    /// router logs about a malformed origin could never fire. Reading them
    /// here is what makes `RouterConfig`'s own promise, that these depend
    /// on runtime configuration rather than hardcoded values, true.
    pub fn router_config(&self) -> RouterConfig {
        RouterConfig {
            request_timeout: self.request_timeout,
            max_body_bytes: self.max_body_bytes,
            cors_allowed_origins: self.cors_allowed_origins.clone(),
        }
    }
}

/// A positive whole number of seconds, or nothing.
///
/// Zero is refused rather than accepted: a zero timeout would fail every
/// request instantly, which is far more likely a typo than an intent.
fn optional_duration_secs(name: &str) -> Result<Option<Duration>, ConfigError> {
    let Ok(raw) = std::env::var(name) else {
        return Ok(None);
    };
    let seconds: u64 = raw.parse().map_err(|_| ConfigError::InvalidNumber {
        name: name.to_owned(),
        value: raw.clone(),
    })?;
    if seconds == 0 {
        return Err(ConfigError::InvalidNumber {
            name: name.to_owned(),
            value: raw,
        });
    }
    Ok(Some(Duration::from_secs(seconds)))
}

fn optional_usize(name: &str) -> Result<Option<usize>, ConfigError> {
    let Ok(raw) = std::env::var(name) else {
        return Ok(None);
    };
    let value: usize = raw.parse().map_err(|_| ConfigError::InvalidNumber {
        name: name.to_owned(),
        value: raw.clone(),
    })?;
    if value == 0 {
        return Err(ConfigError::InvalidNumber {
            name: name.to_owned(),
            value: raw,
        });
    }
    Ok(Some(value))
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("database_url", &"[REDACTED]")
            .field("bind_addr", &self.bind_addr)
            .field("log_filter", &self.log_filter)
            .field("insecure_cookies", &self.insecure_cookies)
            .finish()
    }
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    std::env::var(name).map_err(|_| ConfigError::MissingEnvVar(name))
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} environment variable is required")]
    MissingEnvVar(&'static str),
    #[error("PORT must be a valid port number, got '{0}'")]
    InvalidPort(String),
    #[error("'{host}:{port}' is not a valid bind address")]
    InvalidBindAddress { host: String, port: u16 },
    /// A numeric setting that is absent is fine and takes its default; one
    /// that is present and unusable is a configuration mistake, and
    /// starting anyway on the default would hide it.
    #[error("{name} must be a positive whole number, got '{value}'")]
    InvalidNumber { name: String, value: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every setting must actually reach the router.
    ///
    /// These three were declared, documented, and never read: the binary
    /// passed `RouterConfig::default()`, so an operator could set
    /// `CORS_ALLOWED_ORIGINS` and get total silence -- not even the
    /// warning the router logs for a malformed origin, because the list
    /// was never looked at. This asserts the wiring rather than the
    /// parsing, which is where it broke.
    #[test]
    fn every_router_setting_reaches_the_router() {
        let config = ServerConfig {
            database_url: "postgresql://ignored".to_owned(),
            bind_addr: "127.0.0.1:8080".parse().expect("a bind address"),
            log_filter: "info".to_owned(),
            insecure_cookies: false,
            cors_allowed_origins: vec!["https://app.example".to_owned()],
            request_timeout: Duration::from_secs(3),
            max_body_bytes: 4_096,
        };

        let router = config.router_config();
        assert_eq!(router.cors_allowed_origins, vec!["https://app.example"]);
        assert_eq!(router.request_timeout, Duration::from_secs(3));
        assert_eq!(router.max_body_bytes, 4_096);
    }

    /// A numeric setting that is present and unusable is a mistake worth
    /// failing on. Starting anyway on the default would hide it, and the
    /// operator would believe a limit is in force that is not.
    #[test]
    fn an_unusable_numeric_setting_is_refused_rather_than_defaulted() {
        // Zero is refused as well as non-numeric: a zero timeout fails
        // every request instantly, which is far more likely a typo than an
        // intent.
        for value in ["0", "-1", "ten", ""] {
            unsafe { std::env::set_var("CONTRACTER_TEST_NUMBER", value) };
            assert!(
                optional_duration_secs("CONTRACTER_TEST_NUMBER").is_err(),
                "{value:?} must be refused"
            );
            assert!(optional_usize("CONTRACTER_TEST_NUMBER").is_err());
        }
        unsafe { std::env::remove_var("CONTRACTER_TEST_NUMBER") };
        // Absent is fine and means "take the default".
        assert_eq!(
            optional_duration_secs("CONTRACTER_TEST_NUMBER").expect("absent is ok"),
            None
        );
    }
}
