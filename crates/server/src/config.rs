use std::{fmt, net::SocketAddr};

use application::quote_signing::{EnvironmentQuoteSigner, QuoteSigningError};

/// Typed server configuration, read once at startup. Fails fast (returns
/// `Err`, never panics) on missing or malformed values -- there are no
/// invented production defaults for `DATABASE_URL`, and `Debug`/`Display`
/// never expose it.
pub struct ServerConfig {
    database_url: String,
    bind_addr: SocketAddr,
    log_filter: String,
    insecure_cookies: bool,
    quote_signer: EnvironmentQuoteSigner,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(|name| std::env::var(name).ok())
    }

    fn from_values(
        mut value: impl FnMut(&'static str) -> Option<String>,
    ) -> Result<Self, ConfigError> {
        let database_url = required_value("DATABASE_URL", &mut value)?;
        let quote_signing_key = required_value("QUOTE_SIGNING_KEY", &mut value)?;
        let quote_signer = EnvironmentQuoteSigner::from_base64url(&quote_signing_key)?;

        let host = value("HOST").unwrap_or_else(|| "127.0.0.1".to_owned());
        let port_value = value("PORT").unwrap_or_else(|| "8080".to_owned());
        let port: u16 = port_value
            .parse()
            .map_err(|_| ConfigError::InvalidPort(port_value))?;
        let bind_addr = format!("{host}:{port}")
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddress { host, port })?;

        let log_filter = value("LOG_FILTER").unwrap_or_else(|| "info".to_owned());
        // Opt-out only, and only by an exact value: any typo leaves the
        // Secure attribute on rather than silently dropping it.
        let insecure_cookies = value("INSECURE_COOKIES")
            .is_some_and(|insecure_cookies| insecure_cookies.eq_ignore_ascii_case("true"));

        Ok(Self {
            database_url,
            bind_addr,
            log_filter,
            insecure_cookies,
            quote_signer,
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

    pub fn quote_signer(&self) -> &EnvironmentQuoteSigner {
        &self.quote_signer
    }
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("database_url", &"[REDACTED]")
            .field("bind_addr", &self.bind_addr)
            .field("log_filter", &self.log_filter)
            .field("insecure_cookies", &self.insecure_cookies)
            .field("quote_signer", &self.quote_signer)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigError, ServerConfig};

    #[test]
    fn quote_signing_key_is_required_at_startup() {
        let error = ServerConfig::from_values(|name| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            _ => None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigError::MissingEnvVar("QUOTE_SIGNING_KEY")
        ));
    }

    #[test]
    fn debug_output_does_not_reveal_quote_signing_key() {
        let private_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let config = ServerConfig::from_values(|name| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some(private_key.to_owned()),
            _ => None,
        })
        .unwrap();

        assert!(!format!("{config:?}").contains(private_key));
    }
}

fn required_value(
    name: &'static str,
    value: &mut impl FnMut(&'static str) -> Option<String>,
) -> Result<String, ConfigError> {
    value(name).ok_or(ConfigError::MissingEnvVar(name))
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} environment variable is required")]
    MissingEnvVar(&'static str),
    #[error("PORT must be a valid port number, got '{0}'")]
    InvalidPort(String),
    #[error("'{host}:{port}' is not a valid bind address")]
    InvalidBindAddress { host: String, port: u16 },
    #[error(transparent)]
    QuoteSigning(#[from] QuoteSigningError),
}
