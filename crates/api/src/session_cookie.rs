use axum::http::{HeaderMap, HeaderValue, header::COOKIE};

/// Cookie name used when the `Secure` attribute is on, i.e. everywhere
/// except local HTTP development.
///
/// The `__Host-` prefix is the only thing that actually makes a cookie
/// unshadowable: browsers refuse to store such a cookie unless it is
/// `Secure`, has `Path=/`, and carries no `Domain`, which means no other
/// host -- including a sibling subdomain, a stale CNAME, or a forgotten
/// staging box -- can write it. A merely app-specific name gives no such
/// protection: any related host could set `Domain=example.com;
/// Path=/api/v1`, and because browsers send more specific paths first, a
/// naive first-match parser would authenticate the victim's browser as
/// the attacker's account.
pub const SECURE_SESSION_COOKIE: &str = "__Host-contracter_session";
/// Fallback name for plain-HTTP local development, where `__Host-` cannot
/// be used because the browser would reject a non-`Secure` cookie.
pub const INSECURE_SESSION_COOKIE: &str = "contracter_session";

/// Which cookie name this deployment reads and writes. The name is
/// derived from the security mode rather than accepted from either side,
/// so a `Secure` deployment never honours the unprefixed (writable by
/// other hosts) name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionCookiePolicy {
    secure: bool,
}

impl SessionCookiePolicy {
    pub const fn new(secure: bool) -> Self {
        Self { secure }
    }

