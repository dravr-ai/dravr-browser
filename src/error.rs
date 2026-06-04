// ABOUTME: Error type for headless-browser automation: launch, navigation, interaction, auth, config
// ABOUTME: Consumers convert BrowserError into their own domain error (RunnerError, ScraperError, etc.)
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

/// Result alias for browser operations.
pub type BrowserResult<T> = Result<T, BrowserError>;

/// Errors arising from headless-browser automation.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    /// Browser launch, connection, or low-level CDP failure.
    #[error("browser error: {reason}")]
    Browser {
        /// Detailed failure reason.
        reason: String,
    },

    /// Page navigation failure.
    #[error("navigation error: {reason}")]
    Navigation {
        /// Detailed failure reason.
        reason: String,
    },

    /// DOM interaction failure (element not found, click/fill failed).
    #[error("interaction error: {reason}")]
    Interaction {
        /// Detailed failure reason.
        reason: String,
    },

    /// Authentication / session failure (no cookies captured, expired session).
    #[error("auth error: {reason}")]
    Auth {
        /// Detailed failure reason.
        reason: String,
    },

    /// Configuration error (invalid launch config, missing values).
    #[error("config error: {reason}")]
    Config {
        /// Detailed failure reason.
        reason: String,
    },

    /// Operation exceeded its deadline.
    #[error("timeout: {reason}")]
    Timeout {
        /// Detailed failure reason.
        reason: String,
    },
}

impl BrowserError {
    /// Construct a [`BrowserError::Browser`].
    pub fn browser(reason: impl Into<String>) -> Self {
        Self::Browser {
            reason: reason.into(),
        }
    }

    /// Construct a [`BrowserError::Navigation`].
    pub fn navigation(reason: impl Into<String>) -> Self {
        Self::Navigation {
            reason: reason.into(),
        }
    }

    /// Construct a [`BrowserError::Interaction`].
    pub fn interaction(reason: impl Into<String>) -> Self {
        Self::Interaction {
            reason: reason.into(),
        }
    }

    /// Construct a [`BrowserError::Auth`].
    pub fn auth(reason: impl Into<String>) -> Self {
        Self::Auth {
            reason: reason.into(),
        }
    }

    /// Construct a [`BrowserError::Config`].
    pub fn config(reason: impl Into<String>) -> Self {
        Self::Config {
            reason: reason.into(),
        }
    }

    /// Construct a [`BrowserError::Timeout`].
    pub fn timeout(reason: impl Into<String>) -> Self {
        Self::Timeout {
            reason: reason.into(),
        }
    }

    /// Whether this error is transient and may succeed on retry.
    #[must_use]
    pub const fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Browser { .. } | Self::Navigation { .. } | Self::Timeout { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_classification() {
        assert!(BrowserError::browser("x").is_transient());
        assert!(BrowserError::timeout("x").is_transient());
        assert!(!BrowserError::auth("x").is_transient());
        assert!(!BrowserError::config("x").is_transient());
    }

    #[test]
    fn display_includes_reason() {
        assert_eq!(
            BrowserError::interaction("no element").to_string(),
            "interaction error: no element"
        );
    }
}
