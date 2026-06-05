# dravr-browser

[![crates.io](https://img.shields.io/crates/v/dravr-browser.svg)](https://crates.io/crates/dravr-browser)
[![docs.rs](https://docs.rs/dravr-browser/badge.svg)](https://docs.rs/dravr-browser)
[![CI](https://github.com/dravr-ai/dravr-browser/actions/workflows/ci.yml/badge.svg)](https://github.com/dravr-ai/dravr-browser/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue.svg)](#license)

A headless-Chrome automation library for **driving and scraping real, logged-in
web apps**. It wraps the Chrome DevTools Protocol (via
[`chromiumoxide`](https://crates.io/crates/chromiumoxide)) with the parts you
actually need to automate an authenticated site — without re-implementing login
persistence, bot-detection evasion, and response capture every time.

## What it does

- **Log in once, stay logged in.** Launch Chrome against a persistent on-disk
  profile, so cookies — including Cloudflare's `cf_clearance` and your session —
  survive across runs. Sign in by hand once (headed); subsequent runs are
  headless and already authenticated.
- **Get past bot detection.** Injects stealth tweaks (via CDP, before any page
  script runs) that hide the automation tells Cloudflare and Google sign-in
  check for, so a headless browser loads the real page instead of a challenge.
- **Read the page's own API responses.** A capture hook intercepts the
  `fetch`/`XHR` responses the page makes — including **streaming SSE** bodies as
  they arrive — so you extract data straight from the site's successful API
  calls. (Replaying those calls yourself usually fails: you lose the origin,
  referer, and per-request anti-replay tokens. Reading the page's own fetches
  doesn't.)
- **Drive the page like a human.** CDP-level mouse and keyboard input — click,
  type, fill, locate, read — that real UIs and bot detectors accept.
- **Snapshot and replay sessions.** Capture a logged-in session's cookies into a
  serializable `AuthSession` and inject them into another browser.
- **Vision fallback, LLM-agnostic.** A `VisionAnalyzer` trait you implement to
  analyze page screenshots with your own model when selectors drift — the crate
  itself never depends on any LLM.

Typical uses: scraping data from sites behind a login, or programmatically
driving a web UI (fill a form, submit, read the streamed response).

## Install

```toml
[dependencies]
dravr-browser = "0.1"
```

Requires a Chrome/Chromium binary on the host (auto-detected, or set `CHROME_PATH`).

## Modules

| Module | Purpose |
|--------|---------|
| `launch` | Launch Chrome with a persistent profile, or attach to an external Chrome via CDP (`connect_browser`). |
| `stealth` | Anti-detection JS + an optional network-capture hook (`StealthOptions`), incl. a streaming variant that tees SSE bodies. |
| `capture` | Read the capture buffer (`read_last_capture`) and parse SSE `data:` payloads (`parse_sse_data`). |
| `input` | CDP mouse/keyboard input and DOM helpers (click, fill, locate, read). |
| `session` | Capture / inject cookie sessions (`AuthSession`, `CookieData`). |
| `vision` | The `VisionAnalyzer` seam for screenshot analysis. |
| `teardown_signal` | Suppress chromiumoxide's expected post-close WS-reset error. |

## Log in once, scrape headlessly

```rust,no_run
use dravr_browser::{launch_browser, open_page_with_stealth, BrowserLaunchConfig, StealthOptions};

# async fn example() -> dravr_browser::BrowserResult<()> {
// First run: headed, so you can sign in. Cookies persist under the "my-app" profile.
let config = BrowserLaunchConfig { headless: false, ..Default::default() };
let browser = launch_browser(&config, Some("my-app")).await?;
let page = open_page_with_stealth(
    &browser,
    "https://example.com/login",
    &StealthOptions::stealth_only(),
).await?;
// ... you sign in; the session is now saved in the profile dir ...
# Ok(()) }
```

## Capture the page's streamed API response

```rust,no_run
use dravr_browser::{open_page_with_stealth, parse_sse_data, read_last_capture, StealthOptions};

# async fn example(browser: &dravr_browser::Browser) -> dravr_browser::BrowserResult<()> {
// Tee any response whose URL matches the pattern, including SSE streams.
let stealth = StealthOptions::capture_stream("/api/.*/completion");
let page = open_page_with_stealth(browser, "https://example.com/chat", &stealth).await?;
// ... trigger the page action that fires the request ...
if let Some(state) = read_last_capture(&page).await? {
    for event in parse_sse_data(&state.body) {
        println!("captured SSE event: {event}");
    }
}
# Ok(()) }
```

## Vision seam (bring your own model)

```rust,no_run
use dravr_browser::{VisionAnalyzer, VisionError};

struct MyModel;

#[async_trait::async_trait]
impl VisionAnalyzer for MyModel {
    async fn analyze_screenshot(&self, prompt: &str, png_b64: &str) -> Result<String, VisionError> {
        // forward `prompt` + the screenshot to your own LLM/vision provider …
        # let _ = (prompt, png_b64);
        Ok(String::new())
    }
}
```

## Development

```bash
git submodule update --init --recursive
git config core.hooksPath .build/hooks
cargo test
.build/validation/validate.sh
```

Releases are cut via the `Release` workflow:
`gh workflow run release.yml --field bump=patch`.

## License

Licensed under either of **MIT** or **Apache-2.0** at your option.
