// ABOUTME: Headless-Chrome automation primitives shared across dravr crates
// ABOUTME: Launch, persistent profiles, stealth, CDP input, cookie sessions, streaming capture, vision seam
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

#![deny(unsafe_code)]

//! # dravr-browser
//!
//! Reusable headless-Chrome automation primitives extracted so multiple dravr
//! crates share one battle-tested browser stack instead of each re-rolling it.
//!
//! ## What's here
//!
//! - [`launch`] — launch Chrome with a **persistent profile** (cookies survive
//!   across runs) or attach to an external Chrome via CDP.
//! - [`stealth`] — inject anti-detection + an optional network-capture hook,
//!   including a streaming ([`stealth::StealthOptions::capture_stream`]) variant
//!   that tees SSE bodies as they arrive.
//! - [`capture`] — read the capture buffer and parse SSE `data:` payloads.
//! - [`input`] — CDP mouse/keyboard input and DOM helpers.
//! - [`session`] — capture/inject cookie sessions ([`session::AuthSession`]).
//! - [`vision`] — the [`vision::VisionAnalyzer`] seam consumers implement to
//!   supply screenshot analysis without this crate depending on any LLM.
//!
//! This crate is intentionally LLM-agnostic: it never depends on a concrete
//! model crate, so consumers (which may *be* LLM crates) avoid a dependency
//! cycle.

/// Error type for browser automation.
pub mod error;

/// Chrome launch + persistent-profile management.
pub mod launch;

/// Anti-detection stealth + optional network capture hook.
pub mod stealth;

/// Reading the capture buffer and parsing SSE bodies.
pub mod capture;

/// CDP-based input and DOM helpers.
pub mod input;

/// Cookie-based session capture/injection.
pub mod session;

/// The vision-LLM seam.
pub mod vision;

/// JavaScript string-escaping utilities for CDP evaluate calls.
pub mod js_utils;

/// Process-wide browser-teardown signal for WS-reset log suppression.
pub mod teardown_signal;

// Re-export the underlying chromiumoxide handles so consumers can name them
// without taking a direct chromiumoxide dependency (keeping versions in lockstep).
pub use chromiumoxide::{Browser, Page};

pub use capture::{parse_sse_data, read_last_capture, CaptureState};
pub use error::{BrowserError, BrowserResult};
pub use input::{
    cdp_click_at, cdp_insert_text, cdp_select_all_delete, click_element, element_exists,
    fill_input_field, get_element_center, read_visible_text,
};
pub use launch::{launch_browser, open_page_with_stealth, BrowserLaunchConfig, CONNECT_URL_ENV};
pub use session::{capture_session, generate_session_id, inject_cookies, AuthSession, CookieData};
pub use stealth::{apply_stealth, StealthOptions, CAPTURE_GLOBAL};
pub use teardown_signal::{is_in_progress as is_browser_teardown_in_progress, TeardownGuard};
pub use vision::{VisionAnalyzer, VisionError};
