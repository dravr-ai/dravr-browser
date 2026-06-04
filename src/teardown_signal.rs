// ABOUTME: Process-wide signal that a chromiumoxide browser is being torn down
// ABOUTME: Lets a tracing layer suppress the expected post-close WS-reset error from chromiumoxide
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! Browser-teardown signal published to the rest of the process.
//!
//! `chromiumoxide` drives Chrome over a DevTools-Protocol WebSocket. When the
//! browser closes, Chrome exits without sending the polite WS Close frame, so
//! chromiumoxide's handler task observes a `Ws(Protocol(
//! ResetWithoutClosingHandshake))` and emits an `error!` log. Nothing is
//! actually broken — the work completed before the close. To avoid this
//! lifecycle noise reaching alerting, advertise a short *"teardown in
//! progress"* window via this module; a tracing `Filter` layer can suppress
//! `chromiumoxide::handler` ERROR events **only** while the window is open.
//!
//! - Each [`TeardownGuard::new`] increments the depth counter.
//! - Dropping the guard schedules a task that, after a grace window,
//!   decrements it — covering the gap between `browser.close().await`
//!   returning and the handler task observing the WS reset.
//!
//! The counter is `AtomicU32`, so concurrent teardowns compose.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use tokio::time::sleep;

/// Grace window held open after a guard is dropped so the chromiumoxide
/// handler task can emit its post-close WS-reset error inside the
/// suppression window.
const TEARDOWN_GRACE: Duration = Duration::from_millis(500);

/// Process-wide counter of in-flight browser teardowns. Public **for reading
/// only** via [`is_in_progress`]; treat as opaque from outside.
static TEARDOWN_DEPTH: AtomicU32 = AtomicU32::new(0);

/// Returns `true` while one or more chromiumoxide browsers are inside their
/// teardown grace window.
#[must_use]
pub fn is_in_progress() -> bool {
    TEARDOWN_DEPTH.load(Ordering::Relaxed) > 0
}

/// RAII guard that opens a teardown grace window for the duration of a
/// browser-close call.
///
/// Construct **before** calling `browser.close().await`; let it drop at the
/// end of the close path. Must be created from inside a tokio runtime; the
/// grace-window release uses [`tokio::spawn`].
pub struct TeardownGuard {
    _private: (),
}

impl TeardownGuard {
    /// Open a new teardown window. Returns immediately.
    #[must_use]
    pub fn new() -> Self {
        TEARDOWN_DEPTH.fetch_add(1, Ordering::Relaxed);
        Self { _private: () }
    }
}

impl Default for TeardownGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TeardownGuard {
    fn drop(&mut self) {
        tokio::spawn(async {
            sleep(TEARDOWN_GRACE).await;
            TEARDOWN_DEPTH.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
