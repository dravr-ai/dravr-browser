// ABOUTME: Drives tick_checkbox in a real headless Chrome against pages built for each case
// ABOUTME: Ticks a wrapped box, leaves a ticked box alone, refuses a missing box or one a click cannot tick
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! A consent box is the one field a login form refuses to submit without, so
//! each outcome here is one a caller acts on: ticked means the form can go,
//! and a box that is missing or that a click leaves unticked must stop the
//! login rather than let it post a form the site will ignore.
//!
//! Chrome comes from `CHROME_PATH`, or chromiumoxide's own discovery when unset.

use dravr_browser::{launch_browser, tick_checkbox, BrowserError, BrowserLaunchConfig};
use serde_json::{json, Value};

/// Two look-alike rows sharing a class, as `COROS`' login renders them: a
/// "remember me" box first in document order, then the consent box the
/// selector scopes to by its row. Each wraps its input in a `<label>`.
fn consent_page(consent_ticked: bool) -> String {
    let checked = if consent_ticked { " checked" } else { "" };
    format!(
        "data:text/html,<div class='remember-row'><label class='box'><input type='checkbox'></label></div>\
         <div class='consent-row'><label class='box'><input type='checkbox' name=\"consent\"{checked}></label></div>"
    )
}

/// What one tick answered, and the `checked` state of every box on the page
/// afterwards, in document order.
async fn tick_on(page_url: &str, selector: &str) -> (Result<(), BrowserError>, Value) {
    let browser = launch_browser(&BrowserLaunchConfig::default(), None)
        .await
        .unwrap_or_else(|e| panic!("launch headless Chrome: {e}"));
    let page = browser
        .new_page(page_url)
        .await
        .unwrap_or_else(|e| panic!("open the test page: {e}"));
    let ticked = tick_checkbox(&page, selector).await;
    let states = page
        .evaluate("JSON.stringify([...document.querySelectorAll('input[type=checkbox]')].map(c => c.checked))")
        .await
        .unwrap_or_else(|e| panic!("read the boxes: {e}"))
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| panic!("the boxes read back as a JSON string"));
    (
        ticked,
        serde_json::from_str(&states).unwrap_or_else(|e| panic!("boxes JSON: {e}")),
    )
}

#[tokio::test]
async fn the_label_wrapping_the_consent_box_ticks_it_and_leaves_the_look_alike_alone() {
    let (ticked, states) = tick_on(&consent_page(false), ".consent-row .box").await;
    assert!(ticked.is_ok(), "tick answered {ticked:?}");
    assert_eq!(states, json!([false, true]));
}

#[tokio::test]
async fn a_box_already_ticked_is_not_clicked_again() {
    // A click on a ticked box unticks it, so a second click here would leave
    // the consent box empty and the login unable to submit.
    let (ticked, states) = tick_on(&consent_page(true), ".consent-row .box").await;
    assert!(ticked.is_ok(), "tick answered {ticked:?}");
    assert_eq!(states, json!([false, true]));
}

#[tokio::test]
async fn a_selector_carrying_double_quotes_reaches_the_page_intact() {
    // The selector is embedded in a double-quoted JS string, so its own quotes
    // must be escaped rather than end the string early.
    let (ticked, states) = tick_on(&consent_page(false), r#"input[name="consent"]"#).await;
    assert!(ticked.is_ok(), "tick answered {ticked:?}");
    assert_eq!(states, json!([false, true]));
}

#[tokio::test]
async fn a_missing_box_is_an_interaction_error() {
    let (ticked, states) = tick_on(&consent_page(false), ".privacy-row .box").await;
    match ticked {
        Err(BrowserError::Interaction { reason }) => assert_eq!(
            reason,
            "Element not found for checkbox selector: .privacy-row .box"
        ),
        other => panic!("expected an interaction error, got {other:?}"),
    }
    assert_eq!(states, json!([false, false]));
}

#[tokio::test]
async fn an_element_holding_no_checkbox_is_an_interaction_error() {
    let page = "data:text/html,<div class='consent-row'><span class='box'>I agree</span></div>";
    let (ticked, _) = tick_on(page, ".consent-row .box").await;
    match ticked {
        Err(BrowserError::Interaction { reason }) => assert_eq!(
            reason,
            "Element not found for checkbox selector: .consent-row .box"
        ),
        other => panic!("expected an interaction error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_box_a_click_cannot_tick_is_an_interaction_error() {
    // The page cancels the click, so the box is still empty afterwards.
    let page = "data:text/html,<label class='box' onclick='event.preventDefault()'>\
                <input type='checkbox'></label>";
    let (ticked, states) = tick_on(page, ".box").await;
    match ticked {
        Err(BrowserError::Interaction { reason }) => {
            assert_eq!(reason, "Checkbox .box stayed unticked after a click");
        }
        other => panic!("expected an interaction error, got {other:?}"),
    }
    assert_eq!(states, json!([false]));
}
