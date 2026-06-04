# dravr-browser

Reusable headless-Chrome automation primitives, extracted so multiple dravr
crates share one battle-tested browser stack instead of each re-rolling it.

Built on [`chromiumoxide`](https://crates.io/crates/chromiumoxide) (CDP). It is
intentionally **LLM-agnostic** — it never depends on a concrete model crate, so
consumers (which may *be* LLM crates, e.g. `embacle`) avoid a dependency cycle.

## What's here

| Module | Purpose |
|--------|---------|
| `launch` | Launch Chrome with a **persistent profile** (cookies survive across runs) or attach to an external Chrome via CDP. |
| `stealth` | Inject anti-detection JS plus an optional network-capture hook — including a streaming variant that tees SSE bodies as they arrive. |
| `capture` | Read the capture buffer and parse SSE `data:` payloads. |
| `input` | CDP mouse/keyboard input and DOM helpers (click, fill, locate, read). |
| `session` | Capture / inject cookie sessions (`AuthSession`). |
| `vision` | The `VisionAnalyzer` seam consumers implement to supply screenshot analysis without this crate depending on any LLM. |

## Persistent-profile login

```rust,no_run
use dravr_browser::{launch_browser, open_page_with_stealth, BrowserLaunchConfig, StealthOptions};

# async fn example() -> dravr_browser::BrowserResult<()> {
let config = BrowserLaunchConfig { headless: false, ..Default::default() };
let browser = launch_browser(&config, Some("my-profile")).await?;
let page = open_page_with_stealth(&browser, "https://example.com/login", &StealthOptions::stealth_only()).await?;
// ... user signs in; cookies persist in the profile dir for next time ...
# Ok(()) }
```

## Streaming network capture

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

## License

MIT OR Apache-2.0
