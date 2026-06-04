// ABOUTME: CDP-injected JS that hides automation tells and optionally captures matching network responses
// ABOUTME: Applied via Page.addScriptToEvaluateOnNewDocument so it runs before any page JS on every frame
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use chromiumoxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chromiumoxide::Page;

use crate::error::{BrowserError, BrowserResult};

/// Global JS object the capture hook writes into. Read by [`crate::capture`].
pub const CAPTURE_GLOBAL: &str = "__dravrCaptures";

/// Options controlling the injected stealth + capture payload.
#[derive(Debug, Clone, Default)]
pub struct StealthOptions {
    /// When set, responses whose request URL matches this JS regex source are
    /// captured into [`CAPTURE_GLOBAL`]. When `None`, only stealth is applied.
    pub capture_url_pattern: Option<String>,
    /// When `true`, matching responses are captured incrementally by teeing
    /// their `ReadableStream` (for SSE / streamed bodies). When `false`, the
    /// full response body is captured once it resolves.
    pub streaming: bool,
}

impl StealthOptions {
    /// Stealth only — no network capture.
    #[must_use]
    pub const fn stealth_only() -> Self {
        Self {
            capture_url_pattern: None,
            streaming: false,
        }
    }

    /// Capture full (non-streamed) response bodies matching `pattern`.
    #[must_use]
    pub fn capture(pattern: impl Into<String>) -> Self {
        Self {
            capture_url_pattern: Some(pattern.into()),
            streaming: false,
        }
    }

    /// Capture streamed (SSE) response bodies matching `pattern` incrementally.
    #[must_use]
    pub fn capture_stream(pattern: impl Into<String>) -> Self {
        Self {
            capture_url_pattern: Some(pattern.into()),
            streaming: true,
        }
    }
}

/// Build the JS payload for the given options.
///
/// The `navigator.webdriver` tell is already removed by Chrome's
/// `--disable-blink-features=AutomationControlled` flag (emitted by
/// chromiumoxide's `.hide()`), so JS-level navigator spoofing is intentionally
/// omitted — `Object.defineProperty` overrides leave a detectable `.toString()`
/// trace that modern detectors flag. The capture hook is the active payload.
fn build_script(opts: &StealthOptions) -> String {
    let Some(pattern) = opts.capture_url_pattern.as_ref() else {
        // Pure stealth: nothing to inject beyond the launch-flag behavior.
        return "(function(){})();".to_owned();
    };

    // Embed the pattern as a safe JS string literal.
    let pattern_lit = serde_json::to_string(pattern).unwrap_or_else(|_| "\"\"".to_owned());

    let capture_body = if opts.streaming {
        // Tee the ReadableStream: push decoded chunks as they arrive.
        "var rec = { status: r.status, chunks: [], done: false, streaming: true };
                store.byUrl[url] = rec; store.last = url;
                try {
                    var reader = r.clone().body.getReader();
                    var dec = new TextDecoder();
                    (function pump() {
                        reader.read().then(function(res) {
                            if (res.done) { rec.done = true; return; }
                            rec.chunks.push(dec.decode(res.value, { stream: true }));
                            pump();
                        }).catch(function() { rec.done = true; });
                    })();
                } catch (e) { rec.done = true; }"
    } else {
        // Capture the full body once it resolves.
        "var rec = { status: r.status, chunks: [], done: false, streaming: false };
                store.byUrl[url] = rec; store.last = url;
                try {
                    r.clone().text().then(function(t) {
                        rec.chunks.push(t); rec.done = true;
                    }).catch(function() { rec.done = true; });
                } catch (e) { rec.done = true; }"
    };

    format!(
        r"(function() {{
    if (window.{CAPTURE_GLOBAL}) return;
    var store = window.{CAPTURE_GLOBAL} = {{ byUrl: {{}}, last: null }};
    var pattern = new RegExp({pattern_lit});

    var origFetch = window.fetch;
    window.fetch = function(input, init) {{
        var url = typeof input === 'string' ? input : (input && input.url) || '';
        var p = origFetch.apply(this, arguments);
        if (pattern.test(url)) {{
            p.then(function(r) {{
                {capture_body}
                return r;
            }}).catch(function() {{}});
        }}
        return p;
    }};

    var origOpen = XMLHttpRequest.prototype.open;
    var origSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.open = function(method, url) {{
        this.__dravrUrl = url;
        return origOpen.apply(this, arguments);
    }};
    XMLHttpRequest.prototype.send = function() {{
        var self = this;
        var url = this.__dravrUrl || '';
        if (pattern.test(url)) {{
            this.addEventListener('load', function() {{
                try {{
                    store.byUrl[url] = {{
                        status: self.status,
                        chunks: [self.responseText],
                        done: true,
                        streaming: false
                    }};
                    store.last = url;
                }} catch (e) {{}}
            }});
        }}
        return origSend.apply(this, arguments);
    }};
}})();"
    )
}

/// Inject the stealth + capture payload into a page.
///
/// Must be called after `new_page` and before navigation. Runs on every frame
/// creation thereafter, including subsequent `page.goto(...)` calls.
pub async fn apply_stealth(page: &Page, opts: &StealthOptions) -> BrowserResult<()> {
    page.execute(AddScriptToEvaluateOnNewDocumentParams::new(build_script(
        opts,
    )))
    .await
    .map_err(|e| BrowserError::Browser {
        reason: format!("Failed to inject stealth script: {e}"),
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stealth_only_has_no_capture_hook() {
        let js = build_script(&StealthOptions::stealth_only());
        assert!(!js.contains(CAPTURE_GLOBAL));
    }

    #[test]
    fn capture_script_embeds_pattern_and_global() {
        let js = build_script(&StealthOptions::capture("/completion"));
        assert!(js.contains(CAPTURE_GLOBAL));
        assert!(js.contains("/completion"));
        assert!(js.contains("r.clone().text()"));
        assert!(!js.contains("getReader"));
    }

    #[test]
    fn stream_capture_script_tees_reader() {
        let js = build_script(&StealthOptions::capture_stream("/completion"));
        assert!(js.contains("getReader"));
        assert!(js.contains("streaming: true"));
    }

    #[test]
    fn pattern_with_quotes_is_escaped() {
        let js = build_script(&StealthOptions::capture(r#"a"b"#));
        // Embedded as a JSON string literal — the raw unescaped sequence must
        // not appear, proving the quote was escaped.
        assert!(js.contains(r#"a\"b"#));
    }
}
