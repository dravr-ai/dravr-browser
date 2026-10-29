// ABOUTME: Cookie-based browser session capture/injection and the AuthSession data model
// ABOUTME: Lets a profile's authenticated cookies be snapshotted and replayed across browsers
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

#[cfg(feature = "chrome")]
use chromiumoxide::cdp::browser_protocol::network::{Cookie, CookieParam, TimeSinceEpoch};
#[cfg(feature = "chrome")]
use chromiumoxide::{Browser, Page};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
#[cfg(feature = "chrome")]
use tracing::debug;

use crate::error::{BrowserError, BrowserResult};

/// A captured browser session: an identifier plus the cookies that authenticate it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    /// Session identifier (for cache keying / profile naming).
    pub session_id: String,
    /// Captured browser cookies.
    pub cookies: Vec<CookieData>,
    /// When this session was created.
    pub created_at: DateTime<Utc>,
    /// When this session expires, if known.
    ///
    /// A capture leaves it `None`: a login's jar holds short-lived analytics
    /// and bot-check cookies beside the ones that sign in, so no cookie's
    /// expiry dates the session. Each cookie carries its own
    /// ([`CookieData::expires`]); whether the session still works is the
    /// site's answer to a request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A single browser cookie.
///
/// Its `Debug` prints the value as `[REDACTED]`: a cookie value is a live
/// session credential, and an [`AuthSession`] reaches logs through `{:?}`.
#[derive(Clone, Serialize, Deserialize)]
pub struct CookieData {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Cookie domain.
    pub domain: String,
    /// Cookie path.
    pub path: String,
    /// Whether the cookie is secure-only.
    pub secure: bool,
    /// Whether the cookie is HTTP-only.
    pub http_only: bool,
    /// When the cookie expires, in seconds since the Unix epoch as CDP reports
    /// it; `None` for a session cookie. Absent from sessions stored before the
    /// field existed, which read back as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<f64>,
}

impl fmt::Debug for CookieData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CookieData")
            .field("name", &self.name)
            .field("value", &"[REDACTED]")
            .field("domain", &self.domain)
            .field("path", &self.path)
            .field("secure", &self.secure)
            .field("http_only", &self.http_only)
            .field("expires", &self.expires)
            .finish()
    }
}

/// The CDP parameter that sets `cookie` back into a browser, its expiry
/// included so a persistent cookie stays persistent.
#[cfg(feature = "chrome")]
fn cookie_param(cookie: &CookieData) -> CookieParam {
    let mut param = CookieParam::new(&cookie.name, &cookie.value);
    param.domain = Some(cookie.domain.clone());
    param.path = Some(cookie.path.clone());
    param.secure = Some(cookie.secure);
    param.http_only = Some(cookie.http_only);
    param.expires = cookie.expires.map(TimeSinceEpoch::new);
    param
}

/// Inject session cookies into a page before navigation.
#[cfg(feature = "chrome")]
pub async fn inject_cookies(page: &Page, session: &AuthSession) -> BrowserResult<()> {
    for cookie in &session.cookies {
        page.set_cookie(cookie_param(cookie))
            .await
            .map_err(|e| BrowserError::Browser {
                reason: format!("Failed to set cookie {}: {e}", cookie.name),
            })?;
    }

    debug!(count = session.cookies.len(), "Injected session cookies");
    Ok(())
}

/// A CDP cookie as [`CookieData`].
///
/// CDP reports a session cookie with `session: true` and an `expires` of
/// `-1`; the flag decides, and a non-positive expiry is read as no date too.
#[cfg(feature = "chrome")]
fn cookie_data(cookie: &Cookie) -> CookieData {
    let expires = (!cookie.session && cookie.expires > 0.0).then_some(cookie.expires);
    CookieData {
        name: cookie.name.clone(),
        value: cookie.value.clone(),
        domain: cookie.domain.clone(),
        path: cookie.path.clone(),
        secure: cookie.secure,
        http_only: cookie.http_only,
        expires,
    }
}

/// A fresh [`AuthSession`] over `cookies`, refused when there are none.
#[cfg(feature = "chrome")]
fn session_from_cookies(cookies: &[Cookie]) -> BrowserResult<AuthSession> {
    let cookie_data: Vec<CookieData> = cookies.iter().map(cookie_data).collect();

    if cookie_data.is_empty() {
        return Err(BrowserError::Auth {
            reason: "No cookies captured after login".to_owned(),
        });
    }

    Ok(AuthSession {
        session_id: generate_session_id()?,
        cookies: cookie_data,
        created_at: Utc::now(),
        expires_at: None,
    })
}

