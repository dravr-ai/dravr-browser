// ABOUTME: Chrome launch + persistent-profile management for headless automation
// ABOUTME: Reuses an on-disk profile so cookies (cf_clearance, auth bearers) survive across launches
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use tracing::{debug, info};

use crate::error::{BrowserError, BrowserResult};
use crate::stealth::{apply_stealth, StealthOptions};

/// Environment variable holding a CDP WebSocket URL to attach to an
/// externally-launched Chrome instead of spawning a new one.
pub const CONNECT_URL_ENV: &str = "DRAVR_BROWSER_CONNECT_URL";

/// Configuration for launching (or attaching to) a Chrome browser.
#[derive(Debug, Clone)]
pub struct BrowserLaunchConfig {
    /// Path to a Chrome/Chromium binary. `None` lets chromiumoxide auto-detect.
    pub chrome_path: Option<String>,
    /// Run Chrome headless. Set `false` for interactive (one-time) logins.
    pub headless: bool,
    /// Base directory for persistent per-profile Chrome data. A `profile_id`
    /// resolves to `{profile_base_dir}/{id}`; cookies and `localStorage`
    /// persist there across launches.
    pub profile_base_dir: PathBuf,
    /// Optional `--proxy-server` URL. A literal `{session_id}` placeholder is
    /// replaced with the launch `profile_id` (sticky residential routing).
    pub proxy_url: Option<String>,
    /// Optional override for the `User-Agent` string. `None` selects a
    /// platform-appropriate default.
    pub user_agent: Option<String>,
    /// Enable Chrome's OS-level sandbox. Defaults to `false` because the
    /// primary deployment runs headless as root inside a container, where the
    /// setuid/namespace sandbox cannot initialize and Chrome refuses to start
    /// without `--no-sandbox`. Set to `true` (env
    /// `DRAVR_BROWSER_ENABLE_SANDBOX=1`) on hosts that run as a non-root user
    /// and can support the sandbox — recommended there, since navigation
    /// targets untrusted third-party pages.
    pub enable_sandbox: bool,
}

impl Default for BrowserLaunchConfig {
    fn default() -> Self {
        Self {
            chrome_path: env::var("CHROME_PATH").ok(),
            headless: true,
            profile_base_dir: env::var("DRAVR_BROWSER_PROFILE_DIR").map_or_else(
                |_| env::temp_dir().join("dravr-browser-profiles"),
                PathBuf::from,
            ),
            proxy_url: env::var("DRAVR_BROWSER_PROXY_URL")
                .ok()
                .filter(|s| !s.is_empty()),
            user_agent: env::var("DRAVR_BROWSER_USER_AGENT")
                .ok()
                .filter(|s| !s.is_empty()),
            enable_sandbox: env::var("DRAVR_BROWSER_ENABLE_SANDBOX")
                .ok()
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
        }
    }
}

/// Default `User-Agent` matching the host platform so the fingerprint is
/// consistent with the egress IP (mismatches trigger Cloudflare escalation).
fn default_user_agent() -> &'static str {
    if cfg!(target_os = "linux") {
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
         (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36"
    } else {
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
         (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36"
    }
}

/// Launch a Chrome browser with the given configuration.
///
/// `profile_id` selects the on-disk profile directory:
/// - `Some(id)` — reuses `{config.profile_base_dir}/{id}`. Cookies and
///   storage persist across launches, so an interactive login performed once
///   keeps the session valid for subsequent headless runs.
/// - `None` — ephemeral temp profile under `env::temp_dir()`.
///
/// If [`CONNECT_URL_ENV`] is set, attaches to that externally-launched Chrome
/// via CDP instead of spawning a new process.
pub async fn launch_browser(
    config: &BrowserLaunchConfig,
    profile_id: Option<&str>,
) -> BrowserResult<Browser> {
    if let Ok(connect_url) = env::var(CONNECT_URL_ENV) {
        if !connect_url.is_empty() {
            return connect_browser(&connect_url).await;
        }
    }

    let mut builder = BrowserConfig::builder();

    if config.headless {
        builder = builder.new_headless_mode();
    } else {
        builder = builder
            .with_head()
            .arg(("disable-features", "WebAuthentication"));
    }

    let profile_dir = profile_id.map_or_else(ephemeral_profile_dir, |id| {
        let safe_id: String = id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let dir = config.profile_base_dir.join(&safe_id);
        match fs::create_dir_all(&dir) {
            Ok(()) => dir,
            Err(e) => {
                debug!(error = %e, dir = %dir.display(), "Failed to create persistent profile dir, falling back to ephemeral");
                ephemeral_profile_dir()
            }
        }
    });

    // `.hide()` removes the native `navigator.webdriver=true` Blink injects
    // under headless+CDP and suppresses the automation infobar.
    builder = builder.hide().arg("no-default-browser-check").arg((
        "disable-features",
        "Translate,IsolateOrigins,site-per-process",
    ));

    let user_agent = config
        .user_agent
        .clone()
        .unwrap_or_else(|| default_user_agent().to_owned());

    builder = builder.arg("disable-gpu");

    if !config.enable_sandbox {
        // SECURITY: `--no-sandbox` disables Chrome's OS sandbox, which normally
        // isolates the renderer from the host. It is the DEFAULT here because
        // the primary deployment runs headless as root in a container, where
        // the setuid/namespace sandbox cannot initialize and Chrome refuses to
        // start without this flag. RISK: navigation targets untrusted
        // third-party pages, so a renderer exploit would run with this
        // process's privileges. MITIGATION: run as a non-root user and set
        // `enable_sandbox = true` (`DRAVR_BROWSER_ENABLE_SANDBOX=1`) wherever
        // the host supports the sandbox. Fully automatic detection would need
        // `geteuid(2)` via FFI, which this crate forbids (`unsafe_code = deny`).
        builder = builder.no_sandbox();
    }

    builder = builder
        .arg(("user-agent", user_agent.as_str()))
        .user_data_dir(profile_dir)
        .window_size(1920, 1080);

    if let Some(proxy_url) = config.proxy_url.as_ref() {
        let resolved = profile_id.map_or_else(
            || proxy_url.clone(),
            |id| proxy_url.replace("{session_id}", id),
        );
        builder = builder.arg(("proxy-server", resolved.as_str()));
        debug!(
            proxy_id = %profile_id.unwrap_or("(ephemeral)"),
            "Routing browser through proxy"
        );
    }

    if let Some(ref path) = config.chrome_path {
        builder = builder.chrome_executable(path);
    }

    let browser_config = builder.build().map_err(|e| BrowserError::Browser {
        reason: format!("Failed to configure browser: {e}"),
    })?;

    let (browser, mut handler) =
        Browser::launch(browser_config)
            .await
            .map_err(|e| BrowserError::Browser {
                reason: format!("Failed to launch browser: {e}"),
            })?;

    tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            debug!(?event, "Browser event");
        }
    });

    Ok(browser)
}

