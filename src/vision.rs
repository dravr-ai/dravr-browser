// ABOUTME: Vision-LLM seam — the trait consumers implement to supply screenshot analysis
// ABOUTME: Keeps dravr-browser free of any concrete LLM crate, preventing a dependency cycle
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use std::error::Error;

use async_trait::async_trait;

/// Error returned by a [`VisionAnalyzer`] implementation.
///
/// Boxed so this crate stays agnostic to the consumer's error type (an embacle
/// provider error, an HTTP error, etc.).
pub type VisionError = Box<dyn Error + Send + Sync>;

/// A vision-capable LLM reduced to the single operation page automation needs:
/// analyze a screenshot against a prompt and return the model's text reply.
///
/// `dravr-browser` defines this trait so it does **not** depend on any concrete
/// LLM crate. The consumer implements it — typically by wrapping its own LLM
/// provider — and hands it to whatever flow needs a vision fallback.
#[async_trait]
pub trait VisionAnalyzer: Send + Sync {
    /// Analyze a base64-encoded PNG screenshot against `prompt`; return the
    /// model's text response.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying model call fails.
    async fn analyze_screenshot(
        &self,
        prompt: &str,
        screenshot_png_b64: &str,
    ) -> Result<String, VisionError>;
}
