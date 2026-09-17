use axum::http::{HeaderMap, HeaderValue, header::COOKIE};

/// Session cookie name. Prefixed with the app rather than something
/// generic so it cannot collide with another service on the same host.
pub const SESSION_COOKIE: &str = "contracter_session";

/// Reads the session token out of the `Cookie` header, if present.
///
/// Hand-rolled rather than pulling in a cookie crate: this needs exactly
/// one lookup of one name, and the parsing rules for that are small
/// enough to state precisely (split on `;`, split each pair on the first
/// `=`, trim surrounding whitespace). A value is returned verbatim -- it
/// is a secret to be hashed and compared, never interpreted.
pub fn read_session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(name, value)| (name.trim() == SESSION_COOKIE).then(|| value.trim().to_owned()))
        .filter(|value| !value.is_empty())
}

/// Builds the `Set-Cookie` value that establishes a session.
///
/// `HttpOnly` keeps the token away from page JavaScript, so an XSS bug
/// cannot read it. `SameSite=Lax` means a cross-site POST never carries
/// the cookie, which is the main CSRF barrier for the state-changing
/// endpoints here (all of them are POST, and all of them additionally
/// require a JSON body, which a plain cross-origin HTML form cannot
/// send). `Secure` is configurable only so local HTTP development works;
/// it is on by default and must stay on anywhere else.
pub fn session_cookie(token: &str, max_age: std::time::Duration, secure: bool) -> HeaderValue {
    build_cookie(token, max_age.as_secs(), secure)
}

/// Builds the `Set-Cookie` value that clears a session, with the same
/// attributes so the browser matches and replaces the original cookie.
pub fn cleared_session_cookie(secure: bool) -> HeaderValue {
    build_cookie("", 0, secure)
}

fn build_cookie(token: &str, max_age_seconds: u64, secure: bool) -> HeaderValue {
    let secure_attribute = if secure { "; Secure" } else { "" };
    let value = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_seconds}{secure_attribute}"
    );
    HeaderValue::from_str(&value).unwrap_or_else(|_| {
        // A token is base64url and the rest is fixed, so this is
        // unreachable; falling back to a cleared cookie is still the safe
        // direction (it never grants a session).
        HeaderValue::from_static("contracter_session=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(cookie: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(cookie).unwrap());
        headers
    }

    #[test]
    fn reads_the_session_cookie_among_others() {
        let headers = headers_with("theme=dark; contracter_session=abc123; other=1");
        assert_eq!(read_session_token(&headers).as_deref(), Some("abc123"));
    }

    #[test]
    fn reads_a_lone_session_cookie() {
        let headers = headers_with("contracter_session=abc123");
        assert_eq!(read_session_token(&headers).as_deref(), Some("abc123"));
    }

    #[test]
    fn ignores_a_missing_empty_or_differently_named_cookie() {
        assert_eq!(read_session_token(&HeaderMap::new()), None);
        assert_eq!(read_session_token(&headers_with("theme=dark")), None);
        assert_eq!(
            read_session_token(&headers_with("contracter_session=")),
            None
        );
        assert_eq!(
            read_session_token(&headers_with("not_contracter_session=abc123")),
            None
        );
    }

    #[test]
    fn reads_across_multiple_cookie_headers() {
        let mut headers = HeaderMap::new();
        headers.append(COOKIE, HeaderValue::from_static("theme=dark"));
        headers.append(COOKIE, HeaderValue::from_static("contracter_session=xyz"));
        assert_eq!(read_session_token(&headers).as_deref(), Some("xyz"));
    }

    #[test]
    fn the_session_cookie_carries_every_hardening_attribute() {
        let cookie = session_cookie("token-value", std::time::Duration::from_secs(60), true);
        let rendered = cookie.to_str().unwrap();
        assert!(rendered.contains("contracter_session=token-value"));
        assert!(rendered.contains("HttpOnly"));
        assert!(rendered.contains("SameSite=Lax"));
        assert!(rendered.contains("Path=/"));
        assert!(rendered.contains("Max-Age=60"));
        assert!(rendered.contains("Secure"));
    }

    #[test]
    fn the_secure_attribute_can_be_disabled_only_deliberately() {
        let rendered = session_cookie("t", std::time::Duration::from_secs(60), false);
        let rendered = rendered.to_str().unwrap();
        assert!(!rendered.contains("Secure"));
        assert!(
            rendered.contains("HttpOnly"),
            "other hardening still applies"
        );
    }

    #[test]
    fn clearing_expires_the_cookie_immediately_and_carries_no_token() {
        let rendered = cleared_session_cookie(true);
        let rendered = rendered.to_str().unwrap();
        assert!(rendered.contains("contracter_session=;"));
        assert!(rendered.contains("Max-Age=0"));
        assert!(rendered.contains("HttpOnly"));
    }
}
