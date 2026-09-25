use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use axum::extract::ConnectInfo;
use ipnet::IpNet;
use tower_governor::{errors::GovernorError, key_extractor::KeyExtractor};

#[derive(Debug, Clone, Default)]
pub struct TrustedProxyConfig {
    networks: Vec<IpNet>,
}

impl TrustedProxyConfig {
    pub fn parse(value: &str) -> Result<Self, TrustedProxyConfigError> {
        if value.is_empty() {
            return Ok(Self::default());
        }
        let networks = value
            .split(',')
            .map(|candidate| {
                let candidate = candidate.trim();
                if candidate.is_empty() {
                    return Err(TrustedProxyConfigError);
                }
                IpNet::from_str(candidate).map_err(|_| TrustedProxyConfigError)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { networks })
    }

    pub fn contains(&self, address: IpAddr) -> bool {
        self.networks
            .iter()
            .any(|network| network.contains(&address))
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("trusted proxy CIDR list is invalid")]
pub struct TrustedProxyConfigError;

#[derive(Debug, Clone)]
pub(crate) struct ClientIpKeyExtractor {
    trusted_proxies: TrustedProxyConfig,
}

impl ClientIpKeyExtractor {
    pub(crate) fn new(trusted_proxies: TrustedProxyConfig) -> Self {
        Self { trusted_proxies }
    }
}

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, request: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|connect_info| connect_info.0.ip())
            .ok_or(GovernorError::UnableToExtractKey)?;
        let forwarded_for = request
            .headers()
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok());
        Ok(resolve_client_ip(
            &self.trusted_proxies,
            peer,
            forwarded_for,
        ))
    }
}

pub fn resolve_client_ip(
    config: &TrustedProxyConfig,
    peer: IpAddr,
    forwarded_for: Option<&str>,
) -> IpAddr {
    if !config.contains(peer) {
        return peer;
    }
    let Some(forwarded_for) = forwarded_for else {
        return peer;
    };
    let addresses = match forwarded_for
        .split(',')
        .map(|candidate| candidate.trim().parse::<IpAddr>())
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(addresses) if !addresses.is_empty() => addresses,
        _ => return peer,
    };

    addresses
        .into_iter()
        .rev()
        .find(|address| !config.contains(*address))
        .unwrap_or(peer)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::{TrustedProxyConfig, resolve_client_ip};

    #[test]
    fn direct_peer_ignores_forged_forwarded_for() {
        let config = TrustedProxyConfig::default();
        let peer = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 4));
        assert_eq!(resolve_client_ip(&config, peer, Some("203.0.113.7")), peer);
    }

    #[test]
    fn trusted_proxy_uses_first_untrusted_hop_from_the_right() {
        let config = TrustedProxyConfig::parse("172.30.0.0/24").unwrap();
        assert_eq!(
            resolve_client_ip(
                &config,
                IpAddr::V4(Ipv4Addr::new(172, 30, 0, 2)),
                Some("203.0.113.7, 172.30.0.3"),
            ),
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))
        );
    }

    #[test]
    fn trusted_multi_hop_chain_discards_known_proxies() {
        let config = TrustedProxyConfig::parse("10.0.0.0/8, 2001:db8:abcd::/48").unwrap();
        let peer = "2001:db8:abcd::2".parse().unwrap();
        assert_eq!(
            resolve_client_ip(
                &config,
                peer,
                Some("2001:db8:1::7, 10.0.0.3, 2001:db8:abcd::3"),
            ),
            "2001:db8:1::7".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn malformed_empty_or_non_ip_chain_falls_back_to_peer() {
        let config = TrustedProxyConfig::parse("172.30.0.0/24").unwrap();
        let peer = IpAddr::V4(Ipv4Addr::new(172, 30, 0, 2));
        for malformed in [
            "",
            "203.0.113.7,",
            "203.0.113.7, attacker",
            "203.0.113.7:443",
        ] {
            assert_eq!(resolve_client_ip(&config, peer, Some(malformed)), peer);
        }
    }

    #[test]
    fn ipv4_and_ipv6_cidrs_are_supported() {
        let config = TrustedProxyConfig::parse("172.30.0.0/24,2001:db8:abcd::/48").unwrap();
        assert!(config.contains(IpAddr::V4(Ipv4Addr::new(172, 30, 0, 9))));
        assert!(config.contains(IpAddr::V6("2001:db8:abcd::9".parse::<Ipv6Addr>().unwrap())));
        assert!(!config.contains(IpAddr::V4(Ipv4Addr::new(172, 31, 0, 9))));
    }

    #[test]
    fn invalid_cidr_configuration_is_rejected() {
        for invalid in ["172.30.0.0/24,", "not-a-cidr", "172.30.0.0/99"] {
            assert!(TrustedProxyConfig::parse(invalid).is_err());
        }
        assert!(TrustedProxyConfig::parse("").is_ok());
    }
}