/// Redact the per-session token from a CDP WebSocket debugger URL so it is safe
/// to log or embed in an error. The final path segment of a
/// `webSocketDebuggerUrl` (e.g. `.../devtools/browser/<guid>`) is a bearer
/// token on a control channel — anyone holding it can drive the browser — so it
/// is treated like a credential and never emitted in full.
fn redact_ws_url(ws_url: &str) -> String {
    ws_url.rsplit_once('/').map_or_else(
        || "<redacted>".to_owned(),
        |(prefix, _token)| format!("{prefix}/<redacted>"),
    )
}

/// Connect to an externally-launched Chrome via its CDP WebSocket URL.
///
/// `ws_url` is the `webSocketDebuggerUrl` from
/// `http://127.0.0.1:PORT/json/version` when Chrome was launched with
/// `--remote-debugging-port=PORT`. The remote browser's existing pages and
/// cookies remain; callers just open new tabs against the same process.
pub async fn connect_browser(ws_url: &str) -> BrowserResult<Browser> {
    info!(ws_url = %redact_ws_url(ws_url), "Connecting to externally-launched Chrome via CDP");
    let (browser, mut handler) =
        Browser::connect(ws_url.to_owned())
            .await
            .map_err(|e| BrowserError::Browser {
                reason: format!(
                    "Failed to connect to Chrome at {}: {e}",
                    redact_ws_url(ws_url)
                ),
            })?;
    tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            debug!(?event, "Browser event");
        }
    });
    Ok(browser)
}

/// Open a new page with stealth applied before any navigation.
///
/// Opens `about:blank`, registers the stealth payload (which fires on every
/// subsequent frame creation), then navigates to `url`.
pub async fn open_page_with_stealth(
    browser: &Browser,
    url: &str,
    stealth: &StealthOptions,
) -> BrowserResult<chromiumoxide::Page> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|e| BrowserError::Browser {
            reason: format!("Failed to open blank page: {e}"),
        })?;

    apply_stealth(&page, stealth).await?;

    page.goto(url).await.map_err(|e| BrowserError::Navigation {
        reason: format!("Failed to navigate to {url}: {e}"),
    })?;

    Ok(page)
}

/// Build an ephemeral profile path under `env::temp_dir()` with a process-id
/// plus nanosecond suffix to avoid `SingletonLock` conflicts.
fn ephemeral_profile_dir() -> PathBuf {
    env::temp_dir().join(format!(
        "dravr-browser-{}",
        process::id()
            + SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_headless() {
        let cfg = BrowserLaunchConfig::default();
        assert!(cfg.headless);
    }

    #[test]
    fn default_config_disables_sandbox_for_container_compat() {
        let cfg = BrowserLaunchConfig::default();
        assert!(!cfg.enable_sandbox);
    }

    #[test]
    fn redact_ws_url_masks_session_token() {
        let url = "ws://127.0.0.1:9222/devtools/browser/6c1a-secret-token";
        let redacted = redact_ws_url(url);
        assert!(!redacted.contains("6c1a-secret-token"));
        assert_eq!(redacted, "ws://127.0.0.1:9222/devtools/browser/<redacted>");
    }

    #[test]
    fn redact_ws_url_without_path_is_fully_masked() {
        assert_eq!(redact_ws_url("no-slash-here"), "<redacted>");
    }

    #[test]
    fn proxy_placeholder_substitution() {
        let url = "http://user-{session_id}:pass@proxy:1234";
        assert_eq!(
            url.replace("{session_id}", "abc"),
            "http://user-abc:pass@proxy:1234"
        );
    }

    #[test]
    fn ephemeral_dir_is_unique_prefix() {
        let dir = ephemeral_profile_dir();
        assert!(dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("dravr-browser-")));
    }
}
