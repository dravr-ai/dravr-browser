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
use crate::js_utils::{escape_js_selector, with_selector_list};

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

/// Get the center coordinates of the element `selector` locates.
///
/// `selector` may be a comma-separated list, read the way every helper in this
/// module reads one: split at its top-level commas, part by part in the order
/// given, every match of a part in document order, a part the browser cannot
/// parse skipped. The element is the first match that is rendered — a box, not
/// `display: none` or `visibility: hidden` — or the first match when none is,
/// so a hidden copy of a field rendered ahead of the shown one never takes its
/// place. A transparent element counts as rendered: an input laid with
/// `opacity: 0` over the boxes a code step draws is the one that takes the
/// typing.
///
/// A located element without a box (not displayed, or collapsed) has no point
/// to click, and answers [`BrowserError::Interaction`] like a selector that
/// matches nothing.
pub async fn get_element_center(page: &Page, selector: &str) -> BrowserResult<(f64, f64)> {
    let escaped = escape_js_selector(selector);
    let js = with_selector_list(&format!(
        r#"var el = locateElement("{escaped}");
            if (!el) return "missing";
            var r = el.getBoundingClientRect();
            if (r.width === 0 || r.height === 0) return "unboxed";
            return JSON.stringify({{x: r.x + r.width / 2, y: r.y + r.height / 2}});"#
    ));

    let result = page.evaluate(js).await.map_err(|e| BrowserError::Browser {
        reason: format!("Failed to locate '{selector}': {e}"),
    })?;

    let answer = result
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    match answer.as_str() {
        "missing" | "" => {
            return Err(BrowserError::Interaction {
                reason: format!("Element not found for selector: {selector}"),
            })
        }
        "unboxed" => {
            return Err(BrowserError::Interaction {
                reason: format!("Element for selector {selector} has no box to click"),
            })
        }
        _ => {}
    }

    let coords: serde_json::Value =
        serde_json::from_str(&answer).map_err(|e| BrowserError::Browser {
            reason: format!("Failed to parse element coordinates: {e}"),
        })?;

    Ok((
        coords["x"].as_f64().unwrap_or(0.0),
        coords["y"].as_f64().unwrap_or(0.0),
    ))
}

/// Check whether `selector` matches an element that is rendered on the page
/// for a user to act on: a box, and neither `display: none` nor
/// `visibility: hidden` on it or an ancestor.
///
/// Every match of every part is examined, so a hidden first match never masks
/// a rendered one after it, and a part the browser cannot parse never fails
/// the check of the others. Rendered is the rule every helper here locates an
/// element by (see [`get_element_center`]); a transparent element is rendered,
/// as the input of a code step drawn over its own boxes is.
pub async fn element_exists(page: &Page, selector: &str) -> bool {
    let escaped = escape_js_selector(selector);
    let js = with_selector_list(&format!(
        r#"var matches = selectorMatches("{escaped}");
            for (var i = 0; i < matches.length; i++) {{
                if (isRendered(matches[i])) return "found";
            }}
            return "not_found";"#
    ));
    page.evaluate(js)
        .await
        .ok()
        .and_then(|r| r.value().and_then(|v| v.as_str().map(|s| s == "found")))
        .unwrap_or(false)
}

/// Click the element `selector` locates — the first rendered match, or the
/// first match when none is rendered (see [`get_element_center`]).
///
/// A part with a `text:` prefix matches a button, link or `[role=button]` by
/// its text (e.g. `text:Sign in with Google`).
pub async fn click_element(page: &Page, selector: &str) -> BrowserResult<()> {
    let escaped = escape_js_selector(selector);
    let js = with_selector_list(&format!(
        r#"var el = locateElement("{escaped}");
            if (!el) return "not_found";
            el.click();
            return "clicked";"#
    ));

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
/// The element is the one `selector` locates (see [`get_element_center`]). A
/// form that refuses to submit until a consent box is ticked is otherwise a
/// submission that silently never happens, so a box that is missing, or still
/// unticked after the click, is an error rather than a skipped step.
pub async fn tick_checkbox(page: &Page, selector: &str) -> BrowserResult<()> {
    let escaped = escape_js_selector(selector);
    let js = with_selector_list(&format!(
        r#"var el = locateElement("{escaped}");
            if (!el) return "missing";
            var box = el.matches('input[type="checkbox"]') ? el : el.querySelector('input[type="checkbox"]');
            if (!box) return "missing";
            if (box.checked) return "ticked";
            el.click();
            return box.checked ? "ticked" : "unticked";"#
    ));

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
/// after it. A part the browser cannot parse, or an empty one, is skipped. The
/// list is split at its top-level commas only: a comma inside `:is()`,
/// `:not()` or a quoted attribute value stays in its part.
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
    let js = with_selector_list(&format!(
        r#"var matches = selectorMatches("{escaped}");
            for (var i = 0; i < matches.length; i++) {{
                var el = matches[i];
                if (!isPainted(el)) continue;
                var text = typeof el.innerText === "string" ? el.innerText.trim() : "";
                if (text) return text;
            }}
            return "";"#
    ));
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
