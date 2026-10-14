// ABOUTME: Drives element_exists, get_element_center, click_element and tick_checkbox in a real headless Chrome
// ABOUTME: Every helper reads a selector list one way: top-level parts, every match, painted ahead of hidden
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! A login decides what step it is on from `element_exists`, types into the
//! field `get_element_center` finds and presses what `click_element` finds, so
//! each case here is a page a real form renders: a pre-rendered message that
//! has a box but is not painted, a hidden copy of a field ahead of the shown
//! one, a list whose first part the browser cannot parse, and a part whose own
//! comma sits inside `:is()` or an attribute value. Each helper must answer as
//! `read_visible_text` does for the same page.
//!
//! Chrome comes from `CHROME_PATH`, or chromiumoxide's own discovery when unset.

use dravr_browser::{
    click_element, element_exists, get_element_center, launch_browser, tick_checkbox, Browser,
    BrowserError, BrowserLaunchConfig, Page,
};

/// A headless Chrome, on an ephemeral profile.
async fn launch() -> Browser {
    launch_browser(&BrowserLaunchConfig::default(), None)
        .await
        .unwrap_or_else(|e| panic!("launch headless Chrome: {e}"))
}

/// A page of `browser` showing `url`.
async fn open(browser: &Browser, url: &str) -> Page {
    browser
        .new_page(url)
        .await
        .unwrap_or_else(|e| panic!("open the test page: {e}"))
}

/// The string `expression` evaluates to on `page`.
async fn read_string(page: &Page, expression: &str) -> String {
    page.evaluate(expression)
        .await
        .unwrap_or_else(|e| panic!("evaluate {expression}: {e}"))
        .value()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| panic!("{expression} did not evaluate to a string"))
}

/// A pre-rendered refusal laid out with `visibility: hidden`, then a shown
/// control.
const HIDDEN_THEN_SHOWN: &str = "data:text/html,\
    <div id='a' style='visibility:hidden;width:200px;height:20px'>Invalid password.</div>\
    <div id='b' style='width:200px;height:20px'>Shown.</div>";

#[tokio::test]
async fn a_laid_out_element_hidden_by_visibility_does_not_exist() {
    let browser = launch().await;
    let page = open(&browser, HIDDEN_THEN_SHOWN).await;
    assert!(!element_exists(&page, "#a").await);
    assert!(element_exists(&page, "#b").await);
}

#[tokio::test]
async fn a_transparent_input_over_its_code_boxes_exists_and_takes_the_click() {
    // COROS' code step: six drawn boxes and, laid over them with opacity 0,
    // the input that takes the typing.
    let browser = launch().await;
    let page = open(
        &browser,
        "data:text/html,<div class='code' style='position:relative;width:300px;height:40px'>\
         <button class='cell' style='width:40px;height:40px'></button>\
         <input class='native' style='position:absolute;left:0;top:0;width:300px;height:40px;opacity:0'>\
         </div>",
    )
    .await;
    assert!(element_exists(&page, ".code input").await);
    let (x, y) = get_element_center(&page, ".code input")
        .await
        .unwrap_or_else(|e| panic!("locate the code input: {e}"));
    assert_eq!(
        read_string(
            &page,
            &format!("document.elementFromPoint({x}, {y}).className")
        )
        .await,
        "native",
        "the centre found is a point where the click reaches the input"
    );
}

#[tokio::test]
async fn a_part_the_browser_cannot_parse_does_not_fail_the_parts_after_it() {
    let browser = launch().await;
    let page = open(&browser, HIDDEN_THEN_SHOWN).await;
    // An unclosed bracket: the list is split at every comma, so it cannot
    // swallow the part after it.
    assert!(element_exists(&page, "#bad[, #b").await);
    assert!(!element_exists(&page, "#bad[, #a").await);
    // A part that closes everything it opens and still does not parse.
    assert!(element_exists(&page, "div:::nope, #b").await);
}