/// Capture the cookies the current page sees into an [`AuthSession`].
///
/// Page-scoped: CDP's `Network.getCookies` without URLs returns only the
/// cookies that apply to the page's URL and its subframes, so a cookie set
/// for another host of the same login (an SSO domain, say) is left out.
/// [`capture_browser_session`] reads the whole jar.
///
/// Returns [`BrowserError::Auth`] if no cookies are present (login likely failed).
#[cfg(feature = "chrome")]
pub async fn capture_session(page: &Page) -> BrowserResult<AuthSession> {
    let cookies = page
        .get_cookies()
        .await
        .map_err(|e| BrowserError::Browser {
            reason: format!("Failed to get cookies: {e}"),
        })?;
    session_from_cookies(&cookies)
}

/// Capture every cookie in the browser's default context into an
/// [`AuthSession`], whatever host it belongs to.
///
/// [`Browser::get_cookies`] issues CDP's `Storage.getCookies`, which returns
/// the context's whole cookie jar rather than the cookies one page's URL
/// sees. A login that crosses hosts (an SSO domain that sets cookies the
/// application domain then relies on) is captured whole, which is what an
/// HTTP client replaying the session needs. Each cookie keeps its expiry;
/// [`AuthSession::expires_at`] stays `None` (see there).
///
/// Returns [`BrowserError::Auth`] if the jar is empty (login likely failed).
#[cfg(feature = "chrome")]
pub async fn capture_browser_session(browser: &Browser) -> BrowserResult<AuthSession> {
    let cookies = browser
        .get_cookies()
        .await
        .map_err(|e| BrowserError::Browser {
            reason: format!("Failed to get the browser's cookies: {e}"),
        })?;
    session_from_cookies(&cookies)
}

/// Bytes of operating-system randomness in a session id: 128 bits, beyond the
/// reach of guessing.
const SESSION_ID_BYTES: usize = 16;

