//! The side panel's browser: localhost pages in tabs, several per chat, that
//! both the user and the agent use. The agent drives it through tools (see
//! `tools()` and `call()`); nothing here knows which agent that is.
//!
//! Each page is a web view the main window holds. The tab on show is placed
//! over the side panel's body; every other tab, and every tab while the
//! panel shows something else, is parked off to the side at the same size,
//! so what the agent reads and sees matches what the user would.
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

use crate::events::{AgentEvent, BrowserTab, Envelope};
use crate::sessions::Out;

/// Name the agent knows the browser's tools by.
pub const SERVER: &str = "tilde_browser";

/// A line for the agent's instructions.
pub const INSTRUCTION: &str = "This app has a browser in its side panel for localhost pages. To \
check a web UI you are building, use the tilde_browser tools (open, read, click, type, console, \
screenshot, resize, tabs, close); the user sees the same pages. It has tabs: open loads into the \
tab on show unless you ask for a new one, and every tool takes an optional tab id (see tabs). The \
panel is narrow, so pages show their small-screen layout; use resize to give a page a window \
size of your choosing. Pages keep running and every tool works whether or not the user is looking \
(another chat, another panel view, another app): never stop or wait for the browser to be shown.";

/// How long an action may take before it is given up.
const ACTION_WAIT: Duration = Duration::from_secs(15);
/// How long `open` waits for a page to load.
const LOAD_WAIT: Duration = Duration::from_secs(20);
/// How often to check that no page has taken the window's keys without the
/// user clicking into it (see `guard_keys`).
const KEYS_POLL: Duration = Duration::from_millis(50);
/// After a click: how long to watch for a page load it started.
const CLICK_SETTLE: Duration = Duration::from_millis(400);
/// Where a hidden page is parked: far left of the window, out of sight.
const PARK_X: f64 = -20_000.0;
/// How long a page may sit off screen and unused before it is put away
/// (`TILDE_BROWSER_IDLE_SECONDS` overrides it, for tests).
const PUT_AWAY_AFTER: Duration = Duration::from_secs(10 * 60);
/// How often put-away is checked.
const SWEEP_EVERY: Duration = Duration::from_secs(30);
/// Smallest and largest window side `resize` takes, in CSS pixels.
const MIN_SIDE: f64 = 240.0;
const MAX_SIDE: f64 = 3840.0;
/// Longest image edge sent to the agent, in pixels (larger ones are shrunk by it anyway).
const SHOT_MAX_EDGE: f64 = 1568.0;

/// The `tab` every tool but `tabs` takes.
fn tab_param() -> Value {
    json!({ "type": "string", "description": "Tab id from tabs; default: the tab on show" })
}

/// The tools, as MCP tool definitions.
pub fn tools() -> Value {
    let only_tab = json!({ "type": "object", "properties": { "tab": tab_param() } });
    json!([
        {
            "name": "open",
            "description": "Open a localhost page in the side panel's browser (e.g. http://localhost:5173) and wait for it to load. Only localhost, 127.0.0.1 and [::1] are allowed. Loads into the tab on show (or the tab given), replacing its page; set new_tab to keep that page and open another tab, which then is the one on show. Says which tab it used. Opening the same address again reloads it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Address, e.g. localhost:5173/settings" },
                    "new_tab": { "type": "boolean", "description": "Open in a new tab instead of replacing a page" },
                    "tab": tab_param(),
                },
                "required": ["url"],
            },
        },
        {
            "name": "tabs",
            "description": "List the browser's tabs: each one's id, title and address, and which is on show. Pass an id as `tab` to any other tool to work in that tab.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "close",
            "description": "Close a browser tab and its page.",
            "inputSchema": {
                "type": "object",
                "properties": { "tab": { "type": "string", "description": "Tab id from tabs" } },
                "required": ["tab"],
            },
        },
        {
            "name": "read",
            "description": "Read a page: its address, title, visible text, and a numbered list of the buttons, links and fields on it, each with a selector to pass to click or type.",
            "inputSchema": only_tab,
        },
        {
            "name": "click",
            "description": "Click an element, picked by CSS selector (such as one from read) or by its visible text.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "selector": { "type": "string" },
                    "text": { "type": "string", "description": "Visible text or label of the element, used when no selector is given" },
                    "tab": tab_param(),
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
                    "tab": tab_param(),
                },
                "required": ["selector", "text"],
            },
        },
        {
            "name": "console",
            "description": "Errors and warnings a page logged (console.error/warn, uncaught errors, failed loads of scripts and images) since the last open or console call. Kept across reloads.",
            "inputSchema": only_tab,
        },
        {
            "name": "run",
            "description": "Run JavaScript in a page and return its result as JSON. An expression is returned as is; a function body needs `return`. Can be async (await).",
            "inputSchema": {
                "type": "object",
                "properties": { "script": { "type": "string" }, "tab": tab_param() },
                "required": ["script"],
            },
        },
        {
            "name": "screenshot",
            "description": "A picture of a page at its full layout size (after resize: the size asked for, not the shrunk view in the panel), to check how the UI looks.",
            "inputSchema": only_tab,
        },
        {
            "name": "resize",
            "description": "Set a page's window size in CSS pixels, like resizing a browser window: the page lays out at exactly this size (window.innerWidth/innerHeight), and the side panel shows it shrunk to fit. Screenshots come out at this size. Stays with the tab until changed, across loads. Omit height to fill the panel's height; omit width (or 0) to go back to the panel's own size.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "width": { "type": "number", "description": "Window width in CSS pixels" },
                    "height": { "type": "number", "description": "Window height in CSS pixels" },
                    "tab": tab_param(),
                },
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

/// One tab's page, by chat and tab id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Key {
    chat: String,
    tab: String,
}

impl Key {
    fn new(chat: &str, tab: &str) -> Self {
        Self { chat: chat.to_string(), tab: tab.to_string() }
    }
}

/// A chat's tabs, in order, and the one on show. A tab may have no page yet
/// (a new tab before an address is typed) or have it put away.
#[derive(Default, Debug)]
struct Tabs {
    order: Vec<String>,
    shown: Option<String>,
    /// Tab ids are counted per chat: "1", "2", ...
    next: u64,
}

impl Tabs {
    /// Adds a tab at the end and shows it.
    fn add(&mut self) -> String {
        self.next += 1;
        let id = self.next.to_string();
        self.order.push(id.clone());
        self.shown = Some(id.clone());
        id
    }

    /// The tab on show: the one picked, else the newest.
    fn shown(&self) -> Option<&str> {
        self.shown
            .as_deref()
            .filter(|id| self.order.iter().any(|t| t == id))
            .or(self.order.last().map(String::as_str))
    }

    /// Takes a tab out. If it was on show, the one now in its place (or the
    /// last) shows instead. False if there was no such tab.
    fn remove(&mut self, id: &str) -> bool {
        let Some(at) = self.order.iter().position(|t| t == id) else {
            return false;
        };
        let was_shown = self.shown() == Some(id);
        self.order.remove(at);
        if was_shown {
            self.shown = self.order.get(at.min(self.order.len().saturating_sub(1))).cloned();
        }
        true
    }