#[tokio::test]
async fn a_hidden_first_match_does_not_mask_a_shown_one_after_it() {
    // The mobile copy of the field renders first and is not displayed, as on
    // Strava's login page at a desktop width.
    let browser = launch().await;
    let page = open(
        &browser,
        "data:text/html,<input class='x' style='display:none'>\
         <input class='x' style='display:block;width:100px;height:20px'>",
    )
    .await;
    assert!(element_exists(&page, ".x").await);

    let (x, y) = get_element_center(&page, ".x")
        .await
        .unwrap_or_else(|e| panic!("locate .x: {e}"));
    let expected = read_string(
        &page,
        "(function() { var r = document.querySelectorAll('.x')[1].getBoundingClientRect(); \
         return JSON.stringify([r.x + r.width / 2, r.y + r.height / 2]); })()",
    )
    .await;
    let expected: Vec<f64> =
        serde_json::from_str(&expected).unwrap_or_else(|e| panic!("centre JSON: {e}"));
    assert_eq!(vec![x, y], expected, "the shown copy's centre");
}

#[tokio::test]
async fn a_comma_inside_a_part_stays_in_that_part() {
    // Split at every comma, `:is(#missing, #b)` becomes `:is(#missing` and
    // `#b)`, neither of which parses, and the attribute value splits into two
    // unterminated parts.
    let browser = launch().await;
    let page = open(
        &browser,
        "data:text/html,<div id='b' style='width:50px;height:20px'>B</div>\
         <div data-step='email,password' style='width:50px;height:20px'>Both</div>",
    )
    .await;
    assert!(element_exists(&page, ":is(#missing, #b)").await);
    assert!(element_exists(&page, r#"[data-step="email,password"]"#).await);
    assert!(element_exists(&page, "#gone, div:not(#x, #y)").await);
}

#[tokio::test]
async fn a_click_lands_on_the_shown_copy_of_a_button() {
    let browser = launch().await;
    let page = open(&browser,
        "data:text/html,<button class='go' style='display:none' onclick=\"document.title='hidden'\">Go</button>\
         <button class='go' onclick=\"document.title='shown'\">Go</button>",
    )
    .await;
    click_element(&page, ".go")
        .await
        .unwrap_or_else(|e| panic!("click .go: {e}"));
    assert_eq!(read_string(&page, "document.title").await, "shown");
}

#[tokio::test]
async fn a_text_part_clicks_the_shown_button_holding_that_text() {
    let browser = launch().await;
    let page = open(&browser,
        "data:text/html,<button style='display:none' onclick=\"document.title='hidden'\">Sign in with Google</button>\
         <button onclick=\"document.title='shown'\">Sign in with Google</button>",
    )
    .await;
    click_element(&page, "#absent, text:Sign in with Google")
        .await
        .unwrap_or_else(|e| panic!("click by text: {e}"));
    assert_eq!(read_string(&page, "document.title").await, "shown");
}

#[tokio::test]
async fn a_click_with_only_a_hidden_match_still_reaches_it() {
    // A script click needs no box, so a list whose only match is not shown
    // clicks it rather than answering not found.
    let browser = launch().await;
    let page = open(&browser,
        "data:text/html,<button class='go' style='display:none' onclick=\"document.title='hidden'\">Go</button>",
    )
    .await;
    click_element(&page, ".go")
        .await
        .unwrap_or_else(|e| panic!("click .go: {e}"));
    assert_eq!(read_string(&page, "document.title").await, "hidden");
}

#[tokio::test]
async fn an_element_with_no_box_has_no_centre_to_click() {
    let browser = launch().await;
    let page = open(
        &browser,
        "data:text/html,<input class='x' style='display:none'><p>Other</p>",
    )
    .await;
    match get_element_center(&page, ".x").await {
        Err(BrowserError::Interaction { reason }) => {
            assert_eq!(reason, "Element for selector .x has no box to click");
        }
        other => panic!("expected an interaction error, got {other:?}"),
    }
    match get_element_center(&page, ".absent").await {
        Err(BrowserError::Interaction { reason }) => {
            assert_eq!(reason, "Element not found for selector: .absent");
        }
        other => panic!("expected an interaction error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_tick_reaches_the_shown_box_past_a_hidden_copy() {
    let browser = launch().await;
    let page = open(&browser,
        "data:text/html,<label class='consent' style='display:none'><input type='checkbox'></label>\
         <label class='consent'><input type='checkbox'></label>",
    )
    .await;
    tick_checkbox(&page, ".consent")
        .await
        .unwrap_or_else(|e| panic!("tick .consent: {e}"));
    assert_eq!(
        read_string(
            &page,
            "JSON.stringify([...document.querySelectorAll('input')].map(c => c.checked))"
        )
        .await,
        "[false,true]"
    );
}
