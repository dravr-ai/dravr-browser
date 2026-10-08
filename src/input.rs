// ABOUTME: CDP-based page input + element helpers (click, fill, locate, read) for headless automation
// ABOUTME: Uses native CDP mouse/key events so interactions look human to bot-detection scripts
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use chromiumoxide::cdp::browser_protocol::input::{
    DispatchKeyEventParams, DispatchKeyEventType, DispatchMouseEventParams, DispatchMouseEventType,
    InsertTextParams, MouseButton,
};
use chromiumoxide::Page;
use tracing::debug;

use crate::error::{BrowserError, BrowserResult};
use crate::js_utils::escape_js_selector;

/// Click at coordinates `(x, y)` via CDP mouse press + release.
pub async fn cdp_click_at(page: &Page, x: f64, y: f64) -> BrowserResult<()> {
    let press = DispatchMouseEventParams {
        r#type: DispatchMouseEventType::MousePressed,
        x,
        y,
        modifiers: None,
        timestamp: None,
        button: Some(MouseButton::Left),
        buttons: None,
        click_count: Some(1),
        force: None,
        tangential_pressure: None,
        tilt_x: None,
        tilt_y: None,
        twist: None,
        delta_x: None,
        delta_y: None,
        pointer_type: None,
    };
    page.execute(press)
        .await
        .map_err(|e| BrowserError::Interaction {
            reason: format!("CDP mouse press failed: {e}"),
        })?;

    let release = DispatchMouseEventParams {
        r#type: DispatchMouseEventType::MouseReleased,
        x,
        y,
        modifiers: None,
        timestamp: None,
        button: Some(MouseButton::Left),
        buttons: None,
        click_count: Some(1),
        force: None,
        tangential_pressure: None,
        tilt_x: None,
        tilt_y: None,
        twist: None,
        delta_x: None,
        delta_y: None,
        pointer_type: None,
    };
    page.execute(release)
        .await
        .map_err(|e| BrowserError::Interaction {
            reason: format!("CDP mouse release failed: {e}"),
        })?;
    Ok(())
}

/// Select all text in the focused element and delete it via CDP key events.
pub async fn cdp_select_all_delete(page: &Page) {
    let select_all = DispatchKeyEventParams {
        r#type: DispatchKeyEventType::KeyDown,
        modifiers: Some(if cfg!(target_os = "macos") { 4 } else { 2 }),
        timestamp: None,
        text: None,
        unmodified_text: None,
        key_identifier: None,
        code: Some("KeyA".to_owned()),
        key: Some("a".to_owned()),
        windows_virtual_key_code: None,
        native_virtual_key_code: None,
        auto_repeat: None,
        is_keypad: None,
        is_system_key: None,
        location: None,
        commands: None,
    };
    let _ = page.execute(select_all).await;

    let backspace = DispatchKeyEventParams {
        r#type: DispatchKeyEventType::KeyDown,
        modifiers: None,
        timestamp: None,
        text: None,
        unmodified_text: None,
        key_identifier: None,
        code: Some("Backspace".to_owned()),
        key: Some("Backspace".to_owned()),
        windows_virtual_key_code: None,
        native_virtual_key_code: None,
        auto_repeat: None,
        is_keypad: None,
        is_system_key: None,
        location: None,
        commands: None,
    };
    let _ = page.execute(backspace).await;
}

/// Insert text into the currently-focused element via CDP `Input.insertText`.
pub async fn cdp_insert_text(page: &Page, value: &str) -> BrowserResult<()> {
    page.execute(InsertTextParams::new(value))
        .await
        .map_err(|e| BrowserError::Interaction {
            reason: format!("Failed to insert text: {e}"),
        })?;
    Ok(())
}

/// Fill an input field: click-to-focus, clear, then `Input.insertText`.
pub async fn fill_input_field(page: &Page, selector: &str, value: &str) -> BrowserResult<()> {
    let (x, y) = get_element_center(page, selector).await?;

    cdp_click_at(page, x, y).await?;
    cdp_select_all_delete(page).await;
    cdp_insert_text(page, value).await?;

    let _ = page
        .evaluate("document.activeElement.dispatchEvent(new Event('change', {bubbles: true}))")
        .await;

    debug!(selector, "Input field filled via CDP InsertText");
    Ok(())
}

