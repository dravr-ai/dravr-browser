// ABOUTME: Drives read_visible_text in a real headless Chrome against pages built for each case
// ABOUTME: Reads the rendered text of every match per part, counts fixed toasts, refuses hidden boxes, <html> and <body>
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! A login reads a refusal through this call, and a refusal it cannot read is
//! a login that waits out its whole budget on a page that already said no. So
//! each case is a page a real form renders: a per-field message that stays
//! hidden ahead of the one that shows, a toast pinned with `position: fixed`,
//! and messages pre-rendered but not painted, which must not read as a
//! refusal. Neither must the page's own `<html>` or `<body>`, whose classes a
//! broad part can match and whose text is the whole document. What a match
//! yields is its rendered text, so a container that covers the page never
//! carries its scripts, its styles or the hints it hides with `display` or
//! `visibility` into the refusal, and an SVG icon, which has no `innerText`,
//! never answers in place of the message beside it.
//!
//! Chrome comes from `CHROME_PATH`, or chromiumoxide's own discovery when unset.

use dravr_browser::{launch_browser, read_visible_text, Browser, BrowserLaunchConfig};

/// The selector `TrainingPeaks`' login uses for its refusals: per-field
/// validation spans, then the summary.
const REFUSAL_SELECTOR: &str = ".field-validation-error, span.error, .validation-summary-errors";

/// The selector Garmin's login uses for its refusals, whose
/// attribute-substring parts can match a class on the page's own `<html>` or
/// `<body>`.
const SUBSTRING_SELECTOR: &str =
    r#".alert-error, [class*="error-message"], [class*="status-error"]"#;

/// A sign-in page whose `<html>` and `<body>` classes between them match every
/// part of [`SUBSTRING_SELECTOR`], followed by `message` inside the page.
fn page_with_matching_root(message: &str) -> String {
    format!(
        "data:text/html,<html class='js error-messages-ready status-error-free'>\
         <head><title>Sign in</title></head>\
         <body class='sso alert-error has-error-message-slot'>\
         <script>window.state = {{step: 'email'}};</script>\
         <form><label>Email</label><input name='email'></form>\
         {message}</body></html>"
    )
}

/// A headless Chrome, on an ephemeral profile.
async fn launch() -> Browser {
    launch_browser(&BrowserLaunchConfig::default(), None)
        .await
        .unwrap_or_else(|e| panic!("launch headless Chrome: {e}"))
}

/// What `read_visible_text` answers for `selector` on `page_url`.
async fn read_on(page_url: &str, selector: &str) -> Option<String> {
    let browser = launch().await;
    let page = browser
        .new_page(page_url)
        .await
        .unwrap_or_else(|e| panic!("open the test page: {e}"));
    read_visible_text(&page, selector).await
}

#[tokio::test]
async fn a_hidden_first_match_does_not_mask_a_visible_one_later_in_the_same_part() {
    // The Username span renders first and stays hidden; the refusal lands on
    // the Password span after it.
    let page = "data:text/html,<form>\
                <input name='Username'><span class='error' style='display:none'>The Username field is required.</span>\
                <input name='Password'><span class='error'>Invalid username or password.</span>\
                </form>";
    assert_eq!(
        read_on(page, REFUSAL_SELECTOR).await.as_deref(),
        Some("Invalid username or password.")
    );
}

#[tokio::test]
async fn a_visible_match_holding_no_text_does_not_end_its_part() {
    let page = "data:text/html,<span class='error' style='display:inline-block;width:12px;height:12px'></span>\
                <span class='error'>Invalid password.</span>";
    assert_eq!(
        read_on(page, "span.error").await.as_deref(),
        Some("Invalid password.")
    );
}

#[tokio::test]
async fn a_toast_pinned_with_position_fixed_reads_as_visible() {
    let page = "data:text/html,<div class='toast' style='position:fixed;top:8px;left:8px'>Too many attempts.</div>";
    assert_eq!(
        read_on(page, ".toast").await.as_deref(),
        Some("Too many attempts.")
    );
}

