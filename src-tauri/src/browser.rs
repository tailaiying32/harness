//! The side panel's browser: one web view per chat, for localhost pages, that
//! both the user and the agent use. The agent drives it through tools (see
//! `tools()` and `call()`); nothing here knows which agent that is.
//!
//! The page is a second web view the main window holds, placed over the side
//! panel's body. When it isn't on screen it is parked off to the side at the
//! same size, so what the agent reads and sees matches what the user would.
//!
//! The agent's actions run as scripts inside the page (`BRIDGE_JS`), started
//! with `eval` and read back with `eval_with_callback`. The page gets no way to
//! call into the app. Only screenshots need code for each system.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl};
use tokio::sync::oneshot;

use crate::events::{AgentEvent, Envelope};
use crate::sessions::Out;

/// Name the agent knows the browser's tools by.
pub const SERVER: &str = "harness_browser";

/// A line for the agent's instructions.
pub const INSTRUCTION: &str = "This app has a browser in its side panel for localhost pages. To \
check a web UI you are building, use the harness_browser tools (open, read, click, type, console, \
screenshot, resize); the user sees the same page. The panel is narrow, so pages show their \
small-screen layout; use resize (e.g. 1440) to check the desktop layout.";

/// How long an action may take before it is given up.
const ACTION_WAIT: Duration = Duration::from_secs(15);
/// How long `open` waits for a page to load.
const LOAD_WAIT: Duration = Duration::from_secs(20);
/// How often, during an action, to check that the page hasn't taken the
/// window's keys (see `keep_keys`).
const KEYS_POLL: Duration = Duration::from_millis(30);
/// How long to keep checking once the action is done: WebKit can hand the
/// keys over a moment later.
const KEYS_TAIL: Duration = Duration::from_millis(500);
/// After a click: how long to watch for a page load it started.
const CLICK_SETTLE: Duration = Duration::from_millis(400);
/// Where a hidden browser is parked: far left of the window, out of sight.
const PARK_X: f64 = -20_000.0;
/// How long a browser may sit off screen and unused before its page is put
/// away (`HARNESS_BROWSER_IDLE_SECONDS` overrides it, for tests).
const PUT_AWAY_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often put-away is checked.
const SWEEP_EVERY: Duration = Duration::from_secs(30);
/// Narrowest and widest layout width `resize` takes, in CSS pixels.
const MIN_WIDTH: f64 = 320.0;
const MAX_WIDTH: f64 = 3840.0;
/// Longest image edge sent to the agent, in pixels (larger ones are shrunk by it anyway).
const SHOT_MAX_EDGE: f64 = 1568.0;

/// The tools, as MCP tool definitions.
pub fn tools() -> Value {
    let none = json!({ "type": "object", "properties": {} });
    json!([
        {
            "name": "open",
            "description": "Open a localhost page in the side panel's browser (e.g. http://localhost:5173) and wait for it to load. Only localhost, 127.0.0.1 and [::1] are allowed. Opening the same address again reloads it.",
            "inputSchema": {
                "type": "object",
                "properties": { "url": { "type": "string", "description": "Address, e.g. localhost:5173/settings" } },
                "required": ["url"],
            },
        },
        {
            "name": "read",
            "description": "Read the open page: its address, title, visible text, and a numbered list of the buttons, links and fields on it, each with a selector to pass to click or type.",
            "inputSchema": none,
        },
        {
            "name": "click",
            "description": "Click an element, picked by CSS selector (such as one from read) or by its visible text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "text": { "type": "string", "description": "Visible text or label of the element, used when no selector is given" },
                },
            },
        },
        {
            "name": "type",
            "description": "Replace the value of a field (input, textarea, select or editable element) with text, picked by CSS selector. Set submit to press Enter / submit its form afterwards.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "text": { "type": "string" },
                    "submit": { "type": "boolean" },
                },
                "required": ["selector", "text"],
            },
        },
        {
            "name": "console",
            "description": "Errors and warnings the page logged (console.error/warn, uncaught errors, failed loads of scripts and images) since the last open or console call. Kept across reloads.",
            "inputSchema": none,
        },
        {
            "name": "run",
            "description": "Run JavaScript in the page and return its result as JSON. An expression is returned as is; a function body needs `return`. Can be async (await).",
            "inputSchema": {
                "type": "object",
                "properties": { "script": { "type": "string" } },
                "required": ["script"],
            },
        },
        {
            "name": "screenshot",
            "description": "A picture of the page as shown in the side panel, to check how the UI looks.",
            "inputSchema": none,
        },
        {
            "name": "resize",
            "description": "Set how wide the page lays out, in CSS pixels, without changing the panel: the page is zoomed to fit, so e.g. 1440 shows the full desktop layout, smaller. Stays until changed, across loads. Omit width (or 0) to go back to the panel's own width.",
            "inputSchema": {
                "type": "object",
                "properties": { "width": { "type": "number", "description": "Layout width in CSS pixels, e.g. 1440 for desktop or 390 for a phone" } },
            },
        },
    ])
}

/// Turns what the agent or user typed into an allowed address, or says why not.
/// Allowed: http(s) on localhost, 127.0.0.1 or [::1], any port. `localhost:5173`
/// and `:5173` get `http://` added.
pub fn local_url(input: &str) -> Result<tauri::Url, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("No address given.".into());
    }
    let full = if input.starts_with(':') {
        format!("http://localhost{input}")
    } else if input.contains("://") {
        input.to_string()
    } else {
        format!("http://{input}")
    };
    let url = tauri::Url::parse(&full).map_err(|_| format!("Not an address: {input}"))?;
    if is_local(&url) {
        Ok(url)
    } else {
        Err(format!(
            "Only localhost pages can be opened here (localhost, 127.0.0.1 or [::1]), not {input}."
        ))
    }
}

