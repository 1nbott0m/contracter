use std::{fmt, fs, net::SocketAddr, time::Duration};

use api::{RouterConfig, TrustedProxyConfig};
use application::quote_signing::{EnvironmentQuoteSigner, QuoteSigningError};
use application::seed_protection::{EnvironmentSeedProtector, SeedProtectionError};

const MAX_SECRET_BYTES: u64 = 64 * 1024;

/// Typed server configuration, read once at startup. Fails fast (returns
/// `Err`, never panics) on missing or malformed values -- there are no
/// invented production defaults for `DATABASE_URL`, and `Debug`/`Display`
/// never expose it.
pub struct ServerConfig {
    database_url: String,
    bind_addr: SocketAddr,
    log_filter: String,
    insecure_cookies: bool,
    rate_limit_burst: u32,
    quote_signer: EnvironmentQuoteSigner,
    seed_protector: EnvironmentSeedProtector,
    trusted_proxy_cidrs: TrustedProxyConfig,
    metrics_addr: SocketAddr,
    log_format: LogFormat,
    cors_allowed_origins: Vec<String>,
    request_timeout: Duration,
    max_body_bytes: usize,
    shutdown_drain: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogFormat {
    Json,
    Pretty,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(|name| std::env::var(name).ok())
    }

    fn from_values(mut value: impl FnMut(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let database_url = required_secret("DATABASE_URL", &mut value)?;
        let quote_signing_key = required_secret("QUOTE_SIGNING_KEY", &mut value)?;
        let quote_signer = EnvironmentQuoteSigner::from_base64url(&quote_signing_key)?;
        let quote_seed_key = required_secret("QUOTE_SEED_KEY", &mut value)?;
        let seed_protector = EnvironmentSeedProtector::from_base64url(&quote_seed_key)?;

        let host = value("HOST").unwrap_or_else(|| "127.0.0.1".to_owned());
        let port_value = value("PORT").unwrap_or_else(|| "8080".to_owned());
        let port: u16 = port_value
            .parse()
            .map_err(|_| ConfigError::InvalidPort(port_value))?;
        let bind_addr = format!("{host}:{port}")
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddress { host, port })?;

        let log_filter = value("LOG_FILTER").unwrap_or_else(|| "info".to_owned());
        let log_format = match value("LOG_FORMAT").as_deref() {
            None | Some("json") => LogFormat::Json,
            Some("pretty") => LogFormat::Pretty,
            Some(other) => return Err(ConfigError::InvalidLogFormat(other.to_owned())),
        };
        // Opt-out only, and only by an exact value: any typo leaves the
        // Secure attribute on rather than silently dropping it.
        let insecure_cookies = value("INSECURE_COOKIES")
            .is_some_and(|insecure_cookies| insecure_cookies.eq_ignore_ascii_case("true"));
        let rate_limit_value = value("RATE_LIMIT_BURST").unwrap_or_else(|| "30".to_owned());
        let rate_limit_burst = rate_limit_value
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| ConfigError::InvalidRateLimitBurst(rate_limit_value.clone()))?;
        let trusted_proxy_value = value("TRUSTED_PROXY_CIDRS").unwrap_or_default();
        let trusted_proxy_cidrs = TrustedProxyConfig::parse(&trusted_proxy_value)
            .map_err(|_| ConfigError::InvalidTrustedProxyCidrs(trusted_proxy_value.clone()))?;
        let metrics_host = value("METRICS_HOST").unwrap_or_else(|| "127.0.0.1".to_owned());
        let metrics_port_value = value("METRICS_PORT").unwrap_or_else(|| "9090".to_owned());
        let metrics_port = metrics_port_value
            .parse()
            .map_err(|_| ConfigError::InvalidPort(metrics_port_value.clone()))?;
        let metrics_addr = format!("{metrics_host}:{metrics_port}")
            .parse()
            .map_err(|_| ConfigError::InvalidBindAddress {
                host: metrics_host,
                port: metrics_port,
            })?;
        // Comma-separated, empty entries ignored, so a trailing comma or
        // a blank value is not a malformed origin. Each surviving entry is
        // validated in `build_cors_layer`, which drops what it cannot use
        // rather than failing startup.
        let cors_allowed_origins: Vec<String> = value("CORS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let defaults = RouterConfig::default();
        let request_timeout = optional_duration_secs("REQUEST_TIMEOUT_SECS", &mut value)?
            .unwrap_or(defaults.request_timeout);
        let max_body_bytes =
            optional_usize("MAX_BODY_BYTES", &mut value)?.unwrap_or(defaults.max_body_bytes);
        // Five seconds covers a one- or two-second readiness interval with
        // room for one missed probe. It is not a guess about how long
        // in-flight work takes -- that is what the request timeout is for.
        let shutdown_drain = optional_duration_secs("SHUTDOWN_DRAIN_SECS", &mut value)?
            .unwrap_or(Duration::from_secs(5));

        Ok(Self {
            database_url,
            bind_addr,
            log_filter,
            insecure_cookies,
            rate_limit_burst,
            quote_signer,
            seed_protector,
            trusted_proxy_cidrs,
            metrics_addr,
            log_format,
            cors_allowed_origins,
            request_timeout,
            max_body_bytes,
            shutdown_drain,
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

    pub fn seed_protector(&self) -> &EnvironmentSeedProtector {
        &self.seed_protector
    }

    pub const fn metrics_addr(&self) -> SocketAddr {
        self.metrics_addr
    }

    pub const fn log_format(&self) -> LogFormat {
        self.log_format
    }

    /// How long to report unready before closing the listening socket.
    pub const fn shutdown_drain(&self) -> Duration {
        self.shutdown_drain
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
            rate_limit_burst: Some(self.rate_limit_burst),
            trusted_proxies: self.trusted_proxy_cidrs.clone(),
        }
    }
}

/// A positive whole number of seconds, or nothing.
///
/// Zero is refused rather than accepted: a zero timeout would fail every
/// request instantly, which is far more likely a typo than an intent.
fn optional_duration_secs(
    name: &str,
    value: &mut impl FnMut(&str) -> Option<String>,
) -> Result<Option<Duration>, ConfigError> {
    let Some(raw) = value(name) else {
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

fn optional_usize(
    name: &str,
    value: &mut impl FnMut(&str) -> Option<String>,
) -> Result<Option<usize>, ConfigError> {
    let Some(raw) = value(name) else {
        return Ok(None);
    };
    let parsed: usize = raw.parse().map_err(|_| ConfigError::InvalidNumber {
        name: name.to_owned(),
        value: raw.clone(),
    })?;
    if parsed == 0 {
        return Err(ConfigError::InvalidNumber {
            name: name.to_owned(),
            value: raw,
        });
    }
    Ok(Some(parsed))
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerConfig")
            .field("database_url", &"[REDACTED]")
            .field("bind_addr", &self.bind_addr)
            .field("log_filter", &self.log_filter)
            .field("insecure_cookies", &self.insecure_cookies)
            .field("rate_limit_burst", &self.rate_limit_burst)
            .field("quote_signer", &self.quote_signer)
            .field("seed_protector", &self.seed_protector)
            .field("trusted_proxy_cidrs", &self.trusted_proxy_cidrs)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::SystemTime};

    use super::{ConfigError, ServerConfig, required_secret};

    fn unique_test_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("contracter-{label}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn secret_must_have_exactly_one_nonempty_source() {
        let path = unique_test_path("secret-source");
        fs::write(&path, "from-file\n").expect("write test secret");
        let path_text = path.to_string_lossy().into_owned();

        let mut direct_only = |name: &str| (name == "SECRET").then(|| "direct".to_owned());
        assert_eq!(
            required_secret("SECRET", &mut direct_only).unwrap(),
            "direct"
        );

        let mut file_only = |name: &str| (name == "SECRET_FILE").then(|| path_text.clone());
        assert_eq!(
            required_secret("SECRET", &mut file_only).unwrap(),
            "from-file"
        );

        let mut both = |name: &str| match name {
            "SECRET" => Some("direct".to_owned()),
            "SECRET_FILE" => Some(path_text.clone()),
            _ => None,
        };
        assert!(matches!(
            required_secret("SECRET", &mut both),
            Err(ConfigError::ConflictingSecretSources("SECRET"))
        ));

        let mut neither = |_name: &str| None;
        assert!(matches!(
            required_secret("SECRET", &mut neither),
            Err(ConfigError::MissingEnvVar("SECRET"))
        ));

        fs::remove_file(path).expect("remove test secret");
    }

    #[test]
    fn secret_file_rejects_empty_directory_and_oversized_content_without_leaking() {
        for (label, prepare) in [("empty", 0_usize), ("oversized", 65 * 1024)] {
            let path = unique_test_path(label);
            fs::write(&path, vec![b'x'; prepare]).expect("write invalid test secret");
            let path_text = path.to_string_lossy().into_owned();
            let mut source = |name: &str| (name == "SECRET_FILE").then(|| path_text.clone());
            let error = required_secret("SECRET", &mut source).unwrap_err();
            assert!(matches!(error, ConfigError::InvalidSecretFile("SECRET")));
            assert!(!error.to_string().contains(&path_text));
            fs::remove_file(path).expect("remove invalid test secret");
        }

        let directory = unique_test_path("directory");
        fs::create_dir(&directory).expect("create invalid secret directory");
        let path_text = directory.to_string_lossy().into_owned();
        let mut source = |name: &str| (name == "SECRET_FILE").then(|| path_text.clone());
        let error = required_secret("SECRET", &mut source).unwrap_err();
        assert!(matches!(error, ConfigError::InvalidSecretFile("SECRET")));
        assert!(!error.to_string().contains(&path_text));
        fs::remove_dir(directory).expect("remove invalid secret directory");
    }

    #[test]
    fn config_debug_never_contains_direct_or_file_backed_secrets() {
        let database = unique_test_path("database-url");
        let signing = unique_test_path("signing-key");
        let seed = unique_test_path("seed-key");
        fs::write(&database, "postgres://secret@localhost/contracter\n").unwrap();
        fs::write(&signing, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n").unwrap();
        fs::write(&seed, "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE\n").unwrap();

        let config = ServerConfig::from_values(|name| match name {
            "DATABASE_URL_FILE" => Some(database.to_string_lossy().into_owned()),
            "QUOTE_SIGNING_KEY_FILE" => Some(signing.to_string_lossy().into_owned()),
            "QUOTE_SEED_KEY_FILE" => Some(seed.to_string_lossy().into_owned()),
            _ => None,
        })
        .unwrap();
        let debug = format!("{config:?}");
        for secret in [
            "postgres://secret@localhost/contracter",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE",
        ] {
            assert!(!debug.contains(secret));
        }

        for path in [&database, &signing, &seed] {
            fs::remove_file(path).expect("remove test secret");
        }
    }

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
    fn quote_seed_key_is_required_at_startup() {
        let error = ServerConfig::from_values(|name| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
            _ => None,
        })
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigError::MissingEnvVar("QUOTE_SEED_KEY")
        ));
    }

    #[test]
    fn debug_output_does_not_reveal_server_secrets() {
        let quote_signing_key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let quote_seed_key = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE";
        let config = ServerConfig::from_values(|name| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some(quote_signing_key.to_owned()),
            "QUOTE_SEED_KEY" => Some(quote_seed_key.to_owned()),
            _ => None,
        })
        .unwrap();

        let debug = format!("{config:?}");
        assert!(!debug.contains(quote_signing_key));
        assert!(!debug.contains(quote_seed_key));
    }

