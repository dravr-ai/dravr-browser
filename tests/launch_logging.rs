// ABOUTME: Captures launch_browser's log events and pins that none carries the profile id
// ABOUTME: The profile id is a consumer's session id, a bearer, so it never reaches a log line
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! A consumer names each persistent profile after a session id, and the same
//! id is substituted into the proxy URL's `{session_id}` placeholder. Both of
//! the launch's debug lines about them used to print it: the profile
//! directory on a failed create, and the proxy id on every proxied launch.
//! This drives both lines — a base directory that cannot hold a profile, a
//! proxy with the placeholder, and a "Chrome" that never answers so the launch
//! ends at its timeout — and reads every event the crate logs.

#![cfg(unix)]

use std::env;
use std::fmt::Debug;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use dravr_browser::{generate_session_id, launch_browser, BrowserLaunchConfig};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{subscriber, Event, Metadata, Subscriber};

/// Every event this crate logs, each rendered as `name=value` pairs.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<String>>>);

impl Captured {
    fn events(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// One event's fields, the message included, as `name=value` pairs.
struct Fields(Vec<String>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.push(format!("{}={value:?}", field.name()));
    }
}

impl Subscriber for Captured {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target().starts_with("dravr_browser")
    }

    fn new_span(&self, _span: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }

    fn record(&self, _span: &Id, _values: &Record<'_>) {}

    fn record_follows_from(&self, _span: &Id, _follows: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields.0.join(" "));
    }

    fn enter(&self, _span: &Id) {}

    fn exit(&self, _span: &Id) {}
}

/// A scratch directory unique to this run, named apart from the profile id so
/// a path the launch logs cannot carry the id by way of the test's own layout.
fn scratch_dir() -> PathBuf {
    let run = generate_session_id().unwrap_or_else(|e| panic!("mint a run id: {e}"));
    let dir = env::temp_dir().join(format!("dravr-browser-log-test-{run}"));
    fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create {}: {e}", dir.display()));
    dir
}

#[tokio::test]
async fn launch_logs_never_carry_the_profile_id() {
    let profile_id = generate_session_id().unwrap_or_else(|e| panic!("mint a session id: {e}"));
    let dir = scratch_dir();

    // A regular file as the base directory: creating `{base}/{profile_id}`
    // under it fails, which is the path that logged the profile directory.
    let base_file = dir.join("not-a-directory");
    fs::write(&base_file, b"").unwrap_or_else(|e| panic!("write the base file: {e}"));

    let fake = dir.join("chrome-that-never-answers.sh");
    fs::write(&fake, "#!/bin/sh\nexec sleep 30\n")
        .unwrap_or_else(|e| panic!("write the fake Chrome: {e}"));
    fs::set_permissions(&fake, fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("make the fake Chrome executable: {e}"));

    let config = BrowserLaunchConfig {
        chrome_path: Some(fake.to_string_lossy().into_owned()),
        profile_base_dir: base_file,
        proxy_url: Some("http://user-{session_id}:secret@proxy.invalid:1234".to_owned()),
        launch_timeout_secs: 1,
        ..BrowserLaunchConfig::default()
    };

    let captured = Captured::default();
    let guard = subscriber::set_default(captured.clone());
    let launched = launch_browser(&config, Some(&profile_id)).await;
    drop(guard);
    assert!(launched.is_err(), "the fake Chrome never answers");

    let events = captured.events();
    for expected in [
        "Failed to create persistent profile dir",
        "Routing browser through proxy",
    ] {
        assert!(
            events.iter().any(|e| e.contains(expected)),
            "the launch logged {expected:?}; events: {events:?}"
        );
    }
    for event in &events {
        assert!(
            !event.contains(&profile_id),
            "a log event carries the profile id: {event}"
        );
        assert!(
            !event.contains("secret"),
            "a log event carries the proxy credentials: {event}"
        );
    }

    fs::remove_dir_all(&dir).unwrap_or_else(|e| panic!("remove {}: {e}", dir.display()));
}