/// A page the browser may show: localhost over http(s), or a blank page.
fn is_local(url: &tauri::Url) -> bool {
    if url.as_str() == "about:blank" {
        return true;
    }
    matches!(url.scheme(), "http" | "https")
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

/// Where the panel's body is, in the window's logical pixels.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

struct Page {
    webview: Webview,
    /// Last time the agent used it or the user had it on screen.
    used: Instant,
    loading: bool,
    url: String,
    title: String,
    /// Waiting for the current load to finish.
    load_waiters: Vec<oneshot::Sender<()>>,
    /// The page's native view, by address, to tell whether it holds the
    /// window's keys (see `keep_keys`). 0 where that isn't checked.
    view: usize,
}

/// Where the window wants a chat's browser. Kept apart from the page, so it
/// holds before the page is made and while it is put away.
#[derive(Clone, Copy, Default)]
struct Place {
    /// The panel body; None until the window has placed it.
    rect: Option<Rect>,
    visible: bool,
    /// Width the agent wants the page laid out at (`resize`); None fits the panel.
    width: Option<f64>,
}

impl Place {
    /// Page zoom that lays the page out at `width` inside the panel.
    fn zoom(&self) -> f64 {
        match (self.width, self.rect) {
            (Some(width), Some(rect)) if rect.width > 0.0 => rect.width / width,
            _ => 1.0,
        }
    }
}

/// A page put away after sitting unused: what it showed, to bring it back.
struct Away {
    url: String,
    title: String,
}

struct Inner {
    app: AppHandle,
    out: Out,
    pages: Mutex<HashMap<String, Page>>,
    places: Mutex<HashMap<String, Place>>,
    away: Mutex<HashMap<String, Away>>,
    /// Numbers web view labels, which can't hold every character a chat ID may.
    next: Mutex<u64>,
    /// Numbers actions run in pages.
    next_action: Mutex<u64>,
    /// One web view made at a time, so two calls can't both make one.
    make_lock: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct Browsers(Arc<Inner>);

impl Browsers {
    pub fn new(app: AppHandle, out: Out) -> Self {
        let browsers = Self(Arc::new(Inner {
            app,
            out,
            pages: Mutex::default(),
            places: Mutex::default(),
            away: Mutex::default(),
            next: Mutex::new(0),
            next_action: Mutex::new(0),
            make_lock: tokio::sync::Mutex::new(()),
        }));
        // Put away pages left unused; ends with the app.
        let weak = Arc::downgrade(&browsers.0);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(SWEEP_EVERY).await;
                let Some(inner) = weak.upgrade() else { return };
                Browsers(inner).put_away_unused(put_away_after());
            }
        });
        browsers
    }

    /// Closes the pages of browsers off screen and unused for `after`, keeping
    /// their address: going back to one (the user, or the agent's next tool
    /// call) loads it again. The window isn't told; its tab and address stay.
    fn put_away_unused(&self, after: Duration) {
        let visible: Vec<String> = {
            let places = self.0.places.lock().unwrap();
            places.iter().filter(|(_, p)| p.visible).map(|(chat, _)| chat.clone()).collect()
        };
        let stale: Vec<(String, Page)> = {
            let mut pages = self.0.pages.lock().unwrap();
            let now = Instant::now();
            for chat in &visible {
                if let Some(page) = pages.get_mut(chat) {
                    page.used = now;
                }
            }
            let chats: Vec<String> = pages
                .iter()
                .filter(|(_, p)| !p.loading && p.load_waiters.is_empty() && now.duration_since(p.used) >= after)
                .map(|(chat, _)| chat.clone())
                .collect();
            chats.into_iter().filter_map(|c| pages.remove(&c).map(|p| (c, p))).collect()
        };
        let mut away = self.0.away.lock().unwrap();
        for (chat, page) in stale {
            let _ = page.webview.close();
            away.insert(
                chat,
                Away {
                    url: page.url,
                    title: page.title,
                },
            );
        }
    }

    /// Brings a put-away page back, waiting for it to load when `wait` is set.
    /// False if the chat had nothing put away.
    async fn bring_back(&self, chat: &str, wait: bool) -> Result<bool, String> {
        let Some(away) = self.0.away.lock().unwrap().remove(chat) else {
            return Ok(false);
        };
        let url = tauri::Url::parse(&away.url).map_err(|e| e.to_string())?;
        self.load(chat, url, wait).await?;
        Ok(true)
    }

    /// The page is in use now.
    fn touch(&self, chat: &str) {
        if let Some(page) = self.0.pages.lock().unwrap().get_mut(chat) {
            page.used = Instant::now();
        }
    }

    fn send(&self, chat: &str, event: AgentEvent) {
        (self.0.out)(Envelope {
            session_id: chat.to_string(),
            event,
        });
    }

    fn state_event(&self, chat: &str) {
        let event = {
            let pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get(chat) else { return };
            AgentEvent::BrowserState {
                url: page.url.clone(),
                title: page.title.clone(),
                loading: page.loading,
            }
        };
        self.send(chat, event);
    }

    /// Lays the chat's page out `width` CSS pixels wide (None: the panel's
    /// width), by zooming it to fit the panel. Says what it did.
    fn resize(&self, chat: &str, width: Option<f64>) -> Result<String, String> {
        let width = width.filter(|w| *w > 0.0).map(|w| w.round().clamp(MIN_WIDTH, MAX_WIDTH));
        let place = {
            let mut places = self.0.places.lock().unwrap();
            let place = places.entry(chat.to_string()).or_default();
            place.width = width;
            *place
        };
        if let Some(webview) = self.webview(chat) {
            apply_place(&webview, place);
        }
        let panel = place.rect.map(|r| r.width.round());
        Ok(match (width, panel) {
            (None, Some(panel)) => format!("The page lays out at the panel's width again ({panel} px)."),
            (None, None) => "The page lays out at the panel's width again.".into(),
            (Some(w), Some(_)) => format!(
                "The page now lays out {w} px wide, shown at {:.0}% to fit the panel.",
                place.zoom() * 100.0
            ),
            (Some(w), None) => format!("The page will lay out {w} px wide once the panel shows."),
        })
    }

    fn webview(&self, chat: &str) -> Option<Webview> {
        self.0.pages.lock().unwrap().get(chat).map(|p| p.webview.clone())
    }

    // ---- Called by the window ----

    /// The user typed an address. Makes the chat's browser if needed.
    pub async fn navigate(&self, chat: &str, input: &str) -> Result<(), String> {
        let url = local_url(input)?;
        self.load(chat, url, false).await.map(|_| ())
    }

    pub async fn reload(&self, chat: &str) -> Result<(), String> {
        if self.bring_back(chat, false).await? {
            return Ok(());
        }
        let webview = self.webview(chat).ok_or("No page open")?;
        self.touch(chat);
        webview.reload().map_err(|e| e.to_string())
    }

    /// Where the panel body is, and whether the browser should show there. A
    /// put-away page shown again is loaded again.
    pub fn place(&self, chat: &str, rect: Rect, visible: bool) {
        let place = {
            let mut places = self.0.places.lock().unwrap();
            let place = places.entry(chat.to_string()).or_default();
            if rect.width > 0.0 && rect.height > 0.0 {
                place.rect = Some(rect);
            }
            place.visible = visible;
            *place
        };
        if let Some(webview) = self.webview(chat) {
            apply_place(&webview, place);
            if visible {
                self.touch(chat);
            }
        } else if visible && self.0.away.lock().unwrap().contains_key(chat) {
            let browsers = self.clone();
            let chat = chat.to_string();
            tauri::async_runtime::spawn(async move {
                let _ = browsers.bring_back(&chat, false).await;
            });
        }
    }

    /// Closes the chat's browser, if it has one, put away or not.
    pub fn close(&self, chat: &str) {
        let page = self.0.pages.lock().unwrap().remove(chat);
        let away = self.0.away.lock().unwrap().remove(chat);
        self.0.places.lock().unwrap().remove(chat);
        if let Some(page) = &page {
            let _ = page.webview.close();
        }
        if page.is_some() || away.is_some() {
            self.send(chat, AgentEvent::BrowserClosed {});
        }
    }

    /// The chat's browser, if any (put away counts), for a chat the window loads later.
    pub fn current(&self, chat: &str) -> Option<(String, String, bool)> {
        if let Some(p) = self.0.pages.lock().unwrap().get(chat) {
            return Some((p.url.clone(), p.title.clone(), p.loading));
        }
        let away = self.0.away.lock().unwrap();
        away.get(chat).map(|a| (a.url.clone(), a.title.clone(), false))
    }

    // ---- Loading ----

    /// Shows `url` in the chat's browser, making it first if needed, and waits
    /// for the load when `wait` is set. Returns the page's address and title.
    async fn load(&self, chat: &str, url: tauri::Url, wait: bool) -> Result<(String, String), String> {
        // A new address replaces whatever was put away.
        self.0.away.lock().unwrap().remove(chat);
        let done = {
            let _guard = self.0.make_lock.lock().await;
            let (tx, rx) = oneshot::channel();
            let mut tx = Some(tx);
            let existing = {
                let mut pages = self.0.pages.lock().unwrap();
                match pages.get_mut(chat) {
                    Some(page) => {
                        page.used = Instant::now();
                        page.load_waiters.extend(tx.take());
                        page.loading = true;
                        page.url = url.to_string();
                        Some(page.webview.clone())
                    }
                    None => None,
                }
            };
            match existing {
                Some(webview) => {
                    // Errors logged before this open aren't news to the agent.
                    webview
                        .eval(format!("try{{sessionStorage.removeItem({LOG_KEY:?})}}catch(e){{}}"))
                        .ok();
                    webview.navigate(url.clone()).map_err(|e| e.to_string())?;
                }
                None => self.make(chat, url.clone(), tx.take().expect("unused")).await?,
            }
            rx
        };
        self.state_event(chat);
        if wait {
            match tokio::time::timeout(LOAD_WAIT, done).await {
                Ok(_) => {}
                Err(_) => return Err(format!("{url} didn't finish loading in {} seconds.", LOAD_WAIT.as_secs())),
            }
        }
        let pages = self.0.pages.lock().unwrap();
        let page = pages.get(chat).ok_or("The browser was closed")?;
        Ok((page.url.clone(), page.title.clone()))
    }

    /// Makes a chat's browser showing `url`. `loaded` fires when the first load ends.
    async fn make(&self, chat: &str, url: tauri::Url, loaded: oneshot::Sender<()>) -> Result<(), String> {
        let window = self.0.app.get_window("main").ok_or("The main window is gone")?;
        let label = {
            let mut next = self.0.next.lock().unwrap();
            *next += 1;
            format!("browser-{}", *next)
        };
        let weak: Weak<Inner> = Arc::downgrade(&self.0);
        let chat_id = chat.to_string();
        let app = self.0.app.clone();
        // Starts blank: the page's scripts are set up before the first real load.
        let blank = tauri::Url::parse("about:blank").expect("valid");
        let builder = WebviewBuilder::new(&label, WebviewUrl::External(blank))
            .initialization_script(BRIDGE_JS)
            .on_navigation(move |url| {
                if is_local(url) {
                    return true;
                }
                // Any other site opens in the user's browser, never in here.
                use tauri_plugin_opener::OpenerExt;
                let _ = app.opener().open_url(url.as_str(), None::<&str>);
                false
            })
            .on_page_load(move |webview, payload| {
                let Some(inner) = weak.upgrade() else { return };
                Browsers(inner).page_loaded(&chat_id, webview, payload.url().to_string(), payload.event());
            });

        // Parked until the window says where the panel is.
        let size = window
            .inner_size()
            .ok()
            .zip(window.scale_factor().ok())
            .map(|(s, scale)| s.to_logical::<f64>(scale))
            .map(|s| LogicalSize::new((s.width * 0.46).max(320.0), (s.height - 80.0).max(320.0)))
            .unwrap_or(LogicalSize::new(640.0, 800.0));
        // `add_child` waits on the main thread; keep it off the async workers.
        let webview = tauri::async_runtime::spawn_blocking(move || {
            window.add_child(builder, LogicalPosition::new(PARK_X, 0.0), size)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Couldn't open the browser: {e}"))?;

        // Tauri gives every web view its own app hooks (`window.__TAURI_INTERNALS__`,
        // `isTauri`), locked in place. In this browser they would make a Tauri
        // app's dev page, Harness's own included, think it runs inside Harness.
        // On Mac they are taken out, leaving only the bridge; elsewhere they stay
        // for now (calls through them are refused: no capability covers this view).
        #[cfg(target_os = "macos")]
        let view = {
            let (tx, rx) = oneshot::channel::<usize>();
            let _ = webview.with_webview(move |platform| {
                mac::only_bridge(platform.inner(), BRIDGE_JS);
                let _ = tx.send(platform.inner() as usize);
            });
            rx.await.unwrap_or(0)
        };
        #[cfg(not(target_os = "macos"))]
        let view = 0;

        self.0.pages.lock().unwrap().insert(
            chat.to_string(),
            Page {
                webview: webview.clone(),
                used: Instant::now(),
                loading: true,
                url: url.to_string(),
                title: String::new(),
                load_waiters: vec![loaded],
                view,
            },
        );
        // Where the window last wanted it, if it said already.
        let place = self.0.places.lock().unwrap().get(chat).copied();
        if let Some(place) = place {
            apply_place(&webview, place);
        }
        webview.navigate(url).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn page_loaded(&self, chat: &str, webview: Webview, url: String, event: PageLoadEvent) {
        // The blank page a browser starts on isn't news.
        if url == "about:blank" {
            return;
        }
        {
            let mut pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get_mut(chat) else { return };
            page.url = url;
            match event {
                PageLoadEvent::Started => page.loading = true,
                PageLoadEvent::Finished => page.loading = false,
            }
        }
        if let PageLoadEvent::Started = event {
            self.state_event(chat);
            return;
        }
        // Read the title, then tell the window and anyone waiting.
        let browsers = self.clone();
        let chat_id = chat.to_string();
        let chat = chat_id.clone();
        let asked = webview.eval_with_callback("document.title", move |title| {
            browsers.title_read(&chat, &serde_json::from_str::<String>(&title).unwrap_or_default());
        });
        if asked.is_err() {
            self.title_read(&chat_id, "");
        }
    }

    fn title_read(&self, chat: &str, title: &str) {
        let waiters = {
            let mut pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get_mut(chat) else { return };
            page.title = title.to_string();
            if page.loading {
                Vec::new()
            } else {
                std::mem::take(&mut page.load_waiters)
            }
        };
        for waiter in waiters {
            let _ = waiter.send(());
        }
        self.state_event(chat);
    }

    // ---- Tools ----

    /// Runs one of the agent's tool calls. Returns MCP content items.
    pub async fn call(&self, chat: &str, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        let keys = self.keys_holder().await;
        // Dropped when the call ends, which tells `keep_keys` the action is done.
        let (_running, done) = oneshot::channel::<()>();
        {
            let browsers = self.clone();
            let chat = chat.to_string();
            tauri::async_runtime::spawn(async move { browsers.keep_keys(&chat, keys, done).await });
        }
        self.run_tool(chat, tool, args).await
    }

    /// One tool call, as `call` runs it.
    async fn run_tool(&self, chat: &str, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        let text = |s: String| Ok(vec![json!({ "type": "text", "text": s })]);
        // A page put away while unused comes back for the agent's next look.
        if tool != "open" {
            self.bring_back(chat, true).await?;
        }
        self.touch(chat);
        match tool {
            "open" => {
                let url = local_url(args["url"].as_str().unwrap_or_default())?;
                let (url, title) = self.load(chat, url, true).await?;
                let errors = self.page_action(chat, "peekErrors", &json!({})).await.unwrap_or_default();
                let mut out = format!("Opened {url}");
                if !title.is_empty() {
                    out.push_str(&format!(" ({title})"));
                }
                if let Some(n) = errors.as_u64().filter(|n| *n > 0) {
                    out.push_str(&format!(". The page logged {n} error(s) or warning(s); see console."));
                }
                text(out)
            }
            "read" | "click" | "type" | "run" | "console" => {
                if self.webview(chat).is_none() {
                    return Err("No page is open; call open first.".into());
                }
                let value = self.page_action(chat, tool, args).await?;
                if tool == "click" {
                    self.settle(chat).await;
                }
                text(match value {
                    Value::String(s) => s,
                    other => other.to_string(),
                })
            }
            "resize" => text(self.resize(chat, args["width"].as_f64())?),
            "screenshot" => {
                let webview = self.webview(chat).ok_or("No page is open; call open first.")?;
                let png = screenshot(&webview).await?;
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD.encode(png);
                Ok(vec![json!({ "type": "image", "data": data, "mimeType": "image/png" })])
            }
            _ => Err(format!("Unknown browser tool: {tool}")),
        }
    }

    /// The agent's actions mustn't move the user's keys. Focusing a box in a
    /// page (to type in it or click it) makes WebKit give that page the
    /// window's keys, even when it is parked off screen for a chat not on
    /// show, so the user's typing in another chat would land in it. If the
    /// page took the keys, they go back to `before` (what held them before
    /// the action), or to the main window when that's gone.
    ///
    /// A page on show that held the keys before the action keeps them: the
    /// user clicked into it. A parked page never keeps them, even if it held
    /// them before: that can only be an earlier action's doing, not yet undone
    /// (two quick actions in a row).
    ///
    /// Checks while the action runs and for `KEYS_TAIL` after (`done` closes
    /// when it ends).
    async fn keep_keys(&self, chat: &str, before: usize, mut done: oneshot::Receiver<()>) {
        let mut ended: Option<Instant> = None;
        loop {
            match ended {
                Some(at) if at.elapsed() > KEYS_TAIL => return,
                Some(_) => {}
                None if matches!(done.try_recv(), Err(oneshot::error::TryRecvError::Empty)) => {}
                None => ended = Some(Instant::now()),
            }
            tokio::time::sleep(KEYS_POLL).await;
            let page = self.0.pages.lock().unwrap().get(chat).map(|p| p.view).unwrap_or(0);
            let shown = self.0.places.lock().unwrap().get(chat).is_some_and(|p| p.visible);
            if page == 0 || (page == before && shown) || self.keys_holder().await != page {
                continue;
            }
            let back = self
                .0
                .pages
                .lock()
                .unwrap()
                .values()
                .find(|p| p.view == before && p.view != page)
                .map(|p| p.webview.clone())
                .or_else(|| self.0.app.get_webview("main"));
            if let Some(webview) = back {
                let _ = webview.set_focus();
            }
        }
    }

    /// The view holding the window's keys, by address; 0 if unknown.
    async fn keys_holder(&self) -> usize {
        #[cfg(target_os = "macos")]
        {
            let (tx, rx) = oneshot::channel();
            let app = self.0.app.clone();
            let asked = self.0.app.run_on_main_thread(move || {
                let holder = app
                    .get_window("main")
                    .and_then(|w| w.ns_window().ok())
                    .map(mac::keys_holder)
                    .unwrap_or(0);
                let _ = tx.send(holder);
            });
            if asked.is_err() {
                return 0;
            }
            rx.await.unwrap_or(0)
        }
        #[cfg(not(target_os = "macos"))]
        0
    }

    /// After a click: if it started a page load, wait for it.
    async fn settle(&self, chat: &str) {
        tokio::time::sleep(CLICK_SETTLE).await;
        let rx = {
            let mut pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get_mut(chat) else { return };
            if !page.loading {
                return;
            }
            let (tx, rx) = oneshot::channel();
            page.load_waiters.push(tx);
            rx
        };
        let _ = tokio::time::timeout(LOAD_WAIT, rx).await;
    }

    /// Runs one of `BRIDGE_JS`'s actions in the chat's page and returns its result.
    async fn page_action(&self, chat: &str, op: &str, args: &Value) -> Result<Value, String> {
        let webview = self.webview(chat).ok_or("No page is open; call open first.")?;
        let id = {
            let mut next = self.0.next_action.lock().unwrap();
            *next += 1;
            *next
        };
        // Started in one call; a quick action answers at once, a slow one
        // (an async script) is collected by polling.
        let start = format!(
            "(window.__hb ? window.__hb.start({id}, {op}, {args}) : JSON.stringify({{done:true, ok:false, value:'The page is still loading or has no bridge yet; try again.'}}))",
            op = serde_json::to_string(op).unwrap(),
            args = args,
        );
        let deadline = tokio::time::Instant::now() + ACTION_WAIT;
        let mut reply = eval(&webview, start).await?;
        loop {
            if reply["done"] == true {
                return if reply["ok"] == true {
                    Ok(reply["value"].clone())
                } else {
                    Err(reply["value"].as_str().unwrap_or("The action failed in the page").to_string())
                };
            }
            if tokio::time::Instant::now() > deadline {
                return Err(format!("The page didn't answer in {} seconds.", ACTION_WAIT.as_secs()));
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
            let webview = self.webview(chat).ok_or("The browser was closed")?;
            reply = eval(
                &webview,
                format!("(window.__hb ? window.__hb.take({id}) : JSON.stringify({{done:true, ok:false, value:'The page changed before the action finished.'}}))"),
            )
            .await?;
        }
    }
}

fn put_away_after() -> Duration {
    std::env::var("HARNESS_BROWSER_IDLE_SECONDS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(PUT_AWAY_AFTER)
}

/// Places a page: over the panel body when visible, parked out of sight
/// otherwise, zoomed to lay out at the width the agent asked for.
fn apply_place(webview: &Webview, place: Place) {
    let Some(rect) = place.rect else { return };
    let x = if place.visible { rect.x } else { PARK_X };
    let _ = webview.set_size(LogicalSize::new(rect.width, rect.height));
    let _ = webview.set_position(LogicalPosition::new(x, rect.y));
    let _ = webview.set_zoom(place.zoom());
}

/// What `eval_with_callback` hands back: the script's value as JSON. The
/// bridge answers with a JSON string, so that string is parsed once more.
fn decode(raw: &str) -> Value {
    let value: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    match &value {
        Value::String(s) => serde_json::from_str(s).unwrap_or(value),
        _ => value,
    }
}

/// Runs a script in the page and returns its value.
async fn eval(webview: &Webview, script: String) -> Result<Value, String> {
    let (tx, rx) = oneshot::channel::<String>();
    let tx = Mutex::new(Some(tx));
    webview
        .eval_with_callback(script, move |raw| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(raw);
            }
        })
        .map_err(|e| e.to_string())?;
    let raw = tokio::time::timeout(ACTION_WAIT, rx)
        .await
        .map_err(|_| "The page didn't answer.".to_string())?
        .map_err(|_| "The page didn't answer.".to_string())?;
    Ok(decode(&raw))
}

/// A PNG of what the web view shows.
#[cfg(target_os = "macos")]
async fn screenshot(webview: &Webview) -> Result<Vec<u8>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<u8>, String>>();
    let scale = webview.window().scale_factor().unwrap_or(2.0);
    let size = webview.size().map(|s| s.to_logical::<f64>(scale)).ok();
    webview
        .with_webview(move |platform| mac::snapshot(platform.inner(), scale, size, tx))
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(ACTION_WAIT, rx)
        .await
        .map_err(|_| "The screenshot took too long.".to_string())?
        .map_err(|_| "The screenshot failed.".to_string())?
}

#[cfg(not(target_os = "macos"))]
async fn screenshot(_webview: &Webview) -> Result<Vec<u8>, String> {
    Err("Screenshots aren't supported on this system yet; use read and console instead.".into())
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2::MainThreadOnly;
    use objc2_foundation::{NSDictionary, NSError, NSNumber, NSString};
    use objc2_web_kit::{WKSnapshotConfiguration, WKUserScript, WKUserScriptInjectionTime, WKWebView};
    use tauri::LogicalSize;
    use tokio::sync::oneshot;

    use super::SHOT_MAX_EDGE;

    type Reply = oneshot::Sender<Result<Vec<u8>, String>>;

    /// Runs on the main thread. `view` is the page's `WKWebView`.
    pub fn snapshot(view: *mut c_void, scale: f64, size: Option<LogicalSize<f64>>, tx: Reply) {
        let tx = Mutex::new(Some(tx));
        let reply = move |result: Result<Vec<u8>, String>| {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(result);
            }
        };
        // SAFETY: wry hands over its live WKWebView; we only borrow it here,
        // on the main thread, as `with_webview` guarantees.
        let view: &WKWebView = unsafe { &*(view as *const WKWebView) };
        // SAFETY: `with_webview` runs this on the main thread.
        let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
        let config = unsafe { WKSnapshotConfiguration::new(mtm) };
        // Shrink so the long edge stays within what the agent takes.
        if let Some(size) = size {
            let long = size.width.max(size.height) * scale;
            if long > SHOT_MAX_EDGE {
                let width = size.width * SHOT_MAX_EDGE / long;
                unsafe { config.setSnapshotWidth(Some(&NSNumber::new_f64(width))) };
            }
        }
        let block = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            if image.is_null() {
                let why = if error.is_null() {
                    "no image".to_string()
                } else {
                    // SAFETY: a non-null error from WebKit, valid for this call.
                    unsafe { (*error).localizedDescription().to_string() }
                };
                reply(Err(format!("The screenshot failed: {why}")));
                return;
            }
            // SAFETY: a non-null image from WebKit, valid for this call.
            let image: &NSImage = unsafe { &*image };
            reply(png_of(image).ok_or_else(|| "Couldn't turn the screenshot into a PNG.".to_string()));
        });
        unsafe { view.takeSnapshotWithConfiguration_completionHandler(Some(&config), &block) };
    }

    /// The view holding `window`'s keys (its first responder), by address; 0 for none.
    pub fn keys_holder(window: *mut c_void) -> usize {
        if window.is_null() {
            return 0;
        }
        // SAFETY: Tauri's live NSWindow; called on the main thread.
        let window: &objc2::runtime::AnyObject = unsafe { &*(window as *const objc2::runtime::AnyObject) };
        let holder: *mut objc2::runtime::AnyObject = unsafe { objc2::msg_send![window, firstResponder] };
        holder as usize
    }

    /// Leaves `script` as the page's only start-up script, for every load from now on.
    pub fn only_bridge(view: *mut c_void, script: &str) {
        // SAFETY: as in `snapshot`: wry's live WKWebView, on the main thread.
        let view: &WKWebView = unsafe { &*(view as *const WKWebView) };
        let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
        unsafe {
            let scripts = view.configuration().userContentController();
            scripts.removeAllUserScripts();
            let bridge = WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                WKUserScript::alloc(mtm),
                &NSString::from_str(script),
                WKUserScriptInjectionTime::AtDocumentStart,
                true,
            );
            scripts.addUserScript(&bridge);
        }
    }

    fn png_of(image: &NSImage) -> Option<Vec<u8>> {
        let tiff = image.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let props = NSDictionary::new();
        let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &props) }?;
        Some(png.to_vec())
    }
}