    /// Moves a tab to `to` in the row (clamped to the end). False if there
    /// was no such tab.
    fn move_to(&mut self, id: &str, to: usize) -> bool {
        let Some(at) = self.order.iter().position(|t| t == id) else {
            return false;
        };
        let tab = self.order.remove(at);
        self.order.insert(to.min(self.order.len()), tab);
        true
    }

    /// The tab a tool works in: the one named, else the one on show.
    fn resolve(&self, asked: Option<&str>) -> Result<String, String> {
        match asked {
            Some(id) if self.order.iter().any(|t| t == id) => Ok(id.to_string()),
            Some(id) if self.order.is_empty() => Err(format!("There is no tab {id}: no page is open; call open first.")),
            Some(id) => Err(format!("There is no tab {id}. The tabs are: {}.", self.order.join(", "))),
            None => self.shown().map(str::to_string).ok_or_else(|| "No page is open; call open first.".to_string()),
        }
    }
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
    /// window's keys (see `guard_keys`). 0 where that isn't checked.
    view: usize,
}

/// Where a page goes: the panel body the window gave for its chat, whether
/// it is the one on show there, and the size the agent asked for it. Kept
/// apart from the page, so it holds before the page is made and while it is
/// put away.
#[derive(Clone, Copy, Default)]
struct Place {
    /// The panel body; None until the window has placed it.
    rect: Option<Rect>,
    visible: bool,
    /// Window size the agent asked for (`resize`); None fits the panel.
    size: Option<Size>,
}

/// A page window size the agent asked for, in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Size {
    width: f64,
    /// None: as tall as the panel holds at the zoom the width needs.
    height: Option<f64>,
}

/// Where a page's web view goes and how it is zoomed, in the window's logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    zoom: f64,
}

impl Frame {
    /// The page's layout size (`innerWidth` x `innerHeight`) in CSS pixels,
    /// rounded down as WebKit does.
    fn layout(&self) -> (f64, f64) {
        ((self.width / self.zoom).floor(), (self.height / self.zoom).floor())
    }
}

/// How many whole-point view widths below the largest that fits `frame` tries,
/// looking for one whose height also maps back to the asked height.
const FIT_TRIES: usize = 64;

impl Place {
    /// Where the page goes in the panel. With no size asked for it fills the
    /// panel. With one, the web view is that size times a zoom, and the page is
    /// zoomed by the same amount, so it lays out at the size asked for. The
    /// zoom shrinks it to fit the panel (never enlarges it); it sits at the
    /// top, centred across.
    ///
    /// The view's size is cut to whole points (wry and WebKit both drop the
    /// fraction), and WebKit rounds the layout size down. So the zoom is
    /// width / asked width for a whole-point width, which makes the layout
    /// width exact; of the widths that fit, the first whose whole-point height
    /// maps back to within half a pixel of the asked height wins. The zoom is
    /// then lowered a hair so float error can't round the layout down a pixel.
    fn frame(&self) -> Option<Frame> {
        let rect = self.rect?;
        let Some(size) = self.size else {
            return Some(Frame { x: rect.x, y: rect.y, width: rect.width, height: rect.height, zoom: 1.0 });
        };
        let mut fit = (rect.width / size.width).min(1.0);
        if let Some(height) = size.height {
            fit = fit.min(rect.height / height);
        }
        let largest = (size.width * fit).floor().max(1.0);
        let pick = |width: f64| {
            let zoom = width / size.width;
            let height = size.height.map_or(rect.height.floor(), |h| (h * zoom - 1e-9).ceil());
            let over = size.height.map_or(0.0, |h| height / zoom - h);
            (width, height, zoom, over)
        };
        let (width, height, zoom, _) = (0..FIT_TRIES)
            .map(|i| largest - i as f64)
            .take_while(|w| *w >= 1.0)
            .map(pick)
            .find(|c| c.3 < 0.5)
            .unwrap_or_else(|| pick(largest));
        let zoom = if zoom < 1.0 { zoom / (1.0 + 1e-5) } else { zoom };
        Some(Frame { x: rect.x + (rect.width - width) / 2.0, y: rect.y, width, height, zoom })
    }