/// Lowercase hex digits, indexed by nibble.
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// Mint a session id from the operating system's CSPRNG, hex-encoded: 32
/// lowercase hex digits carrying 128 random bits.
///
/// A consumer that holds many users' sessions reaches one by naming its id, so
/// the id is a bearer and has to be unguessable. An id read off the clock is
/// not: its seconds are the session's wall time and its nanoseconds leave a
/// range small enough to search. Hex keeps the id safe where it is used
/// verbatim — as a Chrome profile directory name and in the proxy URL's
/// `{session_id}` placeholder ([`crate::BrowserLaunchConfig::proxy_url`]).
///
/// # Errors
///
/// [`BrowserError::Browser`] when the operating system's random source fails.
pub fn generate_session_id() -> BrowserResult<String> {
    let mut bytes = [0u8; SESSION_ID_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| BrowserError::Browser {
        reason: format!("Failed to generate a session id: {e}"),
    })?;
    Ok(bytes
        .iter()
        .flat_map(|byte| {
            [
                HEX_DIGITS[usize::from(byte >> 4)],
                HEX_DIGITS[usize::from(byte & 0x0f)],
            ]
        })
        .map(char::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_session_roundtrips_json() {
        let session = AuthSession {
            session_id: "abc".to_owned(),
            cookies: vec![CookieData {
                name: "sessionKey".to_owned(),
                value: "v".to_owned(),
                domain: ".claude.ai".to_owned(),
                path: "/".to_owned(),
                secure: true,
                http_only: true,
                expires: None,
            }],
            created_at: Utc::now(),
            expires_at: None,
        };
        let json = serde_json::to_string(&session).unwrap(); // Safe: test with a valid struct
        let back: AuthSession = serde_json::from_str(&json).unwrap(); // Safe: test roundtrip of just-serialized JSON
        assert_eq!(back.session_id, "abc");
        assert_eq!(back.cookies.len(), 1);
        assert_eq!(back.cookies[0].domain, ".claude.ai");
    }

    #[test]
    fn a_cookie_stored_without_expires_reads_back_as_a_session_cookie() {
        let json = r#"{"name":"SESSION","value":"v","domain":".garmin.com","path":"/","secure":true,"http_only":true}"#;
        let cookie: CookieData =
            serde_json::from_str(json).unwrap_or_else(|e| panic!("pre-expiry JSON: {e}"));
        assert_eq!(
            cookie.expires, None,
            "a stored cookie with no expires key is a session cookie"
        );
        let written = serde_json::to_string(&cookie).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert!(
            !written.contains("expires"),
            "a session cookie writes no expires: {written}"
        );
    }

    #[test]
    fn a_cookie_expiry_round_trips() {
        let cookie = CookieData {
            name: "SESSION".to_owned(),
            value: "v".to_owned(),
            domain: "sso.garmin.com".to_owned(),
            path: "/".to_owned(),
            secure: true,
            http_only: false,
            expires: Some(1_893_456_000.5),
        };
        let json = serde_json::to_string(&cookie).unwrap_or_else(|e| panic!("serialize: {e}"));
        let back: CookieData =
            serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert_eq!(
            back.expires,
            Some(1_893_456_000.5),
            "the expiry survives serialization to the second: {json}"
        );
    }

    #[cfg(feature = "chrome")]
    mod chrome {
        use super::super::*;
        use serde_json::json;

        fn cdp_cookie(name: &str, expires: f64, session: bool) -> Cookie {
            serde_json::from_value(json!({
                "name": name,
                "value": "v",
                "domain": "sso.garmin.com",
                "path": "/",
                "expires": expires,
                "size": 2,
                "httpOnly": true,
                "secure": true,
                "session": session,
                "priority": "Medium",
                "sourceScheme": "Secure",
                "sourcePort": 443,
            }))
            .unwrap_or_else(|e| panic!("CDP cookie JSON: {e}"))
        }

        #[test]
        fn a_cdp_session_cookie_maps_to_no_expiry() {
            let cookie = cookie_data(&cdp_cookie("s", -1.0, true));
            assert_eq!(
                cookie.expires, None,
                "CDP's -1 for a session cookie is no date"
            );
            assert_eq!(
                cookie.domain, "sso.garmin.com",
                "the cookie keeps its own host"
            );
            assert!(
                cookie.http_only && cookie.secure,
                "the flags map across: {cookie:?}"
            );
        }

        #[test]
        fn a_cdp_persistent_cookie_keeps_its_expiry() {
            let cookie = cookie_data(&cdp_cookie("p", 1_893_456_000.0, false));
            assert_eq!(
                cookie.expires,
                Some(1_893_456_000.0),
                "a persistent cookie keeps CDP's expiry"
            );
        }

        #[test]
        fn no_cookie_dates_the_session_whose_cookies_keep_their_own_expiry() {
            let session = session_from_cookies(&[
                cdp_cookie("late", 1_900_000_000.0, false),
                cdp_cookie("session", -1.0, true),
                cdp_cookie("early", 1_800_000_000.25, false),
            ])
            .unwrap_or_else(|e| panic!("capture: {e}"));
            assert_eq!(session.cookies.len(), 3, "every cookie is captured");
            assert_eq!(
                session.expires_at, None,
                "a short-lived tracker must not sign the session out"
            );
            let early = session.cookies.iter().find(|c| c.name == "early");
            assert_eq!(
                early.and_then(|c| c.expires),
                Some(1_800_000_000.25),
                "the earliest cookie keeps its own expiry: {session:?}"
            );
        }

        #[test]
        fn an_empty_jar_is_an_auth_error() {
            let refused = session_from_cookies(&[]);
            assert!(
                matches!(refused, Err(BrowserError::Auth { .. })),
                "{refused:?}"
            );
        }

        #[test]
        fn the_cookie_param_carries_every_field_and_the_expiry_when_set() {
            let mut cookie = cookie_data(&cdp_cookie("p", 1_893_456_000.0, false));
            let param = cookie_param(&cookie);
            assert_eq!(param.name, "p", "the name is set");
            assert_eq!(param.value, "v", "the value is set");
            assert_eq!(
                param.domain.as_deref(),
                Some("sso.garmin.com"),
                "the domain is set"
            );
            assert_eq!(param.path.as_deref(), Some("/"), "the path is set");
            assert_eq!(param.secure, Some(true), "the secure flag is set");
            assert_eq!(param.http_only, Some(true), "the http-only flag is set");
            assert_eq!(
                param.expires.as_ref().map(|t| *t.inner()),
                Some(1_893_456_000.0),
                "a persistent cookie is injected with its expiry"
            );

            cookie.expires = None;
            assert!(
                cookie_param(&cookie).expires.is_none(),
                "a session cookie sets no expiry"
            );
        }
    }
}