/// Where the page keeps its errors and warnings, so they outlast a reload.
const LOG_KEY: &str = "__harness_browser_log";

/// Runs at the start of every page load in the browser. Keeps errors and
/// warnings, and answers the agent's actions. Each action answers with a JSON
/// string `{done, ok, value}`; an unfinished one is collected with `take`.
const BRIDGE_JS: &str = r#"
(() => {
  if (window.__hb || window.top !== window) return;
  const KEY = "__harness_browser_log";
  const MAX_LOG = 200;
  const MAX_TEXT = 20000;
  const show = (p) => {
    if (p instanceof Error) return String(p) + (p.stack ? "\n" + p.stack : "");
    if (typeof p === "string") return p;
    try { return JSON.stringify(p); } catch (e) { return String(p); }
  };
  const keep = (level, parts) => {
    try {
      const text = parts.map(show).join(" ").slice(0, 2000);
      const list = JSON.parse(sessionStorage.getItem(KEY) || "[]");
      list.push({ level, text, url: location.href, at: Date.now() });
      while (list.length > MAX_LOG) list.shift();
      sessionStorage.setItem(KEY, JSON.stringify(list));
    } catch (e) {}
  };
  for (const level of ["error", "warn"]) {
    const original = console[level];
    console[level] = function (...args) { keep(level, args); return original.apply(this, args); };
  }
  addEventListener("error", (e) => {
    const t = e.target;
    if (t && t !== window && (t.src || t.href)) keep("error", ["Failed to load " + (t.src || t.href)]);
    else keep("error", [e.error || (e.message + " at " + e.filename + ":" + e.lineno)]);
  }, true);
  addEventListener("unhandledrejection", (e) => keep("error", ["Unhandled rejection:", e.reason]));

  const INTERACTIVE = 'a[href],button,input,select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=option],[role=switch],[onclick],[contenteditable=""],[contenteditable=true],[tabindex]:not([tabindex="-1"])';
  let nextRef = 1;
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return false;
    const s = getComputedStyle(el);
    return s.visibility !== "hidden" && s.display !== "none";
  };
  const isField = (el) => el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT";
  // A field's label never comes from its value: that is shown on its own.
  const labelOf = (el) => (el.getAttribute("aria-label") || (el.labels && el.labels[0] && el.labels[0].innerText) ||
    (isField(el) ? "" : el.innerText || el.value) || el.placeholder ||
    el.getAttribute("title") || el.getAttribute("alt") || el.name || el.id || "").trim().replace(/\s+/g, " ").slice(0, 80);
  const refOf = (el) => {
    let r = el.getAttribute("data-harness-ref");
    if (!r) { r = String(nextRef++); el.setAttribute("data-harness-ref", r); }
    return r;
  };
  const describe = (el) => {
    let d = el.tagName.toLowerCase();
    if (el.type && el.tagName !== "BUTTON") d += "[type=" + el.type + "]";
    const label = labelOf(el);
    if (label) d += ' "' + label + '"';
    if (isField(el) && el.value) d += " value=" + JSON.stringify(String(el.value).slice(0, 60));
    if (el.disabled) d += " (disabled)";
    if (el.checked) d += " (checked)";
    return d;
  };
  const interactive = () => [...document.querySelectorAll(INTERACTIVE)].filter(visible);
  const find = (selector) => {
    let el;
    try { el = document.querySelector(selector); } catch (e) { throw new Error("Bad selector: " + selector); }
    if (!el) throw new Error("Nothing matches " + selector + ". Call read for the page's elements.");
    return el;
  };
  const byText = (text) => {
    const want = text.trim().toLowerCase();
    const els = interactive();
    return els.find((el) => labelOf(el).toLowerCase() === want) ||
      els.find((el) => labelOf(el).toLowerCase().includes(want)) ||
      [...document.querySelectorAll("body *")].filter(visible).reverse()
        .find((el) => (el.innerText || "").trim().toLowerCase() === want);
  };
  const press = (el) => {
    const opts = { bubbles: true, cancelable: true, view: window };
    el.dispatchEvent(new PointerEvent("pointerdown", opts));
    el.dispatchEvent(new MouseEvent("mousedown", opts));
    el.dispatchEvent(new PointerEvent("pointerup", opts));
    el.dispatchEvent(new MouseEvent("mouseup", opts));
    el.click();
  };
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;

  const ops = {
    peekErrors() {
      try { return JSON.parse(sessionStorage.getItem(KEY) || "[]").length; } catch (e) { return 0; }
    },
    read() {
      const text = (document.body ? document.body.innerText : "").trim();
      const cut = text.length > MAX_TEXT ? text.slice(0, MAX_TEXT) + "\n… (cut; " + text.length + " characters in all)" : text;
      const items = interactive().slice(0, 150)
        .map((el) => "[" + refOf(el) + "] " + describe(el));
      return "URL: " + location.href + "\nTitle: " + document.title + "\n\n" + (cut || "(no text)") +
        "\n\nElements (select with [data-harness-ref=\"n\"]):\n" + (items.join("\n") || "(none)");
    },
    click({ selector, text }) {
      let el;
      if (selector) el = find(selector);
      else if (text) { el = byText(text); if (!el) throw new Error('Nothing with the text "' + text + '". Call read for the page\'s elements.'); }
      else throw new Error("Give a selector or text.");
      el.scrollIntoView({ block: "center", inline: "center" });
      if (el.focus) el.focus();
      const what = describe(el);
      press(el);
      return "Clicked " + what;
    },
    type({ selector, text, submit }) {
      const el = find(selector);
      el.scrollIntoView({ block: "center" });
      el.focus();
      if (el.tagName === "SELECT") {
        const opt = [...el.options].find((o) => o.value === text || o.text.trim() === text);
        if (!opt) throw new Error("No option " + JSON.stringify(text) + "; options: " + [...el.options].map((o) => o.text.trim()).join(", "));
        el.value = opt.value;
        el.dispatchEvent(new Event("input", { bubbles: true }));
        el.dispatchEvent(new Event("change", { bubbles: true }));
      } else if (el.isContentEditable) {
        document.execCommand("selectAll", false);
        document.execCommand("insertText", false, text);
      } else {
        if (el.select) el.select();
        // insertText goes through the browser's own editing, so frameworks see
        // a real input; the value setter is the fallback.
        if (!document.execCommand("insertText", false, text) || el.value !== text) {
          const proto = el.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
          Object.getOwnPropertyDescriptor(proto, "value").set.call(el, text);
          el.dispatchEvent(new Event("input", { bubbles: true }));
        }
        el.dispatchEvent(new Event("change", { bubbles: true }));
      }
      let note = "Typed into " + describe(el);
      if (submit) {
        const key = { key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true, cancelable: true };
        const go = el.dispatchEvent(new KeyboardEvent("keydown", key));
        el.dispatchEvent(new KeyboardEvent("keyup", key));
        if (go && el.form && el.tagName !== "TEXTAREA") el.form.requestSubmit();
        note += " and pressed Enter";
      }
      return note;
    },
    console() {
      let list = [];
      try { list = JSON.parse(sessionStorage.getItem(KEY) || "[]"); sessionStorage.removeItem(KEY); } catch (e) {}
      if (!list.length) return "No errors or warnings.";
      return list.map((e) => {
        const t = new Date(e.at).toTimeString().slice(0, 8);
        return "[" + e.level + " " + t + "] " + e.text + (e.url !== location.href ? "  (on " + e.url + ")" : "");
      }).join("\n");
    },
    async run({ script }) {
      let fn;
      try { fn = new AsyncFunction("return (" + script + "\n)"); } catch (e) { fn = new AsyncFunction(script); }
      const value = await fn();
      if (value === undefined) return "undefined";
      let out;
      try { out = JSON.stringify(value, null, 1); } catch (e) { out = String(value); }
      if (out === undefined) out = String(value);
      return out.length > MAX_TEXT ? out.slice(0, MAX_TEXT) + "\n… (cut)" : out;
    },
  };

  const results = {};
  const fail = (e) => ({ done: true, ok: false, value: String((e && e.message) || e) });
  window.__hb = {
    start(id, op, args) {
      const f = ops[op];
      if (!f) return JSON.stringify(fail("Unknown action " + op));
      let r;
      try { r = f(args || {}); } catch (e) { return JSON.stringify(fail(e)); }
      if (r && typeof r.then === "function") {
        results[id] = { done: false };
        r.then((value) => { results[id] = { done: true, ok: true, value }; },
               (e) => { results[id] = fail(e); });
        return JSON.stringify({ done: false });
      }
      return JSON.stringify({ done: true, ok: true, value: r });
    },
    take(id) {
      const r = results[id];
      if (!r) return JSON.stringify(fail("The page lost track of this action (it may have reloaded)."));
      if (r.done) delete results[id];
      return JSON.stringify(r);
    },
  };
})();
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_localhost_addresses_are_allowed() {
        assert_eq!(local_url("localhost:5173").unwrap().as_str(), "http://localhost:5173/");
        assert_eq!(local_url(":3000/x").unwrap().as_str(), "http://localhost:3000/x");
        assert_eq!(local_url("http://127.0.0.1:8080").unwrap().as_str(), "http://127.0.0.1:8080/");
        assert!(local_url("https://localhost").is_ok());
        assert!(local_url("http://[::1]:4000").is_ok());
        assert!(local_url("example.com").is_err());
        assert!(local_url("http://localhost.evil.com").is_err());
        assert!(local_url("file:///etc/passwd").is_err());
        assert!(local_url("").is_err());
    }

    #[test]
    fn a_bridge_answer_is_unwrapped_once() {
        let raw = serde_json::to_string(&r#"{"done":true,"ok":true,"value":"hi"}"#).unwrap();
        assert_eq!(decode(&raw)["value"], "hi");
        assert_eq!(decode("\"plain title\""), json!("plain title"));
    }

    #[test]
    fn zoom_fits_the_asked_width_into_the_panel() {
        let rect = Some(Rect { x: 0.0, y: 0.0, width: 720.0, height: 900.0 });
        assert_eq!(Place { rect, visible: true, width: None }.zoom(), 1.0);
        assert_eq!(Place { rect, visible: true, width: Some(1440.0) }.zoom(), 0.5);
        assert_eq!(Place { rect, visible: true, width: Some(360.0) }.zoom(), 2.0);
        assert_eq!(Place { rect: None, visible: false, width: Some(1440.0) }.zoom(), 1.0);
    }

    #[test]
    fn every_tool_has_a_schema() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["name"].is_string());
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }
}
