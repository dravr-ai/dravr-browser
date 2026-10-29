// ABOUTME: Drives capture_browser_session in a real headless Chrome against cookies on two hosts
// ABOUTME: The full jar holds both hosts' cookies and their expiry; a page-scoped capture sees only its own
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! A login that crosses hosts leaves cookies on each of them: Garmin's SSO
//! host sets some, the application host others. An HTTP client replaying the
//! session needs all of them, so the capture reads the browser's whole jar.
//! Here the page sits on `127.0.0.1` (served by a one-response local server)
//! while a second cookie lives on `localhost`, a host the page never visits.
//!
//! Chrome comes from `CHROME_PATH`, or chromiumoxide's own discovery when unset.

use chromiumoxide::cdp::browser_protocol::network::{CookieParam, TimeSinceEpoch};
use chrono::{Duration, Utc};
use dravr_browser::session::CookieData;
use dravr_browser::{
    capture_browser_session, capture_session, launch_browser, BrowserLaunchConfig,
};
use tokio::net::TcpListener;

/// A month from now, in whole seconds since the epoch: inside Chrome's
/// 400-day cap on a cookie's lifetime, which clamps any later date.
fn a_month_from_now() -> i64 {
    (Utc::now() + Duration::days(30)).timestamp()
}

/// Serve one empty HTML page on `127.0.0.1`, answering every connection the
/// same way, one connection at a time, and return its URL.
async fn serve_page() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap_or_else(|e| panic!("bind a local port: {e}"));
    let addr = listener
        .local_addr()
        .unwrap_or_else(|e| panic!("read the local port: {e}"));
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            let mut request = [0u8; 4096];
            if stream.read(&mut request).await.is_ok() {
                let body = "<html><body>page</body></html>";
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.ok();
            }
        }
    });
    format!("http://127.0.0.1:{}/", addr.port())
}

fn cookie(name: &str, domain: &str, expires: Option<f64>) -> CookieParam {
    let mut param = CookieParam::new(name, format!("{name}-value"));
    param.domain = Some(domain.to_owned());
    param.path = Some("/".to_owned());
    param.expires = expires.map(TimeSinceEpoch::new);
    param
}

fn named<'a>(cookies: &'a [CookieData], name: &str) -> Option<&'a CookieData> {
    cookies.iter().find(|c| c.name == name)
}

#[tokio::test]
async fn the_full_jar_holds_every_host_while_the_page_sees_its_own() {
    let url = serve_page().await;
    let expires = a_month_from_now() as f64;
    let browser = launch_browser(&BrowserLaunchConfig::default(), None)
        .await
        .unwrap_or_else(|e| panic!("launch headless Chrome: {e}"));
    browser
        .set_cookies(vec![
            cookie("page_host", "127.0.0.1", Some(expires)),
            cookie("other_host", "localhost", None),
        ])
        .await
        .unwrap_or_else(|e| panic!("seed the jar: {e}"));
    let page = browser
        .new_page(url.as_str())
        .await
        .unwrap_or_else(|e| panic!("open the local page: {e}"));

    let full = capture_browser_session(&browser)
        .await
        .unwrap_or_else(|e| panic!("capture the full jar: {e}"));
    let page_scoped = capture_session(&page)
        .await
        .unwrap_or_else(|e| panic!("capture the page's cookies: {e}"));

    let page_cookie = named(&full.cookies, "page_host")
        .unwrap_or_else(|| panic!("the full jar holds the page host's cookie: {full:?}"));
    let other_cookie = named(&full.cookies, "other_host")
        .unwrap_or_else(|| panic!("the full jar holds the other host's cookie: {full:?}"));
    assert_eq!(
        full.cookies.len(),
        2,
        "the full jar holds exactly the two seeded cookies: {full:?}"
    );
    assert_eq!(
        page_cookie.domain, "127.0.0.1",
        "the page host's cookie keeps its host"
    );
    assert_eq!(
        page_cookie.value, "page_host-value",
        "the page host's cookie keeps its value"
    );
    assert_eq!(
        page_cookie.expires,
        Some(expires),
        "a persistent cookie keeps its expiry"
    );
    assert_eq!(
        other_cookie.domain, "localhost",
        "the other host's cookie keeps its host"
    );
    assert_eq!(
        other_cookie.expires, None,
        "a session cookie carries no expiry"
    );
    assert_eq!(
        full.expires_at, None,
        "no cookie's expiry dates the session"
    );

    assert_eq!(
        page_scoped.cookies.len(),
        1,
        "a page-scoped capture leaves out the other host: {page_scoped:?}"
    );
    assert!(
        named(&page_scoped.cookies, "page_host").is_some(),
        "the page sees its own host's cookie: {page_scoped:?}"
    );
}