/// Get the center coordinates of the first element matching `selector`.
///
/// `selector` may be a comma-separated list of fallback selectors.
pub async fn get_element_center(page: &Page, selector: &str) -> BrowserResult<(f64, f64)> {
    let escaped = escape_js_selector(selector);
    let js = format!(
        r#"(function() {{
            var selectors = "{escaped}".split(",").map(function(s) {{ return s.trim(); }});
            var el = null;
            for (var i = 0; i < selectors.length; i++) {{
                el = document.querySelector(selectors[i]);
                if (el) break;
            }}
            if (!el) return null;
            var r = el.getBoundingClientRect();
            return JSON.stringify({{x: r.x + r.width / 2, y: r.y + r.height / 2}});
        }})()"#
    );

    let result = page.evaluate(js).await.map_err(|e| BrowserError::Browser {
        reason: format!("Failed to locate '{selector}': {e}"),
    })?;

    let coords_str = result
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| BrowserError::Interaction {
            reason: format!("Element not found for selector: {selector}"),
        })?;

    let coords: serde_json::Value =
        serde_json::from_str(&coords_str).map_err(|e| BrowserError::Browser {
            reason: format!("Failed to parse element coordinates: {e}"),
        })?;

    Ok((
        coords["x"].as_f64().unwrap_or(0.0),
        coords["y"].as_f64().unwrap_or(0.0),
    ))
}

/// Check whether a visible element matching `selector` exists in the DOM.
pub async fn element_exists(page: &Page, selector: &str) -> bool {
    let escaped = escape_js_selector(selector);
    let js = format!(
        r#"(function() {{
            var selectors = "{escaped}".split(",").map(function(s) {{ return s.trim(); }});
            for (var i = 0; i < selectors.length; i++) {{
                var el = document.querySelector(selectors[i]);
                if (el) {{
                    var r = el.getBoundingClientRect();
                    if (r.width > 0 && r.height > 0) return "found";
                }}
            }}
            return "not_found";
        }})()"#
    );
    page.evaluate(js)
        .await
        .ok()
        .and_then(|r| r.value().and_then(|v| v.as_str().map(|s| s == "found")))
        .unwrap_or(false)
}

/// Click the first element matching `selector`.
///
/// Supports comma-separated fallback selectors and a `text:` prefix for
/// matching by visible button/link text (e.g. `text:Sign in with Google`).
pub async fn click_element(page: &Page, selector: &str) -> BrowserResult<()> {
    let escaped_selector = escape_js_selector(selector);
    let js = format!(
        r#"(function() {{
            var parts = "{escaped_selector}".split(",").map(function(s) {{ return s.trim(); }});
            for (var i = 0; i < parts.length; i++) {{
                var sel = parts[i];
                if (sel.indexOf("text:") === 0) {{
                    var text = sel.substring(5);
                    var buttons = document.querySelectorAll("button, a, [role=button]");
                    for (var j = 0; j < buttons.length; j++) {{
                        if (buttons[j].textContent.trim().indexOf(text) !== -1) {{
                            buttons[j].click();
                            return "clicked";
                        }}
                    }}
                }} else {{
                    var el = document.querySelector(sel);
                    if (el) {{ el.click(); return "clicked"; }}
                }}
            }}
            return "not_found";
        }})()"#
    );

    let result = page.evaluate(js).await.map_err(|e| BrowserError::Browser {
        reason: format!("Failed to click '{selector}': {e}"),
    })?;

    let status = result
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();

    if status == "not_found" {
        return Err(BrowserError::Interaction {
            reason: format!("Element not found for selector: {selector}"),
        });
    }

    debug!(selector, "Element clicked");
    Ok(())
}

