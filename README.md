# dravr-browser

[![crates.io](https://img.shields.io/crates/v/dravr-browser.svg)](https://crates.io/crates/dravr-browser)
[![docs.rs](https://docs.rs/dravr-browser/badge.svg)](https://docs.rs/dravr-browser)
[![CI](https://github.com/dravr-ai/dravr-browser/actions/workflows/ci.yml/badge.svg)](https://github.com/dravr-ai/dravr-browser/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](#license)

Reusable headless-Chrome automation primitives, extracted so multiple dravr
crates share one battle-tested browser stack instead of each re-rolling it.

Built on [`chromiumoxide`](https://crates.io/crates/chromiumoxide) (Chrome
DevTools Protocol). It is intentionally **LLM-agnostic** — it never depends on a
concrete model crate, so consumers (which may *be* LLM crates) avoid a
dependency cycle.

**Consumed by:**
- [`embacle`](https://crates.io/crates/embacle) — its `web-ui` feature drives the
  Claude.ai web UI through this crate.
- `dravr-sciotte` — sport-activity scraping (login + capture).

## Install

```toml
[dependencies]
dravr-browser = "0.1"
```

Requires a Chrome/Chromium binary on the host (auto-detected, or set `CHROME_PATH`).

## What's here

| Module | Purpose |
|--------|---------|
| `launch` | Launch Chrome with a **persistent profile** (cookies survive across runs) or attach to an external Chrome via CDP (`connect_browser`). |
| `stealth` | Inject anti-detection JS plus an optional network-capture hook — including a streaming variant that tees SSE bodies as they arrive. |
| `capture` | Read the capture buffer (`read_last_capture`) and parse SSE `data:` payloads (`parse_sse_data`). |
| `input` | CDP mouse/keyboard input and DOM helpers (click, fill, locate, read). |
| `session` | Capture / inject cookie sessions (`AuthSession`, `CookieData`). |
| `vision` | The `VisionAnalyzer` seam consumers implement to supply screenshot analysis **without this crate depending on any LLM**. |
| `teardown_signal` | Process-wide signal to suppress chromiumoxide's expected post-close WS-reset error. |

## Persistent-profile login

A profile reused across launches keeps cookies (incl. Cloudflare `cf_clearance`)
on disk, so an interactive login performed once survives subsequent headless runs.

```rust,no_run
use dravr_browser::{launch_browser, open_page_with_stealth, BrowserLaunchConfig, StealthOptions};

# async fn example() -> dravr_browser::BrowserResult<()> {
let config = BrowserLaunchConfig { headless: false, ..Default::default() };
let browser = launch_browser(&config, Some("my-profile")).await?;
let page = open_page_with_stealth(
    &browser,
    "https://example.com/login",
    &StealthOptions::stealth_only(),
).await?;
// ... user signs in; cookies persist in the profile dir for next time ...
# Ok(()) }
```

## Streaming network capture

The stealth hook can tee a streamed (SSE) response body as it arrives, so you
can read tokens incrementally without scraping the DOM.

```rust,no_run
use dravr_browser::{open_page_with_stealth, parse_sse_data, read_last_capture, StealthOptions};

# async fn example(browser: &dravr_browser::Browser) -> dravr_browser::BrowserResult<()> {
let stealth = StealthOptions::capture_stream("/completion");
let page = open_page_with_stealth(browser, "https://example.com/chat", &stealth).await?;
// ... trigger a request ...
if let Some(state) = read_last_capture(&page).await? {
    for event in parse_sse_data(&state.body) {
        println!("SSE event: {event}");
    }
}
# Ok(()) }
```

## Vision seam

`dravr-browser` stays LLM-free by exposing a trait the consumer implements:

```rust,no_run
use dravr_browser::{VisionAnalyzer, VisionError};

struct MyModel;

#[async_trait::async_trait]
impl VisionAnalyzer for MyModel {
    async fn analyze_screenshot(&self, prompt: &str, png_b64: &str) -> Result<String, VisionError> {
        // forward to your own LLM provider …
        # let _ = (prompt, png_b64);
        Ok(String::new())
    }
}
```

## Development

This repo uses the shared [`dravr-build-config`](https://github.com/dravr-ai/dravr-build-config)
submodule for hooks, lints, and architectural validation. After cloning:

```bash
git submodule update --init --recursive
git config core.hooksPath .build/hooks
cargo test
.build/validation/validate.sh
```

Releases are cut via the `Release` workflow: `gh workflow run release.yml --field bump=patch`.

## License

Licensed under either of **MIT** or **Apache-2.0** at your option.