    fn zoom(&self) -> f64 {
        self.frame().map_or(1.0, |f| f.zoom)
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
    tabs: Mutex<HashMap<String, Tabs>>,
    pages: Mutex<HashMap<Key, Page>>,
    /// By chat: where the window wants its pages (no size; see `place_of`).
    places: Mutex<HashMap<String, Place>>,
    /// Window sizes the agent asked for, by tab.
    sizes: Mutex<HashMap<Key, Size>>,
    away: Mutex<HashMap<Key, Away>>,
    /// Numbers web view labels, which can't hold every character a chat ID may.
    next: Mutex<u64>,
    /// Numbers actions run in pages.
    next_action: Mutex<u64>,
    /// One web view made at a time, so two calls can't both make one.
    make_lock: tokio::sync::Mutex<()>,
    /// The page the user last clicked into (its view's address), or 0 when
    /// the last click was anywhere else. Only that page may hold the keys.
    clicked: Mutex<usize>,
    /// What last held the window's keys rightfully: where they go back to.
    keys_home: Mutex<usize>,
}

#[derive(Clone)]
pub struct Browsers(Arc<Inner>);

impl Browsers {
    pub fn new(app: AppHandle, out: Out) -> Self {
        let browsers = Self(Arc::new(Inner {
            app,
            out,
            tabs: Mutex::default(),
            pages: Mutex::default(),
            places: Mutex::default(),
            sizes: Mutex::default(),
            away: Mutex::default(),
            next: Mutex::new(0),
            next_action: Mutex::new(0),
            make_lock: tokio::sync::Mutex::new(()),
            clicked: Mutex::new(0),
            keys_home: Mutex::new(0),
        }));
        browsers.watch_clicks();
        // Keys taken by a page the user didn't click into go back; ends with the app.
        let weak = Arc::downgrade(&browsers.0);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(KEYS_POLL).await;
                let Some(inner) = weak.upgrade() else { return };
                Browsers(inner).guard_keys().await;
            }
        });
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

    /// The tab on show in each chat whose panel shows its browser.
    fn on_screen(&self) -> Vec<Key> {
        let visible: Vec<String> = {
            let places = self.0.places.lock().unwrap();
            places.iter().filter(|(_, p)| p.visible).map(|(chat, _)| chat.clone()).collect()
        };
        let tabs = self.0.tabs.lock().unwrap();
        visible
            .into_iter()
            .filter_map(|chat| tabs.get(&chat).and_then(Tabs::shown).map(|tab| Key::new(&chat, tab)))
            .collect()
    }

    /// Closes pages off screen and unused for `after`, keeping their address:
    /// going back to one (the user, or the agent's next tool call in it)
    /// loads it again. The window isn't told; its tab and address stay.
    fn put_away_unused(&self, after: Duration) {
        let on_screen = self.on_screen();
        let stale: Vec<(Key, Page)> = {
            let mut pages = self.0.pages.lock().unwrap();
            let now = Instant::now();
            for key in &on_screen {
                if let Some(page) = pages.get_mut(key) {
                    page.used = now;
                }
            }
            let keys: Vec<Key> = pages
                .iter()
                .filter(|(_, p)| !p.loading && p.load_waiters.is_empty() && now.duration_since(p.used) >= after)
                .map(|(key, _)| key.clone())
                .collect();
            keys.into_iter().filter_map(|k| pages.remove(&k).map(|p| (k, p))).collect()
        };
        let mut away = self.0.away.lock().unwrap();
        for (key, page) in stale {
            let _ = page.webview.close();
            away.insert(
                key,
                Away {
                    url: page.url,
                    title: page.title,
                },
            );
        }
    }

    /// Brings a put-away page back, waiting for it to load when `wait` is set.
    /// False if the tab had nothing put away.
    async fn bring_back(&self, key: &Key, wait: bool) -> Result<bool, String> {
        let Some(away) = self.0.away.lock().unwrap().remove(key) else {
            return Ok(false);
        };
        let url = tauri::Url::parse(&away.url).map_err(|e| e.to_string())?;
        self.load(key, url, wait).await?;
        Ok(true)
    }

    /// The page is in use now.
    fn touch(&self, key: &Key) {
        if let Some(page) = self.0.pages.lock().unwrap().get_mut(key) {
            page.used = Instant::now();
        }
    }

    /// What the window shows of a chat's browser: every tab, and the one on show.
    fn tab_list(&self, chat: &str) -> (Vec<BrowserTab>, Option<String>) {
        let (order, shown) = {
            let tabs = self.0.tabs.lock().unwrap();
            match tabs.get(chat) {
                Some(t) => (t.order.clone(), t.shown().map(str::to_string)),
                None => return (Vec::new(), None),
            }
        };
        let pages = self.0.pages.lock().unwrap();
        let away = self.0.away.lock().unwrap();
        let list = order
            .into_iter()
            .map(|id| {
                let key = Key::new(chat, &id);
                let (url, title, loading) = match (pages.get(&key), away.get(&key)) {
                    (Some(p), _) => (p.url.clone(), p.title.clone(), p.loading),
                    (None, Some(a)) => (a.url.clone(), a.title.clone(), false),
                    (None, None) => (String::new(), String::new(), false),
                };
                BrowserTab { id, url, title, loading }
            })
            .collect();
        (list, shown)
    }

    /// Tells the window what the chat's browser now holds.
    fn tabs_event(&self, chat: &str) {
        let (tabs, shown) = self.tab_list(chat);
        (self.0.out)(Envelope {
            session_id: chat.to_string(),
            event: AgentEvent::BrowserTabs { tabs, shown },
        });
    }

    /// Where a tab's page goes: its chat's panel, if it is the tab on show,
    /// at the size the agent asked for it.
    fn place_of(&self, key: &Key) -> Place {
        let mut place = self.0.places.lock().unwrap().get(&key.chat).copied().unwrap_or_default();
        let shown = self.0.tabs.lock().unwrap().get(&key.chat).and_then(|t| t.shown().map(str::to_string));
        place.visible = place.visible && shown.as_deref() == Some(key.tab.as_str());
        place.size = self.0.sizes.lock().unwrap().get(key).copied();
        place
    }

    /// Puts each of a chat's pages where it goes: the one on show over the
    /// panel, the rest parked. A put-away page coming on screen is loaded again.
    fn arrange(&self, chat: &str) {
        let order = self.0.tabs.lock().unwrap().get(chat).map(|t| t.order.clone()).unwrap_or_default();
        for tab in order {
            let key = Key::new(chat, &tab);
            let place = self.place_of(&key);
            if let Some(webview) = self.webview(&key) {
                apply_place(&webview, place);
                if place.visible {
                    self.touch(&key);
                }
            } else if place.visible && self.0.away.lock().unwrap().contains_key(&key) {
                let browsers = self.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = browsers.bring_back(&key, false).await;
                });
            }
        }
    }

    /// Gives a tab's page a window size in CSS pixels (no width: the panel's
    /// own size), shown shrunk to fit the panel. With a page open, waits for
    /// it to take the new size and redraw, so a screenshot right after shows
    /// it, and says the size the page measures.
    async fn resize(&self, key: &Key, width: Option<f64>, height: Option<f64>) -> Result<String, String> {
        let side = |v: Option<f64>| v.filter(|v| *v > 0.0).map(|v| v.round().clamp(MIN_SIDE, MAX_SIDE));
        let size = side(width).map(|width| Size { width, height: side(height) });
        {
            let mut sizes = self.0.sizes.lock().unwrap();
            match size {
                Some(size) => sizes.insert(key.clone(), size),
                None => sizes.remove(key),
            };
        }
        let place = self.place_of(key);
        if let Some(webview) = self.webview(key) {
            apply_place(&webview, place);
        }
        let Some(frame) = place.frame() else {
            return Ok(match size {
                None => "The page fits the panel again.".into(),
                Some(Size { width, height: Some(height) }) => {
                    format!("The page will lay out at {width} x {height} once the panel shows.")
                }
                Some(Size { width, height: None }) => format!("The page will lay out {width} px wide once the panel shows."),
            });
        };
        let (mut w, mut h) = frame.layout();
        if self.webview(key).is_some() {
            let want = json!({ "width": w, "height": if size.is_some_and(|s| s.height.is_some()) { h } else { 0.0 } });
            if let Ok(Value::Array(got)) = self.page_action(key, "settle", &want).await {
                if let [Some(gw), Some(gh)] = [got.first().and_then(Value::as_f64), got.get(1).and_then(Value::as_f64)] {
                    (w, h) = (gw, gh);
                }
            }
        }
        Ok(match size {
            None => format!("The page fits the panel again ({w} x {h})."),
            Some(_) if frame.zoom >= 1.0 => format!("The page now lays out at {w} x {h}, shown at full size."),
            Some(_) => format!(
                "The page now lays out at {w} x {h}, shown at {:.0}% to fit the panel.",
                frame.zoom * 100.0
            ),
        })
    }

    fn webview(&self, key: &Key) -> Option<Webview> {
        self.0.pages.lock().unwrap().get(key).map(|p| p.webview.clone())
    }

    /// Adds an empty tab to a chat's browser and shows it.
    fn add_tab(&self, chat: &str) -> String {
        let id = self.0.tabs.lock().unwrap().entry(chat.to_string()).or_default().add();
        self.arrange(chat);
        id
    }

    /// The tab a tool works in (see `Tabs::resolve`).
    fn resolve(&self, chat: &str, asked: Option<&str>) -> Result<Key, String> {
        let tabs = self.0.tabs.lock().unwrap();
        let tab = match tabs.get(chat) {
            Some(t) => t.resolve(asked)?,
            None => Tabs::default().resolve(asked)?,
        };
        Ok(Key::new(chat, &tab))
    }

    // ---- Called by the window ----

    /// A new empty tab, shown; the user types its address next.
    pub fn new_tab(&self, chat: &str) -> String {
        let id = self.add_tab(chat);
        self.tabs_event(chat);
        id
    }

    /// The user typed an address: into the tab given, else the one on show,
    /// else a new one.
    pub async fn navigate(&self, chat: &str, tab: Option<&str>, input: &str) -> Result<(), String> {
        let url = local_url(input)?;
        let key = match tab {
            Some(tab) => self.resolve(chat, Some(tab))?,
            None => match self.resolve(chat, None) {
                Ok(key) => key,
                Err(_) => Key::new(chat, &self.add_tab(chat)),
            },
        };
        self.load(&key, url, false).await.map(|_| ())
    }

    pub async fn reload(&self, chat: &str, tab: &str) -> Result<(), String> {
        let key = self.resolve(chat, Some(tab))?;
        if self.bring_back(&key, false).await? {
            return Ok(());
        }
        let webview = self.webview(&key).ok_or("No page open")?;
        self.touch(&key);
        webview.reload().map_err(|e| e.to_string())
    }

    /// Find in the chat's shown browser tab: "search", "show" or "clear".
    /// Returns the match count for "search", 0 otherwise or when no page is open.
    pub async fn find(&self, chat: &str, op: &str, query: Option<&str>, index: Option<usize>) -> Result<usize, String> {
        let Ok(key) = self.resolve(chat, None) else {
            return Ok(0);
        };
        if self.webview(&key).is_none() {
            return Ok(0);
        }
        let (action, args) = match op {
            "search" => ("find_search", json!({ "query": query.unwrap_or("") })),
            "show" => ("find_show", json!({ "index": index.unwrap_or(0) })),
            "clear" => ("find_clear", json!({})),
            _ => return Err(format!("Unknown find action: {op}")),
        };
        let value = self.page_action(&key, action, &args).await?;
        Ok(value.as_u64().unwrap_or(0) as usize)
    }

    /// Whether one of the browser's pages currently holds the window's keys.
    /// Must be called on the main thread (as `on_menu_event` is on macOS).
    #[cfg(target_os = "macos")]
    pub fn has_focus(&self) -> bool {
        let holder = self
            .0
            .app
            .get_window("main")
            .and_then(|w| w.ns_window().ok())
            .map(mac::keys_holder)
            .unwrap_or(0);
        if holder == 0 {
            return false;
        }
        let hit = holder as *mut objc2::runtime::AnyObject;
        let views: Vec<usize> = self.0.pages.lock().unwrap().values().map(|p| p.view).collect();
        views.into_iter().any(|view| view != 0 && mac::is_inside(hit, view))
    }

    /// Gives the window's keys back to the main web view (e.g. when Cmd+F
    /// opens the browser find box while a page holds focus).
    #[cfg(target_os = "macos")]
    pub fn yield_focus(&self) {
        *self.0.clicked.lock().unwrap() = 0;
        if let Some(main) = self.0.app.get_webview("main") {
            let _ = main.set_focus();
        }
    }

    /// The user picked a tab.
    pub fn pick(&self, chat: &str, tab: &str) -> Result<(), String> {
        {
            let mut tabs = self.0.tabs.lock().unwrap();
            let tabs = tabs.get_mut(chat).ok_or("No browser tabs")?;
            if !tabs.order.iter().any(|t| t == tab) {
                return Err(format!("There is no tab {tab}"));
            }
            tabs.shown = Some(tab.to_string());
        }
        self.arrange(chat);
        self.tabs_event(chat);
        Ok(())
    }

    /// The user dragged a tab to another place in the row.
    pub fn move_tab(&self, chat: &str, tab: &str, to: usize) -> Result<(), String> {
        {
            let mut tabs = self.0.tabs.lock().unwrap();
            let tabs = tabs.get_mut(chat).ok_or("No browser tabs")?;
            if !tabs.move_to(tab, to) {
                return Err(format!("There is no tab {tab}"));
            }
        }
        self.tabs_event(chat);
        Ok(())
    }

    /// Closes one tab and its page; the tab beside it shows instead.
    pub fn close_tab(&self, chat: &str, tab: &str) -> Result<(), String> {
        let key = Key::new(chat, tab);
        {
            let mut tabs = self.0.tabs.lock().unwrap();
            let Some(list) = tabs.get_mut(chat) else {
                return Err(format!("There is no tab {tab}"));
            };
            if !list.remove(tab) {
                return Err(format!("There is no tab {tab}. The tabs are: {}.", list.order.join(", ")));
            }
            if list.order.is_empty() {
                tabs.remove(chat);
            }
        }
        let page = self.0.pages.lock().unwrap().remove(&key);
        self.0.away.lock().unwrap().remove(&key);
        self.0.sizes.lock().unwrap().remove(&key);
        if let Some(page) = page {
            let _ = page.webview.close();
        }
        self.arrange(chat);
        self.tabs_event(chat);
        Ok(())
    }

    /// Where the panel body is, and whether the browser shows there.
    pub fn place(&self, chat: &str, rect: Rect, visible: bool) {
        {
            let mut places = self.0.places.lock().unwrap();
            let place = places.entry(chat.to_string()).or_default();
            if rect.width > 0.0 && rect.height > 0.0 {
                place.rect = Some(rect);
            }
            place.visible = visible;
        }
        self.arrange(chat);
    }

    /// Closes all of a chat's tabs, when the chat goes.
    pub fn close(&self, chat: &str) {
        let had = self.0.tabs.lock().unwrap().remove(chat).is_some();
        self.0.places.lock().unwrap().remove(chat);
        self.0.sizes.lock().unwrap().retain(|k, _| k.chat != chat);
        self.0.away.lock().unwrap().retain(|k, _| k.chat != chat);
        let pages: Vec<Page> = {
            let mut pages = self.0.pages.lock().unwrap();
            let keys: Vec<Key> = pages.keys().filter(|k| k.chat == chat).cloned().collect();
            keys.iter().filter_map(|k| pages.remove(k)).collect()
        };
        for page in &pages {
            let _ = page.webview.close();
        }
        if had {
            self.tabs_event(chat);
        }
    }

    /// The chat's tabs, for a chat the window loads later.
    pub fn current(&self, chat: &str) -> (Vec<BrowserTab>, Option<String>) {
        self.tab_list(chat)
    }

    // ---- Loading ----

    /// Shows `url` in a tab, making its page first if needed, and waits for
    /// the load when `wait` is set. Returns the page's address and title.
    async fn load(&self, key: &Key, url: tauri::Url, wait: bool) -> Result<(String, String), String> {
        // A new address replaces whatever was put away.
        self.0.away.lock().unwrap().remove(key);
        let done = {
            let _guard = self.0.make_lock.lock().await;
            let (tx, rx) = oneshot::channel();
            let mut tx = Some(tx);
            let existing = {
                let mut pages = self.0.pages.lock().unwrap();
                match pages.get_mut(key) {
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
                None => self.make(key, url.clone(), tx.take().expect("unused")).await?,
            }
            rx
        };
        self.tabs_event(&key.chat);
        if wait {
            match tokio::time::timeout(LOAD_WAIT, done).await {
                Ok(_) => {}
                Err(_) => return Err(format!("{url} didn't finish loading in {} seconds.", LOAD_WAIT.as_secs())),
            }
        }
        let pages = self.0.pages.lock().unwrap();
        let page = pages.get(key).ok_or("The tab was closed")?;
        Ok((page.url.clone(), page.title.clone()))
    }

    /// Makes a tab's page showing `url`. `loaded` fires when the first load ends.
    async fn make(&self, key: &Key, url: tauri::Url, loaded: oneshot::Sender<()>) -> Result<(), String> {
        // Closed meanwhile: nothing to make.
        if !self.0.tabs.lock().unwrap().get(&key.chat).is_some_and(|t| t.order.contains(&key.tab)) {
            return Err("The tab was closed".into());
        }
        let window = self.0.app.get_window("main").ok_or("The main window is gone")?;
        let label = {
            let mut next = self.0.next.lock().unwrap();
            *next += 1;
            format!("browser-{}", *next)
        };
        let weak: Weak<Inner> = Arc::downgrade(&self.0);
        let page_key = key.clone();
        let app = self.0.app.clone();
        // Starts blank: the page's scripts are set up before the first real load.
        let blank = tauri::Url::parse("about:blank").expect("valid");
        // Not focused: a focused new web view brings the app to the front.
        // Never paused or slowed while unseen: the agent works pages the user
        // isn't looking at (another chat, another app, another Space).
        let builder = WebviewBuilder::new(&label, WebviewUrl::External(blank))
            .focused(false)
            .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
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
                Browsers(inner).page_loaded(&page_key, webview, payload.url().to_string(), payload.event());
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
        // app's dev page, Tilde's own included, think it runs inside Tilde.
        // On Mac they are taken out, leaving only the bridge; elsewhere they stay
        // for now (calls through them are refused: no capability covers this view).
        #[cfg(target_os = "macos")]
        let view = {
            let (tx, rx) = oneshot::channel::<usize>();
            let _ = webview.with_webview(move |platform| {
                mac::only_bridge(platform.inner(), BRIDGE_JS);
                mac::draw_when_unseen(platform.inner());
                let _ = tx.send(platform.inner() as usize);
            });
            rx.await.unwrap_or(0)
        };
        #[cfg(not(target_os = "macos"))]
        let view = 0;

        self.0.pages.lock().unwrap().insert(
            key.clone(),
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
        apply_place(&webview, self.place_of(key));
        webview.navigate(url).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn page_loaded(&self, key: &Key, webview: Webview, url: String, event: PageLoadEvent) {
        // The blank page a browser starts on isn't news.
        if url == "about:blank" {
            return;
        }
        {
            let mut pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get_mut(key) else { return };
            page.url = url;
            match event {
                PageLoadEvent::Started => page.loading = true,
                PageLoadEvent::Finished => page.loading = false,
            }
        }
        if let PageLoadEvent::Started = event {
            self.tabs_event(&key.chat);
            return;
        }
        // Read the title, then tell the window and anyone waiting.
        let browsers = self.clone();
        let read_key = key.clone();
        let asked = webview.eval_with_callback("document.title", move |title| {
            browsers.title_read(&read_key, &serde_json::from_str::<String>(&title).unwrap_or_default());
        });
        if asked.is_err() {
            self.title_read(key, "");
        }
    }

    fn title_read(&self, key: &Key, title: &str) {
        let waiters = {
            let mut pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get_mut(key) else { return };
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
        self.tabs_event(&key.chat);
    }

    // ---- Tools ----

    /// The `tabs` tool's answer.
    fn tabs_text(&self, chat: &str) -> String {
        let (list, shown) = self.tab_list(chat);
        if list.is_empty() {
            return "No tabs are open; call open to open a page.".into();
        }
        let on_screen = self.0.places.lock().unwrap().get(chat).is_some_and(|p| p.visible);
        tabs_lines(&list, shown.as_deref(), on_screen)
    }

    /// Runs one of the agent's tool calls. Returns MCP content items.
    pub async fn call(&self, chat: &str, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        let text = |s: String| Ok(vec![json!({ "type": "text", "text": s })]);
        let asked = args["tab"].as_str().filter(|t| !t.trim().is_empty()).map(str::trim);
        match tool {
            "tabs" => return text(self.tabs_text(chat)),
            "close" => {
                let tab = asked.ok_or("Say which tab to close (see tabs).")?;
                self.close_tab(chat, tab)?;
                return text(format!("Closed tab {tab}."));
            }
            "open" => {
                let url = local_url(args["url"].as_str().unwrap_or_default())?;
                let key = if args["new_tab"].as_bool() == Some(true) && asked.is_none() {
                    Key::new(chat, &self.add_tab(chat))
                } else {
                    match self.resolve(chat, asked) {
                        Ok(key) => key,
                        Err(e) if asked.is_some() => return Err(e),
                        Err(_) => Key::new(chat, &self.add_tab(chat)),
                    }
                };
                let (url, title) = self.load(&key, url, true).await?;
                let errors = self.page_action(&key, "peekErrors", &json!({})).await.unwrap_or_default();
                let mut out = format!("Opened {url} in tab {}", key.tab);
                if !title.is_empty() {
                    out.push_str(&format!(" ({title})"));
                }
                if let Some(n) = errors.as_u64().filter(|n| *n > 0) {
                    out.push_str(&format!(". The page logged {n} error(s) or warning(s); see console."));
                }
                return text(out);
            }
            _ => {}
        }
        let key = self.resolve(chat, asked)?;
        // A page put away while unused comes back for the agent's next look.
        self.bring_back(&key, true).await?;
        self.touch(&key);
        match tool {
            "read" | "click" | "type" | "run" | "console" => {
                if self.webview(&key).is_none() {
                    return Err(format!("Tab {} has no page yet; call open with tab {0}.", key.tab));
                }
                let value = self.page_action(&key, tool, args).await?;
                if tool == "click" {
                    self.settle(&key).await;
                }
                text(match value {
                    Value::String(s) => s,
                    other => other.to_string(),
                })
            }
            "resize" => text(self.resize(&key, args["width"].as_f64(), args["height"].as_f64()).await?),
            "screenshot" => {
                let webview = self
                    .webview(&key)
                    .ok_or_else(|| format!("Tab {} has no page yet; call open with tab {0}.", key.tab))?;
                let zoom = self.place_of(&key).zoom();
                let png = screenshot(&webview, zoom).await?;
                use base64::Engine;
                let data = base64::engine::general_purpose::STANDARD.encode(png);
                Ok(vec![json!({ "type": "image", "data": data, "mimeType": "image/png" })])
            }
            _ => Err(format!("Unknown browser tool: {tool}")),
        }
    }

    /// Pages mustn't take the user's keys. A page that focuses a box (the
    /// agent typing or clicking in it, or the page's own script, like a box
    /// that takes the cursor when a dev server reloads the page) makes WebKit
    /// give it the window's keys, even when it is parked off screen for a chat
    /// not on show, and the user's typing lands there. So a page may hold the
    /// keys only if the user's last click was in it; otherwise they go back to
    /// what last held them rightfully, or to the main window.
    async fn guard_keys(&self) {
        let pages: Vec<(usize, Webview)> = {
            let pages = self.0.pages.lock().unwrap();
            pages.values().filter(|p| p.view != 0).map(|p| (p.view, p.webview.clone())).collect()
        };
        if pages.is_empty() {
            return;
        }
        let holder = self.keys_holder().await;
        if holder == 0 {
            return;
        }
        let clicked = *self.0.clicked.lock().unwrap();
        if holder == clicked || !pages.iter().any(|(view, _)| *view == holder) {
            *self.0.keys_home.lock().unwrap() = holder;
            return;
        }
        let home = *self.0.keys_home.lock().unwrap();
        let back = pages
            .iter()
            .find(|(view, _)| *view == home && home == clicked)
            .map(|(_, webview)| webview.clone())
            .or_else(|| self.0.app.get_webview("main"));
        if let Some(webview) = back {
            let _ = webview.set_focus();
        }
    }

    /// Notes which page, if any, each mouse press in the window lands in.
    fn watch_clicks(&self) {
        #[cfg(target_os = "macos")]
        {
            let weak = Arc::downgrade(&self.0);
            let _ = self.0.app.run_on_main_thread(move || {
                mac::watch_clicks(move |hit| {
                    let Some(inner) = weak.upgrade() else { return };
                    let views: Vec<usize> = inner.pages.lock().unwrap().values().map(|p| p.view).collect();
                    let page = views.into_iter().find(|&view| view != 0 && mac::is_inside(hit, view)).unwrap_or(0);
                    *inner.clicked.lock().unwrap() = page;
                });
            });
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
    async fn settle(&self, key: &Key) {
        tokio::time::sleep(CLICK_SETTLE).await;
        let rx = {
            let mut pages = self.0.pages.lock().unwrap();
            let Some(page) = pages.get_mut(key) else { return };
            if !page.loading {
                return;
            }
            let (tx, rx) = oneshot::channel();
            page.load_waiters.push(tx);
            rx
        };
        let _ = tokio::time::timeout(LOAD_WAIT, rx).await;
    }

    /// Runs one of `BRIDGE_JS`'s actions in a tab's page and returns its result.
    async fn page_action(&self, key: &Key, op: &str, args: &Value) -> Result<Value, String> {
        let webview = self.webview(key).ok_or("No page is open; call open first.")?;
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
            let webview = self.webview(key).ok_or("The tab was closed")?;
            reply = eval(
                &webview,
                format!("(window.__hb ? window.__hb.take({id}) : JSON.stringify({{done:true, ok:false, value:'The page changed before the action finished.'}}))"),
            )
            .await?;
        }
    }
}

/// The `tabs` tool's lines: id, title, address, and which is on show.
fn tabs_lines(list: &[BrowserTab], shown: Option<&str>, on_screen: bool) -> String {
    let mut out = String::new();
    for tab in list {
        let title = if tab.title.is_empty() { "(no title)" } else { tab.title.as_str() };
        let url = if tab.url.is_empty() { "(empty)" } else { tab.url.as_str() };
        out.push_str(&format!("{}  {title}  {url}", tab.id));
        if shown == Some(tab.id.as_str()) {
            out.push_str(if on_screen { ", on screen" } else { ", on show (off screen for now; every tool still works)" });
        }
        out.push('\n');
    }
    out.trim_end().to_string()
}

fn put_away_after() -> Duration {
    crate::home::app_env_text("BROWSER_IDLE_SECONDS")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(PUT_AWAY_AFTER)
}

/// Places a page: over the panel body when visible, parked out of sight
/// otherwise, sized and zoomed to lay out at the size the agent asked for.
fn apply_place(webview: &Webview, place: Place) {
    let Some(frame) = place.frame() else { return };
    let x = if place.visible { frame.x } else { PARK_X };
    let _ = webview.set_size(LogicalSize::new(frame.width, frame.height));
    let _ = webview.set_position(LogicalPosition::new(x, frame.y));
    // On Mac the page is shrunk with WebKit's view scale, laid out at view
    // size / scale. Page zoom (the other way, used elsewhere) makes WebKit's
    // "smart minimum" text size kick in: text of 9 px or more is kept at
    // least 9 points on screen, so at 28% a 12.5 px font became 32 px and the
    // whole page ballooned.
    #[cfg(target_os = "macos")]
    {
        let scale = frame.zoom;
        let _ = webview.with_webview(move |platform| mac::set_view_scale(platform.inner(), scale));
    }
    #[cfg(not(target_os = "macos"))]
    let _ = webview.set_zoom(frame.zoom);
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

/// A PNG of what the web view shows, drawn at the page's layout size (the
/// view's size undone by `zoom`), not the shrunk size it shows at.
#[cfg(target_os = "macos")]
async fn screenshot(webview: &Webview, zoom: f64) -> Result<Vec<u8>, String> {
    let (tx, rx) = oneshot::channel::<Result<Vec<u8>, String>>();
    let scale = webview.window().scale_factor().unwrap_or(2.0);
    let size = webview
        .size()
        .map(|s| s.to_logical::<f64>(scale))
        .map(|s| LogicalSize::new(s.width / zoom, s.height / zoom))
        .ok();
    webview
        .with_webview(move |platform| mac::snapshot(platform.inner(), scale, size, tx))
        .map_err(|e| e.to_string())?;
    tokio::time::timeout(ACTION_WAIT, rx)
        .await
        .map_err(|_| "The screenshot took too long.".to_string())?
        .map_err(|_| "The screenshot failed.".to_string())?
}

#[cfg(not(target_os = "macos"))]
async fn screenshot(_webview: &Webview, _zoom: f64) -> Result<Vec<u8>, String> {
    Err("Screenshots aren't supported on this system yet; use read and console instead.".into())
}

#[cfg(target_os = "macos")]
pub fn send_edit_action(action: &str) {
    mac::send_edit_action(action);
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::sync::Mutex;

    use block2::RcBlock;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2::MainThreadOnly;
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSError, NSNumber, NSPoint, NSString};
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
        // Drawn at `size` (which can be larger than the view: WebKit paints the
        // page again at that size, so it stays sharp), shrunk so the long edge
        // stays within what the agent takes.
        if let Some(size) = size {
            let long = size.width.max(size.height) * scale;
            let width = size.width * (SHOT_MAX_EDGE / long).min(1.0);
            unsafe { config.setSnapshotWidth(Some(&NSNumber::new_f64(width))) };
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
        let window: &AnyObject = unsafe { &*(window as *const AnyObject) };
        let holder: *mut AnyObject = unsafe { msg_send![window, firstResponder] };
        holder as usize
    }

    /// Calls `on_click` with the view under every mouse press in the app, on
    /// the main thread, for the app's whole life. Run on the main thread.
    pub fn watch_clicks(on_click: impl Fn(*mut AnyObject) + 'static) {
        let block = RcBlock::new(move |event: NonNull<AnyObject>| -> *mut AnyObject {
            let event = event.as_ptr();
            // SAFETY: AppKit hands a live NSEvent, on the main thread.
            unsafe {
                let window: *mut AnyObject = msg_send![&*event, window];
                if !window.is_null() {
                    // Hit-tested from the content view's parent, as the window
                    // does: the browser's web views sit beside the main one.
                    let content: *mut AnyObject = msg_send![&*window, contentView];
                    let frame: *mut AnyObject = if content.is_null() { content } else { msg_send![&*content, superview] };
                    if !frame.is_null() {
                        let at: NSPoint = msg_send![&*event, locationInWindow];
                        let hit: *mut AnyObject = msg_send![&*frame, hitTest: at];
                        on_click(hit);
                    }
                }
            }
            event
        });
        // Left, right and other mouse buttons going down.
        let mask: u64 = (1 << 1) | (1 << 3) | (1 << 25);
        // SAFETY: a class method taking a mask and a block; on the main thread.
        let monitor: *mut AnyObject =
            unsafe { msg_send![objc2::class!(NSEvent), addLocalMonitorForEventsMatchingMask: mask, handler: &*block] };
        if !monitor.is_null() {
            // Kept for the app's life.
            let _: *mut AnyObject = unsafe { msg_send![monitor, retain] };
        }
    }

    /// Whether `hit` is the view at address `view` or inside it. On the main
    /// thread, with `view` a live page's web view.
    pub fn is_inside(hit: *mut AnyObject, view: usize) -> bool {
        if hit.is_null() || view == 0 {
            return false;
        }
        // SAFETY: both are live NSViews, on the main thread.
        unsafe { msg_send![&*hit, isDescendantOf: view as *mut AnyObject] }
    }

    /// Sends Cocoa's `undo:` or `redo:` action to the window's first responder.
    /// Must be called on the main thread.
    pub fn send_edit_action(action: &str) {
        let sel = match action {
            "undo" => objc2::sel!(undo:),
            "redo" => objc2::sel!(redo:),
            _ => return,
        };
        // SAFETY: NSApplication sharedApplication and sendAction:to:from: on main thread.
        unsafe {
            let app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
            if !app.is_null() {
                let nil = std::ptr::null_mut::<AnyObject>();
                let _: bool = msg_send![app, sendAction: sel, to: nil, from: nil];
            }
        }
    }

    /// Draws the page `scale` times its size, laid out at the view's size /
    /// `scale` (WebKit's `_setViewScale:` with layout mode "dynamic size
    /// computed from view scale"). Text sizes are left as the page set them.
    pub fn set_view_scale(view: *mut c_void, scale: f64) {
        // SAFETY: as in `snapshot`: wry's live WKWebView, on the main thread.
        let view: &WKWebView = unsafe { &*(view as *const WKWebView) };
        let known = |sel| -> bool { unsafe { msg_send![view, respondsToSelector: sel] } };
        if !known(objc2::sel!(_setViewScale:)) || !known(objc2::sel!(_setLayoutMode:)) {
            return;
        }
        // _WKLayoutModeViewSize = 0, _WKLayoutModeDynamicSizeComputedFromViewScale = 2.
        let mode: usize = if scale < 1.0 { 2 } else { 0 };
        // SAFETY: both checked above; they take an NSUInteger and a CGFloat.
        unsafe {
            let _: () = msg_send![view, _setLayoutMode: mode];
            let _: () = msg_send![view, _setViewScale: scale];
        }
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

    /// Keeps the page drawing (animation frames, `visibilityState` "visible")
    /// while the window is covered or on another Space. WebKit otherwise
    /// counts a covered window as hidden and stops drawing, so screenshots go
    /// stale. Private WebKit setting; skipped if this WebKit lacks it.
    pub fn draw_when_unseen(view: *mut c_void) {
        // SAFETY: as in `snapshot`: wry's live WKWebView, on the main thread.
        let view: &AnyObject = unsafe { &*(view as *const AnyObject) };
        let setter = objc2::sel!(_setWindowOcclusionDetectionEnabled:);
        let has: bool = unsafe { msg_send![view, respondsToSelector: setter] };
        if has {
            let _: () = unsafe { msg_send![view, _setWindowOcclusionDetectionEnabled: false] };
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
const LOG_KEY: &str = "__tilde_browser_log";

/// Runs at the start of every page load in the browser. Keeps errors and
/// warnings, and answers the agent's actions. Each action answers with a JSON
/// string `{done, ok, value}`; an unfinished one is collected with `take`.
const BRIDGE_JS: &str = r#"
(() => {
  if (window.__hb || window.top !== window) return;
  const KEY = "__tilde_browser_log";
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

  // Cmd+Y on Mac (and Ctrl+Shift+Z elsewhere) redoes in any text box on the page.
  const IS_MAC = /Mac/.test(navigator.userAgent);
  addEventListener("keydown", (e) => {
    if (e.isComposing || e.altKey) return;
    const el = document.activeElement;
    if (!el || !(el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
    const k = e.key.toLowerCase();
    const redo = IS_MAC
      ? e.metaKey && !e.ctrlKey && !e.shiftKey && k === "y"
      : e.ctrlKey && !e.metaKey && e.shiftKey && k === "z";
    if (!redo) return;
    e.preventDefault();
    document.execCommand("redo");
  }, true);

  let findRanges = [];
  const hasHighlights = () => typeof CSS !== "undefined" && "highlights" in CSS;
  const ensureFindStyle = () => {
    if (document.getElementById("__hb_find_style")) return;
    const s = document.createElement("style");
    s.id = "__hb_find_style";
    s.textContent = "::highlight(hb-find){background:rgba(255,210,70,0.45);color:inherit}::highlight(hb-find-current){background:#f5b800;color:#111}";
    (document.head || document.documentElement).appendChild(s);
  };
  const findVisible = (node) => {
    const el = node.parentElement;
    if (!el || el.closest("script,style,noscript,template")) return false;
    const fold = el.closest("details:not([open])");
    if (fold && !el.closest("summary")) return false;
    if (el.checkVisibility) return el.checkVisibility();
    const st = getComputedStyle(el);
    return st.display !== "none" && st.visibility !== "hidden";
  };

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
    let r = el.getAttribute("data-tilde-ref");
    if (!r) { r = String(nextRef++); el.setAttribute("data-tilde-ref", r); }
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
        "\n\nElements (select with [data-tilde-ref=\"n\"]):\n" + (items.join("\n") || "(none)");
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
    // After a resize: waits (up to a second) for the new size to reach the
    // page, then two frames so its resize handlers have run and painted.
    // A page off screen may get no frames, so each is capped.
    async settle({ width, height }) {
      const frame = () => new Promise((r) => { requestAnimationFrame(() => r()); setTimeout(r, 100); });
      for (let i = 0; i < 20 && (innerWidth !== width || (height && innerHeight !== height)); i++) {
        await new Promise((r) => setTimeout(r, 50));
      }
      await frame();
      await frame();
      return [innerWidth, innerHeight];
    },
    find_search({ query }) {
      findRanges = [];
      const q = String(query || "").toLowerCase();
      if (q && document.body) {
        const nodes = [];
        const starts = [];
        let text = "";
        const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          const val = n.nodeValue || "";
          if (!val || !findVisible(n)) continue;
          nodes.push(n);
          starts.push(text.length);
          text += val;
        }
        const lower = text.toLowerCase();
        const hay = lower.length === text.length ? lower : text;
        const needle = hay === lower ? q : String(query || "");
        const at = (pos) => {
          let lo = 0, hi = starts.length - 1;
          while (lo < hi) {
            const mid = (lo + hi + 1) >> 1;
            if (starts[mid] <= pos) lo = mid;
            else hi = mid - 1;
          }
          return [nodes[lo], pos - starts[lo]];
        };
        for (let i = hay.indexOf(needle); i >= 0; i = hay.indexOf(needle, i + needle.length)) {
          const r = document.createRange();
          const [sn, so] = at(i);
          const [en, eo] = at(i + needle.length - 1);
          r.setStart(sn, so);
          r.setEnd(en, eo + 1);
          findRanges.push(r);
        }
      }
      if (hasHighlights()) {
        ensureFindStyle();
        CSS.highlights.set("hb-find", new Highlight(...findRanges));
        CSS.highlights.delete("hb-find-current");
      }
      return findRanges.length;
    },
    find_show({ index }) {
      const r = findRanges[index];
      if (!r) return 0;
      if (hasHighlights()) {
        ensureFindStyle();
        CSS.highlights.set("hb-find-current", new Highlight(r));
      } else {
        const sel = getSelection();
        if (sel) { sel.removeAllRanges(); sel.addRange(r); }
      }
      const el = r.startContainer.parentElement;
      if (el && el.scrollIntoView) el.scrollIntoView({ block: "center", inline: "nearest" });
      return 0;
    },
    find_clear() {
      findRanges = [];
      if (hasHighlights()) {
        CSS.highlights.delete("hb-find");
        CSS.highlights.delete("hb-find-current");
      }
      return 0;
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
    fn the_asked_size_is_shrunk_to_fit_at_the_top() {
        let rect = Some(Rect { x: 100.0, y: 50.0, width: 720.0, height: 900.0 });
        let at = |width, height| Place { rect, visible: true, size: Some(Size { width, height }) }.frame().unwrap();
        // Nothing asked: fills the panel.
        let fill = Place { rect, visible: true, size: None }.frame().unwrap();
        assert_eq!(fill, Frame { x: 100.0, y: 50.0, width: 720.0, height: 900.0, zoom: 1.0 });
        // Desktop: width-bound, half size, top of the panel.
        let desk = at(1440.0, Some(900.0));
        assert_eq!((desk.x, desk.y, desk.width, desk.height), (100.0, 50.0, 720.0, 450.0));
        assert_eq!(desk.layout(), (1440.0, 900.0));
        // Tall: height-bound, centred across.
        let tall = at(720.0, Some(1800.0));
        assert_eq!((tall.x, tall.y, tall.width, tall.height), (280.0, 50.0, 360.0, 900.0));
        assert_eq!(tall.layout(), (720.0, 1800.0));
        // Small: shown at full size, never enlarged.
        let phone = at(390.0, Some(844.0));
        assert_eq!(phone.zoom, 1.0);
        assert_eq!(phone.layout(), (390.0, 844.0));
        // Width only: as tall as the panel holds.
        assert_eq!(at(1440.0, None).layout(), (1440.0, 1800.0));
        // No panel yet: nowhere to put it.
        assert!(Place { rect: None, visible: false, size: Some(Size { width: 1440.0, height: None }) }.frame().is_none());
    }

    #[test]
    fn an_odd_panel_still_gives_the_exact_size() {
        // A real panel: fractional width (seen live as 1439 x 900 before).
        let rect = Some(Rect { x: 878.90625, y: 39.0, width: 521.09375, height: 861.0 });
        for (width, height) in [(1440.0, 900.0), (1366.0, 768.0), (1920.0, 1080.0), (1280.0, 800.0), (2560.0, 1440.0)] {
            let frame = Place { rect, visible: true, size: Some(Size { width, height: Some(height) }) }.frame().unwrap();
            // Whole points (the view drops fractions), inside the panel.
            assert_eq!(frame.width, frame.width.floor());
            assert_eq!(frame.height, frame.height.floor());
            assert!(frame.width <= 521.09375 && frame.height <= 861.0);
            // What the page measures, as WebKit works it out in single precision.
            let zoom = frame.zoom as f32;
            let inner = ((frame.width as f32 / zoom).floor(), (frame.height as f32 / zoom).floor());
            assert_eq!(inner, (width as f32, height as f32), "{width} x {height}");
        }
    }

    #[test]
    fn every_tool_has_a_schema() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["name"].is_string());
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn tabs_count_up_and_a_new_one_shows() {
        let mut tabs = Tabs::default();
        assert_eq!(tabs.add(), "1");
        assert_eq!(tabs.add(), "2");
        assert_eq!(tabs.shown(), Some("2"));
        tabs.shown = Some("1".into());
        assert_eq!(tabs.shown(), Some("1"));
        // Ids aren't reused after a close.
        tabs.remove("2");
        assert_eq!(tabs.add(), "3");
    }

    #[test]
    fn a_dragged_tab_moves_and_keeps_its_id() {
        let mut tabs = Tabs::default();
        for _ in 0..3 {
            tabs.add();
        }
        assert!(tabs.move_to("3", 0));
        assert_eq!(tabs.order, ["3", "1", "2"]);
        assert!(tabs.move_to("3", 9));
        assert_eq!(tabs.order, ["1", "2", "3"]);
        assert_eq!(tabs.shown(), Some("3"));
        assert!(!tabs.move_to("7", 0));
    }

    #[test]
    fn closing_the_tab_on_show_shows_the_one_beside_it() {
        let mut tabs = Tabs::default();
        for _ in 0..3 {
            tabs.add();
        }
        tabs.shown = Some("2".into());
        assert!(tabs.remove("2"));
        assert_eq!(tabs.shown(), Some("3"));
        assert!(tabs.remove("3"));
        assert_eq!(tabs.shown(), Some("1"));
        // Closing one not on show leaves the pick alone.
        tabs.add();
        tabs.shown = Some("1".into());
        assert!(tabs.remove("4"));
        assert_eq!(tabs.shown(), Some("1"));
        assert!(!tabs.remove("9"));
        assert!(tabs.remove("1"));
        assert_eq!(tabs.shown(), None);
    }

    #[test]
    fn a_tool_works_in_the_tab_named_or_the_one_on_show() {
        let mut tabs = Tabs::default();
        assert_eq!(tabs.resolve(None).unwrap_err(), "No page is open; call open first.");
        assert!(tabs.resolve(Some("1")).unwrap_err().contains("no page is open"));
        tabs.add();
        tabs.add();
        tabs.shown = Some("1".into());
        assert_eq!(tabs.resolve(None).unwrap(), "1");
        assert_eq!(tabs.resolve(Some("2")).unwrap(), "2");
        assert_eq!(tabs.resolve(Some("7")).unwrap_err(), "There is no tab 7. The tabs are: 1, 2.");
    }

    #[test]
    fn the_tab_list_marks_the_one_on_show() {
        let tab = |id: &str, url: &str, title: &str| BrowserTab {
            id: id.into(),
            url: url.into(),
            title: title.into(),
            loading: false,
        };
        let list = [tab("1", "http://localhost:5173/", "My app"), tab("2", "", "")];
        assert_eq!(
            tabs_lines(&list, Some("1"), true),
            "1  My app  http://localhost:5173/, on screen\n2  (no title)  (empty)"
        );
        assert!(tabs_lines(&list, Some("2"), false).ends_with("(empty), on show (off screen for now; every tool still works)"));
    }
}
