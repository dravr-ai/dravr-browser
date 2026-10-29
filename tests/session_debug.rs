// ABOUTME: Pins that an AuthSession's Debug names its cookies but never prints their values
// ABOUTME: A cookie value is a live provider session, and sessions reach logs through {:?}
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

use chrono::Utc;
use dravr_browser::session::{AuthSession, CookieData};

const SENTINEL: &str = "sentinel-cookie-value-7f3a";

fn session() -> AuthSession {
    AuthSession {
        session_id: "session-1".to_owned(),
        cookies: vec![CookieData {
            name: "SESSIONID".to_owned(),
            value: SENTINEL.to_owned(),
            domain: "example.com".to_owned(),
            path: "/".to_owned(),
            secure: true,
            http_only: true,
            expires: None,
        }],
        created_at: Utc::now(),
        expires_at: None,
    }
}

#[test]
fn a_session_prints_its_cookie_names_never_their_values() {
    let session = session();
    for printed in [format!("{session:?}"), format!("{session:#?}")] {
        assert!(
            !printed.contains(SENTINEL),
            "a cookie value leaked: {printed}"
        );
        assert!(
            printed.contains("SESSIONID"),
            "the cookie is named: {printed}"
        );
        assert!(
            printed.contains("[REDACTED]"),
            "the value is marked redacted: {printed}"
        );
    }
}

#[test]
fn a_cookie_prints_everything_but_its_value() {
    let cookie = &session().cookies[0];
    let printed = format!("{cookie:?}");
    assert!(
        !printed.contains(SENTINEL),
        "the cookie value leaked: {printed}"
    );
    assert!(
        printed.contains("example.com"),
        "the domain prints: {printed}"
    );
}
