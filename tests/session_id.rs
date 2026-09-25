// ABOUTME: Pins generate_session_id as CSPRNG output: 128 random bits as 32 lowercase hex digits
// ABOUTME: An id read off the clock shares its prefix with every id minted in the same second
//
// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 dravr.ai

//! A consumer holding many users' sessions reaches one by naming its id, so
//! the id is a bearer. These tests fail on a generator that reads the clock:
//! its ids carry the wall-clock seconds as their leading digits, contain a
//! `-`, and repeat their prefix across ids minted together.

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use dravr_browser::generate_session_id;

/// Enough ids to see a clock-derived generator repeat its prefix.
const SAMPLE: usize = 1000;

/// Mint `SAMPLE` ids, failing the test if the random source does.
fn sample_ids() -> Vec<String> {
    (0..SAMPLE)
        .map(|_| generate_session_id().unwrap_or_else(|e| panic!("mint a session id: {e}")))
        .collect()
}

#[test]
fn an_id_is_32_lowercase_hex_digits() {
    for id in sample_ids() {
        assert_eq!(id.len(), 32, "128 bits as hex: {id}");
        assert!(
            id.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "lowercase hex only, safe as a profile directory and a proxy session name: {id}"
        );
    }
}

#[test]
fn a_thousand_ids_are_all_distinct() {
    let ids = sample_ids();
    let distinct: HashSet<&String> = ids.iter().collect();
    assert_eq!(distinct.len(), SAMPLE, "every id is distinct");
}

#[test]
fn an_id_does_not_derive_from_the_clock() {
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|e| panic!("clock before the epoch: {e}"))
        .as_secs();
    let clock_prefix = format!("{now_secs:x}");

    let ids = sample_ids();
    for id in &ids {
        assert!(
            !id.starts_with(&clock_prefix),
            "an id must not carry the wall-clock seconds: {id}"
        );
    }

    // Ids minted within one second by a clock-derived generator share their
    // leading hex digits; random ones do not.
    let prefixes: HashSet<&str> = ids.iter().map(|id| &id[..8]).collect();
    assert!(
        prefixes.len() > SAMPLE / 2,
        "leading digits vary across ids minted together: {} distinct of {SAMPLE}",
        prefixes.len()
    );
}