#[tokio::test]
async fn a_message_laid_out_but_not_painted_reads_as_hidden() {
    // Each box here takes space on the page, which `offsetParent` alone
    // cannot tell from a message on screen.
    let page = "data:text/html,\
                <span class='error' style='visibility:hidden'>Hidden by visibility.</span>\
                <div style='visibility:hidden'><span class='error'>Hidden by a parent's visibility.</span></div>\
                <span class='error' style='opacity:0'>Transparent.</span>\
                <div style='opacity:0'><span class='error'>Transparent through its parent.</span></div>\
                <p class='control'>Control.</p>";
    assert_eq!(
        read_on(page, &format!("{REFUSAL_SELECTOR}, .control"))
            .await
            .as_deref(),
        Some("Control."),
        "every hidden match is examined and skipped, and the read goes on to the control"
    );
}

#[tokio::test]
async fn a_message_not_displayed_reads_as_hidden() {
    let page = "data:text/html,\
                <span class='error' style='display:none'>Not displayed.</span>\
                <div style='display:none'><span class='error'>Parent not displayed.</span></div>\
                <p class='control'>Control.</p>";
    assert_eq!(
        read_on(page, &format!("{REFUSAL_SELECTOR}, .control"))
            .await
            .as_deref(),
        Some("Control."),
        "every hidden match is examined and skipped, and the read goes on to the control"
    );
}

#[tokio::test]
async fn a_message_with_no_size_reads_as_hidden() {
    let page = "data:text/html,<div class='validation-summary-errors' style='height:0;overflow:hidden'>Collapsed.</div>\
                <p class='control'>Control.</p>";
    assert_eq!(
        read_on(page, &format!("{REFUSAL_SELECTOR}, .control"))
            .await
            .as_deref(),
        Some("Control."),
        "the collapsed match is examined and skipped, and the read goes on to the control"
    );
}

#[tokio::test]
async fn a_part_the_browser_cannot_parse_is_skipped() {
    // `querySelectorAll` throws on an unparseable part, and a doubled comma
    // leaves an empty part; neither may cost the read of the parts after them.
    let page = "data:text/html,<span class='error'>Refused.</span>";
    assert_eq!(
        read_on(page, "span:::nope,, span.error").await.as_deref(),
        Some("Refused.")
    );
}

#[tokio::test]
async fn every_match_of_a_part_is_read_before_the_next_part() {
    // The first part's first match is hidden and its second is shown; the
    // second part's only match comes first in the document. Reading one match
    // per part in turn would answer the second part.
    let page = "data:text/html,<span class='b'>Second part.</span>\
                <span class='a' style='display:none'>First part, hidden.</span>\
                <span class='a'>First part, shown.</span>";
    assert_eq!(
        read_on(page, ".a, .b").await.as_deref(),
        Some("First part, shown.")
    );
}

#[tokio::test]
async fn an_earlier_part_wins_over_a_later_part_that_comes_first_in_the_document() {
    let page = "data:text/html,<div class='summary'>Please correct the errors below.</div>\
                <span class='field'>Invalid password.</span>";
    assert_eq!(
        read_on(page, ".field, .summary").await.as_deref(),
        Some("Invalid password.")
    );
}