/// Tick the checkbox `selector` names — the checkbox itself, or the element
/// that wraps it (a `<label>`) — unless it is already ticked.
///
/// `selector` goes to `querySelector` whole, so a comma-separated list matches
/// the first element in document order that fits any part of it. A form that
/// refuses to submit until a consent box is ticked is otherwise a submission
/// that silently never happens, so a box that is missing, or still unticked
/// after the click, is an error rather than a skipped step.
pub async fn tick_checkbox(page: &Page, selector: &str) -> BrowserResult<()> {
    let escaped = escape_js_selector(selector);
    let js = format!(
        r#"(function() {{
            var el = document.querySelector("{escaped}");
            if (!el) return "missing";
            var box = el.matches('input[type="checkbox"]') ? el : el.querySelector('input[type="checkbox"]');
            if (!box) return "missing";
            if (box.checked) return "ticked";
            el.click();
            return box.checked ? "ticked" : "unticked";
        }})()"#
    );

    let result = page.evaluate(js).await.map_err(|e| BrowserError::Browser {
        reason: format!("Failed to tick checkbox '{selector}': {e}"),
    })?;

    let state = result
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();

    match state.as_str() {
        "ticked" => {
            debug!(selector, "Checkbox ticked");
            Ok(())
        }
        "missing" => Err(BrowserError::Interaction {
            reason: format!("Element not found for checkbox selector: {selector}"),
        }),
        _ => Err(BrowserError::Interaction {
            reason: format!("Checkbox {selector} stayed unticked after a click"),
        }),
    }
}

/// Read the trimmed rendered text of the first element `selector` matches
/// that is visible and holds text, or `None` when there is no such element or
/// the page fails to evaluate the read.
///
/// Rendered text is the element's `innerText`: its text as the page lays it
/// out, which leaves out whatever within it is not displayed, a `<script>` or
/// `<style>` among them, or has `visibility: hidden`, and separates the blocks
/// inside it with line breaks. An element without `innerText`, such as an SVG
/// icon, holds no text for this read, so an icon matched ahead of a message is
/// passed over for the message rather than answering with its `<title>` or
/// styles.
///
/// `selector` may be a comma-separated list, read part by part in the order
/// given. Every element a part matches is examined, in document order, before
/// the next part is tried, so a hidden first match never masks a visible one
/// after it. A part the browser cannot parse, or an empty one, is skipped.
/// Parts are split on every comma, so a comma inside `:is()`, `:not()` or an
/// attribute value splits the selector too, and a fragment the browser can
/// still parse is read as a selector of its own, matching a different set of
/// elements than the whole selector would.
///
/// Visible means painted with a size: a non-empty bounding box, and
/// `checkVisibility` with opacity and `visibility` checked — or, in a browser
/// without `checkVisibility`, the element's computed `display`, `visibility`
/// and `opacity`. `offsetParent` is not used: it is null for a
/// `position: fixed` element, such as a toast, that is plainly on screen. An
/// element whose own box is empty reads as hidden even when its floated or
/// absolutely positioned children paint.
///
/// The page's `<html>` and `<body>` are never read, whatever `selector`
/// matches: a message is an element within the page, and their text is the
/// whole page.
pub async fn read_visible_text(page: &Page, selector: &str) -> Option<String> {
    let escaped = escape_js_selector(selector);
    let js = format!(
        r#"(function() {{
            function shown(el) {{
                var box = el.getBoundingClientRect();
                if (box.width === 0 || box.height === 0) return false;
                if (el.checkVisibility) {{
                    return el.checkVisibility({{ checkOpacity: true, checkVisibilityCSS: true }});
                }}
                var style = getComputedStyle(el);
                return style.display !== "none" && style.visibility !== "hidden" && style.opacity !== "0";
            }}
            var parts = "{escaped}".split(",").map(function(s) {{ return s.trim(); }});
            for (var i = 0; i < parts.length; i++) {{
                if (!parts[i]) continue;
                var matches;
                try {{ matches = document.querySelectorAll(parts[i]); }} catch (e) {{ continue; }}
                for (var j = 0; j < matches.length; j++) {{
                    var el = matches[j];
                    if (el === document.documentElement || el === document.body || !shown(el)) continue;
                    var text = typeof el.innerText === "string" ? el.innerText.trim() : "";
                    if (text) return text;
                }}
            }}
            return "";
        }})()"#
    );
    let result = page.evaluate(js).await.ok()?;
    let text = result
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
