// ABOUTME: Reads the network-capture buffer injected by the stealth hook and parses SSE bodies
// ABOUTME: Provider-agnostic: returns raw captured text + SSE data payloads; JSON shaping lives in consumers
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use chromiumoxide::Page;
use serde::Deserialize;

use crate::error::{BrowserError, BrowserResult};
use crate::stealth::CAPTURE_GLOBAL;

/// Snapshot of the most-recently-captured response.
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureState {
    /// HTTP status of the captured response.
    pub status: u16,
    /// Body captured so far (streamed chunks joined, or the full body).
    pub body: String,
    /// Whether the response has fully arrived (stream closed / body resolved).
    pub done: bool,
    /// Whether this capture is a streamed (incremental) body.
    pub streaming: bool,
}

/// Read the most-recent capture recorded by the stealth hook, if any.
///
/// Returns `Ok(None)` when no matching request has been observed yet.
pub async fn read_last_capture(page: &Page) -> BrowserResult<Option<CaptureState>> {
    let js = format!(
        r"(function() {{
            var s = window.{CAPTURE_GLOBAL};
            if (!s || !s.last) return '';
            var rec = s.byUrl[s.last];
            if (!rec) return '';
            return JSON.stringify({{
                status: rec.status,
                body: rec.chunks.join(''),
                done: rec.done,
                streaming: rec.streaming
            }});
        }})()"
    );

    let result = page.evaluate(js).await.map_err(|e| BrowserError::Browser {
        reason: format!("Failed to read capture buffer: {e}"),
    })?;

    let raw = result
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();

    if raw.is_empty() {
        return Ok(None);
    }

    let state: CaptureState = serde_json::from_str(&raw).map_err(|e| BrowserError::Browser {
        reason: format!("Failed to parse capture state: {e}"),
    })?;
    Ok(Some(state))
}

/// Parse a Server-Sent-Events body into its ordered `data:` payloads.
///
/// Each returned string is one event's concatenated `data:` lines (SSE allows
/// multiple `data:` lines per event, joined with `\n`). Comment lines (`:`),
/// `event:`/`id:`/`retry:` fields, and blank separators are handled per the
/// SSE spec. The terminal `[DONE]` sentinel, if present, is returned verbatim
/// so consumers can recognize it.
#[must_use]
pub fn parse_sse_data(body: &str) -> Vec<String> {
    let mut events = Vec::new();
    let mut current: Vec<String> = Vec::new();

    let flush = |current: &mut Vec<String>, events: &mut Vec<String>| {
        if !current.is_empty() {
            events.push(current.join("\n"));
            current.clear();
        }
    };

    for line in body.lines() {
        if line.is_empty() {
            flush(&mut current, &mut events);
            continue;
        }
        if line.starts_with(':') {
            // SSE comment — ignore.
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            // A single leading space after the colon is part of the syntax.
            current.push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
        }
        // Other fields (event:, id:, retry:) are not needed by consumers here.
    }
    // Trailing event with no blank-line terminator (common mid-stream).
    flush(&mut current, &mut events);

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_events() {
        let body = "data: hello\n\ndata: world\n\n";
        assert_eq!(parse_sse_data(body), vec!["hello", "world"]);
    }

    #[test]
    fn joins_multiline_data() {
        let body = "data: line1\ndata: line2\n\n";
        assert_eq!(parse_sse_data(body), vec!["line1\nline2"]);
    }

    #[test]
    fn ignores_comments_and_other_fields() {
        let body = ": keep-alive\nevent: completion\ndata: {\"x\":1}\n\n";
        assert_eq!(parse_sse_data(body), vec![r#"{"x":1}"#]);
    }

    #[test]
    fn handles_trailing_event_without_blank_line() {
        let body = "data: a\n\ndata: b";
        assert_eq!(parse_sse_data(body), vec!["a", "b"]);
    }

    #[test]
    fn preserves_done_sentinel() {
        let body = "data: {\"type\":\"x\"}\n\ndata: [DONE]\n\n";
        let events = parse_sse_data(body);
        assert_eq!(events.last().map(String::as_str), Some("[DONE]"));
    }

    #[test]
    fn empty_body_yields_no_events() {
        assert!(parse_sse_data("").is_empty());
    }

    #[test]
    fn capture_state_deserializes() {
        let raw = r#"{"status":200,"body":"data: hi\n\n","done":true,"streaming":true}"#;
        let s: CaptureState = serde_json::from_str(raw).unwrap(); // Safe: test with valid inline JSON
        assert_eq!(s.status, 200);
        assert!(s.done);
        assert!(s.streaming);
        assert_eq!(parse_sse_data(&s.body), vec!["hi"]);
    }
}