#[tokio::test]
async fn without_check_visibility_the_computed_style_decides() {
    // The page removes `checkVisibility` before any read, as a browser that
    // predates it would lack it: the hidden and transparent messages in the
    // earlier parts are still skipped, and the fixed toast is still read.
    let url = "data:text/html,<script>delete Element.prototype.checkVisibility;</script>\
                <span class='a' style='visibility:hidden'>Hidden by visibility.</span>\
                <span class='b' style='opacity:0'>Transparent.</span>\
                <span class='c' style='display:none'>Not displayed.</span>\
                <div class='toast' style='position:fixed;top:8px;left:8px'>Too many attempts.</div>";
    let browser = launch().await;
    let page = browser
        .new_page(url)
        .await
        .unwrap_or_else(|e| panic!("open the test page: {e}"));
    let probe = page
        .evaluate("typeof Element.prototype.checkVisibility")
        .await
        .unwrap_or_else(|e| panic!("probe checkVisibility: {e}"));
    assert_eq!(
        probe.value().and_then(|v| v.as_str()),
        Some("undefined"),
        "the page still has checkVisibility, so this case reads nothing it names"
    );
    assert_eq!(
        read_visible_text(&page, ".a, .b, .c, .toast")
            .await
            .as_deref(),
        Some("Too many attempts.")
    );
}

#[tokio::test]
async fn a_selector_carrying_double_quotes_reaches_the_page_intact() {
    let page = "data:text/html,<span data-kind='refusal'>  Account locked.  </span>";
    assert_eq!(
        read_on(page, r#"span[data-kind="refusal"]"#)
            .await
            .as_deref(),
        Some("Account locked.")
    );
}

#[tokio::test]
async fn a_page_root_whose_class_matches_is_passed_over_for_the_message_within() {
    // Each part matches the page's <html> or <body> first; the paragraph the
    // second part also matches is the only message. The second part matches
    // both <html> and <body> ahead of it, so passing over either must not end
    // the read.
    let page =
        page_with_matching_root("<p class='form-error-message'>Invalid email or password.</p>");
    assert_eq!(
        read_on(&page, SUBSTRING_SELECTOR).await.as_deref(),
        Some("Invalid email or password.")
    );
}

#[tokio::test]
async fn a_page_whose_only_matches_are_its_root_and_body_reads_as_none() {
    let page = page_with_matching_root("");
    assert_eq!(read_on(&page, SUBSTRING_SELECTOR).await, None);
}

#[tokio::test]
async fn a_fixed_container_covering_the_page_yields_only_its_rendered_text() {
    // An app shell pinned over the page, whose class a broad part matches,
    // holding a script, a style, a hint not displayed, a hint with
    // `visibility: hidden` and the message.
    let page =
        "data:text/html,<div class='app has-error-message-slot' style='position:fixed;inset:0'>\
                <script>window.state = {step: 'email'};</script>\
                <style>.app{color:inherit}</style>\
                <span style='display:none'>Hidden hint.</span>\
                <span style='visibility:hidden'>Invisible hint.</span>\
                <p>Invalid email or password.</p></div>";
    assert_eq!(
        read_on(page, SUBSTRING_SELECTOR).await.as_deref(),
        Some("Invalid email or password.")
    );
}

#[tokio::test]
async fn a_message_spread_over_blocks_reads_one_line_per_block() {
    let page = "data:text/html,<div class='validation-summary-errors'><ul>\
                <li>Username is required.</li><li>Password is required.</li></ul></div>";
    assert_eq!(
        read_on(page, REFUSAL_SELECTOR).await.as_deref(),
        Some("Username is required.\nPassword is required.")
    );
}

#[tokio::test]
async fn an_icon_matched_ahead_of_the_message_is_passed_over_for_it() {
    // The third part matches the error icon first, in document order; an SVG
    // element has no `innerText`, so its title and styles never answer.
    let page = "data:text/html,<p><svg class='status-error-icon' width='16' height='16'>\
                <title>Error</title><style>.st0{fill:red;}</style>\
                <circle class='st0' cx='8' cy='8' r='8'/></svg> \
                <span class='status-error-text'>Invalid password.</span></p>";
    assert_eq!(
        read_on(page, SUBSTRING_SELECTOR).await.as_deref(),
        Some("Invalid password.")
    );
}

#[tokio::test]
async fn a_selector_matching_nothing_reads_as_none() {
    let page = "data:text/html,<p>Welcome back.</p>";
    assert_eq!(read_on(page, REFUSAL_SELECTOR).await, None);
}