    #[test]
    fn rate_limit_burst_is_positive_and_configurable() {
        let base = |name: &str| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
            "QUOTE_SEED_KEY" => Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE".to_owned()),
            "RATE_LIMIT_BURST" => Some("17".to_owned()),
            _ => None,
        };
        assert_eq!(
            ServerConfig::from_values(base)
                .unwrap()
                .router_config()
                .rate_limit_burst,
            Some(17)
        );

        let error = ServerConfig::from_values(|name| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
            "QUOTE_SEED_KEY" => Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE".to_owned()),
            "RATE_LIMIT_BURST" => Some("0".to_owned()),
            _ => None,
        })
        .unwrap_err();
        assert!(matches!(error, ConfigError::InvalidRateLimitBurst(_)));
    }

    #[test]
    fn trusted_proxy_cidrs_are_parsed_and_invalid_values_fail_startup() {
        let valid = ServerConfig::from_values(|name: &str| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
            "QUOTE_SEED_KEY" => Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE".to_owned()),
            "TRUSTED_PROXY_CIDRS" => Some("172.30.0.0/24".to_owned()),
            _ => None,
        })
        .unwrap();
        assert!(
            valid
                .router_config()
                .trusted_proxies
                .contains("172.30.0.2".parse().unwrap())
        );

        let error = ServerConfig::from_values(|name: &str| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
            "QUOTE_SEED_KEY" => Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE".to_owned()),
            "TRUSTED_PROXY_CIDRS" => Some("not-a-cidr".to_owned()),
            _ => None,
        })
        .unwrap_err();
        assert!(matches!(error, ConfigError::InvalidTrustedProxyCidrs(_)));
    }
}