    pub const fn name(self) -> &'static str {
        if self.secure {
            SECURE_SESSION_COOKIE
        } else {
            INSECURE_SESSION_COOKIE
        }
    }

    /// Reads the session token out of the `Cookie` header.
    ///
    /// Hand-rolled rather than pulling in a cookie crate: this needs
    /// exactly one lookup of one name, and the rules for that are small
    /// enough to state precisely -- split on `;`, split each pair on the
    /// first `=`, trim, and match the name exactly.
    ///
    /// A name appearing more than once is rejected outright rather than
    /// resolved by picking one. Duplicates are not something a
    /// well-behaved client produces; they are what cookie-shadowing looks
    /// like, and any "pick the first" or "pick the last" rule just moves
    /// which half of the attack works.
    pub fn read_token(self, headers: &HeaderMap) -> Option<String> {
        let name = self.name();
        let mut found: Option<String> = None;

        for value in headers
            .get_all(COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .flat_map(|header| header.split(';'))
            .filter_map(|pair| pair.split_once('='))
            .filter_map(|(candidate, value)| (candidate.trim() == name).then_some(value.trim()))
        {
            if found.is_some() {
                return None;
            }
            found = Some(value.to_owned());
        }

        found.filter(|value| !value.is_empty())
    }

    /// The `Set-Cookie` value that establishes a session.
    ///
    /// `HttpOnly` keeps the token away from page JavaScript, so an XSS
    /// bug cannot read it. Production serves the browser from the separate
    /// Cloudflare Pages origin, so `SameSite=None` is required for the
    /// credentialed API fetches to carry this cookie; `Secure` prevents that
    /// cross-site allowance from working over plain HTTP. Local development
    /// stays `Lax` because it uses a same-origin API.
    pub fn set(self, token: &str, max_age: std::time::Duration) -> HeaderValue {
        self.build(token, max_age.as_secs())
    }

    /// The `Set-Cookie` value that clears a session, with the same
    /// attributes so the browser matches and replaces the original.
    pub fn clear(self) -> HeaderValue {
        self.build("", 0)
    }

    fn build(self, token: &str, max_age_seconds: u64) -> HeaderValue {
        let name = self.name();
        // Path=/ and Secure are not optional decoration here: __Host-
        // requires both, and the browser silently drops the cookie
        // otherwise.
        let secure_attribute = if self.secure { "; Secure" } else { "" };
        let same_site = if self.secure { "None" } else { "Lax" };
        let value = format!(
            "{name}={token}; Path=/; HttpOnly; SameSite={same_site}; Max-Age={max_age_seconds}{secure_attribute}"
        );
        HeaderValue::from_str(&value).unwrap_or_else(|_| {
            // A token is base64url and everything else is fixed, so this
            // is unreachable; falling back to a cleared cookie still
            // fails in the safe direction (it never grants a session).
            HeaderValue::from_static(
                "__Host-contracter_session=; Path=/; HttpOnly; SameSite=None; Max-Age=0; Secure",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECURE: SessionCookiePolicy = SessionCookiePolicy::new(true);
    const INSECURE: SessionCookiePolicy = SessionCookiePolicy::new(false);

    fn headers_with(cookie: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(cookie).unwrap());
        headers
    }

    #[test]
    fn reads_the_session_cookie_among_others() {
        let headers = headers_with("theme=dark; __Host-contracter_session=abc123; other=1");
        assert_eq!(SECURE.read_token(&headers).as_deref(), Some("abc123"));
    }

    #[test]
    fn a_secure_deployment_ignores_the_unprefixed_name_entirely() {
        // This is the shadowing defence: the name a sibling host is able
        // to write is simply not the name this deployment reads.
        let headers = headers_with("contracter_session=attacker-token");
        assert_eq!(SECURE.read_token(&headers), None);
    }

    #[test]
    fn a_duplicated_session_cookie_is_rejected_rather_than_resolved() {
        // Browsers order by descending path specificity, so an attacker
        // who can set Path=/api/v1 on a parent domain would otherwise win
        // a first-match rule; picking the last would just invert who wins.
        let headers =
            headers_with("__Host-contracter_session=attacker; __Host-contracter_session=victim");
        assert_eq!(SECURE.read_token(&headers), None);

        let mut split = HeaderMap::new();
        split.append(
            COOKIE,
            HeaderValue::from_static("__Host-contracter_session=attacker"),
        );
        split.append(
            COOKIE,
            HeaderValue::from_static("__Host-contracter_session=victim"),
        );
        assert_eq!(SECURE.read_token(&split), None);
    }

    #[test]
    fn ignores_a_missing_empty_or_differently_named_cookie() {
        assert_eq!(SECURE.read_token(&HeaderMap::new()), None);
        assert_eq!(SECURE.read_token(&headers_with("theme=dark")), None);
        assert_eq!(
            SECURE.read_token(&headers_with("__Host-contracter_session=")),
            None
        );
        assert_eq!(
            SECURE.read_token(&headers_with("not___Host-contracter_session=abc123")),
            None
        );
    }

    #[test]
    fn a_value_containing_an_equals_sign_is_preserved_whole() {
        let headers = headers_with("__Host-contracter_session=a=b=c");
        assert_eq!(SECURE.read_token(&headers).as_deref(), Some("a=b=c"));
    }

    #[test]
    fn reads_across_multiple_cookie_headers() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("theme=dark"));
        headers.append(
            COOKIE,
            HeaderValue::from_static("__Host-contracter_session=xyz"),
        );
        assert_eq!(SECURE.read_token(&headers).as_deref(), Some("xyz"));
    }

    #[test]
    fn the_secure_cookie_carries_every_attribute_the_host_prefix_requires() {
        let rendered = SECURE.set("token-value", std::time::Duration::from_secs(60));
        let rendered = rendered.to_str().unwrap();
        assert!(rendered.starts_with("__Host-contracter_session=token-value"));
        assert!(rendered.contains("HttpOnly"));
        assert!(rendered.contains("SameSite=None"));
        assert!(rendered.contains("Path=/"));
        assert!(rendered.contains("Max-Age=60"));
        assert!(rendered.contains("Secure"));
        assert!(
            !rendered.contains("Domain="),
            "__Host- forbids Domain; setting one makes the browser drop the cookie"
        );
    }

    #[test]
    fn local_development_falls_back_to_an_unprefixed_name_without_secure() {
        let rendered = INSECURE.set("t", std::time::Duration::from_secs(60));
        let rendered = rendered.to_str().unwrap();
        assert!(rendered.starts_with("contracter_session=t"));
        assert!(!rendered.contains("Secure"));
        assert!(
            rendered.contains("HttpOnly") && rendered.contains("SameSite=Lax"),
            "the rest of the hardening still applies"
        );
        assert_eq!(
            INSECURE
                .read_token(&headers_with("contracter_session=t"))
                .as_deref(),
            Some("t")
        );
    }

    #[test]
    fn clearing_expires_the_cookie_immediately_and_carries_no_token() {
        let rendered = SECURE.clear();
        let rendered = rendered.to_str().unwrap();
        assert!(rendered.starts_with("__Host-contracter_session=;"));
        assert!(rendered.contains("Max-Age=0"));
        assert!(rendered.contains("HttpOnly"));
        assert!(rendered.contains("Secure"));
    }
}
