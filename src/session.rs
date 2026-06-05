// ABOUTME: Cookie-based browser session capture/injection and the AuthSession data model
// ABOUTME: Lets a profile's authenticated cookies be snapshotted and replayed across browsers
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use std::time::SystemTime;

use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chromiumoxide::Page;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A single browser cookie.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

/// Inject session cookies into a page before navigation.
pub async fn inject_cookies(page: &Page, session: &AuthSession) -> BrowserResult<()> {
    for cookie in &session.cookies {
        let mut param = CookieParam::new(&cookie.name, &cookie.value);
        param.domain = Some(cookie.domain.clone());
        param.path = Some(cookie.path.clone());
        param.secure = Some(cookie.secure);
        param.http_only = Some(cookie.http_only);

        page.set_cookie(param)
            .await
            .map_err(|e| BrowserError::Browser {
                reason: format!("Failed to set cookie {}: {e}", cookie.name),
            })?;
    }

    debug!(count = session.cookies.len(), "Injected session cookies");
    Ok(())
}

/// Capture all cookies from the current page into an [`AuthSession`].
///
/// Returns [`BrowserError::Auth`] if no cookies are present (login likely failed).
pub async fn capture_session(page: &Page) -> BrowserResult<AuthSession> {
    let cookies = page
        .get_cookies()
        .await
        .map_err(|e| BrowserError::Browser {
            reason: format!("Failed to get cookies: {e}"),
        })?;

    let cookie_data: Vec<CookieData> = cookies
        .iter()
        .map(|c| CookieData {
            name: c.name.clone(),
            value: c.value.clone(),
            domain: c.domain.clone(),
            path: c.path.clone(),
            secure: c.secure,
            http_only: c.http_only,
        })
        .collect();

    if cookie_data.is_empty() {
        return Err(BrowserError::Auth {
            reason: "No cookies captured after login".to_owned(),
        });
    }

    Ok(AuthSession {
        session_id: generate_session_id(),
        cookies: cookie_data,
        created_at: Utc::now(),
        expires_at: None,
    })
}

/// Generate a unique session identifier from the current system time.
#[must_use]
pub fn generate_session_id() -> String {
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}-{:x}", d.as_secs(), d.subsec_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_is_nonempty_and_hyphenated() {
        let id = generate_session_id();
        assert!(id.contains('-'));
        assert!(!id.starts_with('-'));
    }

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
}