fn required_secret(
    name: &'static str,
    value: &mut impl FnMut(&str) -> Option<String>,
) -> Result<String, ConfigError> {
    let file_name = format!("{name}_FILE");
    let direct = value(name);
    let file = value(&file_name);

    match (direct, file) {
        (Some(_), Some(_)) => Err(ConfigError::ConflictingSecretSources(name)),
        (Some(secret), None) if !secret.is_empty() => Ok(secret),
        (Some(_), None) | (None, None) => Err(ConfigError::MissingEnvVar(name)),
        (None, Some(path)) => {
            let metadata =
                fs::metadata(&path).map_err(|_| ConfigError::UnreadableSecretFile(name))?;
            if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_SECRET_BYTES {
                return Err(ConfigError::InvalidSecretFile(name));
            }
            let mut secret =
                fs::read_to_string(&path).map_err(|_| ConfigError::UnreadableSecretFile(name))?;
            if secret.ends_with('\n') {
                secret.pop();
                if secret.ends_with('\r') {
                    secret.pop();
                }
            }
            if secret.is_empty() {
                return Err(ConfigError::InvalidSecretFile(name));
            }
            Ok(secret)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{0} environment variable is required")]
    MissingEnvVar(&'static str),
    #[error("{0} and its _FILE source cannot both be set")]
    ConflictingSecretSources(&'static str),
    #[error("{0}_FILE must refer to a non-empty regular file no larger than 64 KiB")]
    InvalidSecretFile(&'static str),
    #[error("{0}_FILE could not be read")]
    UnreadableSecretFile(&'static str),
    #[error("PORT must be a valid port number, got '{0}'")]
    InvalidPort(String),
    #[error("'{host}:{port}' is not a valid bind address")]
    InvalidBindAddress { host: String, port: u16 },
    #[error("RATE_LIMIT_BURST must be a positive integer, got '{0}'")]
    InvalidRateLimitBurst(String),
    #[error("TRUSTED_PROXY_CIDRS must be a comma-separated CIDR list, got '{0}'")]
    InvalidTrustedProxyCidrs(String),
    #[error("invalid LOG_FORMAT: {0}")]
    InvalidLogFormat(String),
    #[error(transparent)]
    QuoteSigning(#[from] QuoteSigningError),
    #[error(transparent)]
    SeedProtection(#[from] SeedProtectionError),
    /// A numeric setting that is absent is fine and takes its default; one
    /// that is present and unusable is a configuration mistake, and
    /// starting anyway on the default would hide it.
    #[error("{name} must be a positive whole number, got '{value}'")]
    InvalidNumber { name: String, value: String },
}

#[cfg(test)]
mod router_setting_tests {
    use super::*;

    /// The minimum a valid environment needs, plus whatever a test adds.
    fn environment(
        extra: &'static [(&'static str, &'static str)],
    ) -> impl FnMut(&str) -> Option<String> {
        move |name| match name {
            "DATABASE_URL" => Some("postgres://example.invalid/contracter".to_owned()),
            "QUOTE_SIGNING_KEY" => Some("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_owned()),
            "QUOTE_SEED_KEY" => Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE".to_owned()),
            other => extra
                .iter()
                .find(|(key, _)| *key == other)
                .map(|(_, value)| (*value).to_owned()),
        }
    }

    /// Every setting must actually reach the router.
    ///
    /// These were declared, documented, and never read: the binary passed
    /// `RouterConfig::default()`, so an operator could set
    /// `CORS_ALLOWED_ORIGINS` and get total silence. This asserts the wiring
    /// rather than the parsing, which is where it broke.
    #[test]
    fn every_router_setting_reaches_the_router() {
        let config = ServerConfig::from_values(environment(&[
            (
                "CORS_ALLOWED_ORIGINS",
                " https://a.example , ,https://b.example,",
            ),
            ("REQUEST_TIMEOUT_SECS", "7"),
            ("MAX_BODY_BYTES", "8192"),
            ("SHUTDOWN_DRAIN_SECS", "11"),
            ("INSECURE_COOKIES", "TRUE"),
            ("RATE_LIMIT_BURST", "17"),
        ]))
        .expect("a valid environment parses");

        let router = config.router_config();
        // A trailing comma and a blank entry are not malformed origins.
        assert_eq!(
            router.cors_allowed_origins,
            vec!["https://a.example", "https://b.example"]
        );
        assert_eq!(router.request_timeout, Duration::from_secs(7));
        assert_eq!(router.max_body_bytes, 8_192);
        assert_eq!(router.rate_limit_burst, Some(17));

        // Not routed through `RouterConfig`, and the same class of setting:
        // one decides whether the session cookie carries `Secure`.
        assert!(config.insecure_cookies());
        assert_eq!(config.shutdown_drain(), Duration::from_secs(11));
    }

    /// An absent numeric setting takes its default; a present, unusable one
    /// is a mistake worth failing on, because starting anyway would leave
    /// the operator believing a limit is in force that is not.
    #[test]
    fn an_unusable_numeric_setting_is_refused_rather_than_defaulted() {
        for value in ["0", "-1", "ten", ""] {
            let mut lookup = |_: &str| Some(value.to_owned());
            assert!(
                optional_duration_secs("X", &mut lookup).is_err(),
                "{value:?} must be refused as a duration"
            );
            assert!(
                optional_usize("X", &mut lookup).is_err(),
                "{value:?} must be refused as a size"
            );
        }
        let mut absent = |_: &str| None;
        assert_eq!(
            optional_duration_secs("X", &mut absent).expect("absent is ok"),
            None
        );

        let error =
            ServerConfig::from_values(environment(&[("REQUEST_TIMEOUT_SECS", "0")])).unwrap_err();
        assert!(matches!(error, ConfigError::InvalidNumber { .. }));
    }
}
