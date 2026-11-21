//! The side panel's terminal: real shells in pseudo-terminals. Tabs belong to a
//! project, not a chat: every chat of the project shows the same ones, so a
//! scratch server left running stays in view.
//!
//! The core owns the shells and every timer (a hidden window's own timers run
//! late). The window draws each tab with xterm.js and gets its output through a
//! channel of its own. The core keeps the last stretch of each tab's raw output,
//! for the agent's `read` tool and for bringing tabs back after a quit.
//!
//! Output only reads right at the width it was drawn at: a progress display
//! that steps back up over its own lines lands on the wrong ones when those
//! lines wrap. So the kept output carries a size mark (see `size_mark`) each
//! time the shell's size changes, and the window draws each stretch at its
//! own size before fitting the tab to the panel.
//!
//! Rules:
//! - The user closes tabs. As a safety net, a tab unused for a day (no typing,
//!   not on screen, not read by the agent) is closed; it keeps its output and a
//!   line saying why, and typing starts a fresh shell under it. The clock is
//!   `Instant`, which doesn't advance while the Mac sleeps.
//! - A shell that exits by itself (`exit`, Ctrl+D) takes its tab with it.
//! - Quitting saves every tab's output and ends every shell. On the next start
//!   the tabs are back, closed, with a "restored" line.
//! - A tab opened in a chat's worktree keeps that worktree from being put away
//!   while its shell runs, and is closed with a line when the worktree goes.
//! - A tab an agent opened goes with its chat when the chat is deleted, unless
//!   something still runs in it or the user has typed in it (then it is theirs).

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::events::{AgentEvent, Envelope};
use crate::sessions::Out;
use crate::store::{SavedTerminal, Store, now_ms};

/// Name the agent knows the terminal's tools by.
pub const SERVER: &str = "tilde_terminal";

/// The lines for the agent's instructions. `tab_command` is the full path of
/// `tilde-tab` (see `Home::write_tab_command`).
pub fn instruction(tab_command: &Path) -> String {
    format!(
        "The user has a terminal in this app's side panel for this project, with one or more tabs. \
When they mention their terminal or what it shows, read it with the tilde_terminal tools (tabs, read). \
To run a command in a new tab where the user can watch it and type into it (a dev server, a sign-in \
prompt, or when they ask you to run something in the terminal), run `{} \"<command>\"` with your shell \
tool. It opens the tab in this chat's folder, shows it, and returns at once with the tab's name; read the \
tab for its output. You can't type into tabs the user opened.",
        tab_command.display()
    )
}

/// How long an agent's tab waits for its shell's first output before the
/// command is typed in anyway.
const PROMPT_WAIT: Duration = Duration::from_secs(3);
/// How long a tab the agent opens waits for the panel to give its size
/// before its shell starts anyway (the panel may be hidden or covered).
const SIZE_WAIT: Duration = Duration::from_secs(1);

/// Raw output kept per tab, for the agent and for saving.
const TAIL_MAX: usize = 512 * 1024;
/// Raw output saved per tab at quit.
const SAVE_MAX: usize = 256 * 1024;
/// How long a tab may sit unused before its shell is closed
/// (`TILDE_TERMINAL_IDLE_SECONDS` overrides it, for tests).
const IDLE: Duration = Duration::from_secs(24 * 60 * 60);
/// How often running commands are checked.
const TICK: Duration = Duration::from_secs(1);
/// Unused tabs are checked every this many ticks.
const SWEEP_TICKS: u64 = 30;
/// Output is saved (if any came) every this many ticks, so a crash loses little.
const SAVE_TICKS: u64 = 300;
/// How long a closing shell gets to end before it is killed.
const KILL_GRACE: Duration = Duration::from_secs(2);
/// How long quitting waits for shells to end before killing them.
const QUIT_GRACE: Duration = Duration::from_secs(1);
/// Lines `read` returns by default, and at most.
/// How a paste into the terminal starts (bracketed paste).
const PASTE_START: &str = "\x1b[200~";
const READ_LINES: usize = 200;
const MAX_READ_LINES: usize = 2000;
/// Size of a shell started before the window has measured its tab.
const DEFAULT_SIZE: PtySize = PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 };

/// Setting that keeps the panel's last size ("cols x rows") across a quit, so
/// a tab the agent opens before the panel is shown starts at the right width.
const PANEL_SIZE_KEY: &str = "terminal_panel_size";

/// A tab as the window shows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabInfo {
    pub id: String,
    /// What runs in it ("python3"), or its folder's name when nothing does.
    pub title: String,
    /// The command line running in it, if anything runs besides the shell.
    pub running: Option<String>,
    /// Its shell has ended; typing starts a new one.
    pub closed: bool,
    /// A command finished in it since it was last on screen.
    pub done: bool,
    /// The worktree folder it was opened in, if any.
    pub worktree: Option<String>,
}

/// A tab's running shell.
struct Live {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    /// The shell's process ID, which is also its process group.
    pid: i32,
}

struct Tab {
    /// The project's main folder, as the window names it.
    project: String,
    /// Where its shell starts.
    folder: PathBuf,
    /// The chat worktree folder it was opened in, if any.
    worktree: Option<PathBuf>,
    /// Its name when nothing runs in it.
    label: String,
    /// The chat whose agent opened it, until the user types in it.
    chat: Option<String>,
    /// The command line running in it besides the shell, and that job's group.
    running: Option<String>,
    fg: Option<i32>,
    /// A command finished in it while it was off screen, and it hasn't been
    /// on screen since.
    done: bool,
    /// The last of its raw output.
    tail: Vec<u8>,
    /// A full-screen program (vim, less) has the screen.
    alt_screen: bool,
    used: Instant,
    live: Option<Live>,
    /// Counts shells started in this tab, so an old shell's reader stops.
    run: u64,
    /// The window's sink for its output.
    channel: Option<Channel<InvokeResponseBody>>,
    size: PtySize,
    /// The size the end of `tail` was drawn at: its last size mark.
    drawn: Option<PtySize>,
    /// The running shell has printed something (its prompt, at first).
    spoke: bool,
    /// The window has given this tab its size.
    sized: bool,
    /// Output came since the last save.
    dirty: bool,
    /// Counts tabs as they open, so two with one name keep their " 2" when
    /// dragged past each other.
    born: u64,
}

impl Tab {
    fn new(project: &str, folder: PathBuf, worktree: Option<PathBuf>, label: String) -> Self {
        Tab {
            project: project.to_string(),
            folder,
            worktree,
            label,
            chat: None,
            running: None,
            fg: None,
            done: false,
            tail: Vec::new(),
            alt_screen: false,
            used: Instant::now(),
            live: None,
            run: 0,
            channel: None,
            size: DEFAULT_SIZE,
            drawn: None,
            spoke: false,
            sized: false,
            dirty: false,
            born: 0,
        }
    }

    /// New output: kept, and passed to the window.
    fn push(&mut self, bytes: &[u8]) {
        self.alt_screen = alt_screen_after(self.alt_screen, bytes);
        self.tail.extend_from_slice(bytes);
        if self.tail.len() > TAIL_MAX {
            let cut = trim_point(&self.tail, self.tail.len() - TAIL_MAX);
            // What is left keeps the size it was drawn at.
            let mark = last_size_mark(&self.tail[..cut]).map(|m| m.to_vec());
            self.tail.drain(..cut);
            if let Some(mark) = mark {
                self.tail.splice(0..0, mark);
            }
        }
        self.dirty = true;
        if let Some(channel) = &self.channel {
            if channel.send(InvokeResponseBody::Raw(bytes.to_vec())).is_err() {
                self.channel = None;
            }
        }
    }

    /// The shell draws at `size` from here on: noted in the kept output, for
    /// drawing it again later. Not sent to the window, which knows its size.
    fn mark(&mut self, size: PtySize) {
        if self.drawn == Some(size) {
            return;
        }
        self.drawn = Some(size);
        self.tail.extend_from_slice(&size_mark(size));
        self.dirty = true;
    }

    /// Writes a line of the app's own under the output.
    fn line(&mut self, text: &str) {
        let bytes = line_bytes(text, self.alt_screen);
        self.push(&bytes);
    }

    fn base_title(&self) -> String {
        self.running.as_deref().map(command_name).unwrap_or_else(|| self.label.clone())
    }
}

#[derive(Default)]
struct State {
    tabs: HashMap<String, Tab>,
    /// Every tab ID, oldest first; a project's tabs are its entries in order.
    order: Vec<String>,
    /// The tab on screen, if any.
    shown: Option<String>,
    /// The last `Tab::born` given.
    born: u64,
    /// The size the window last gave a tab: the panel's, for tabs the agent
    /// opens. A shell started wider than the panel draws its prompt wrong.
    panel_size: Option<PtySize>,
}

impl State {
    fn add(&mut self, id: String, mut tab: Tab) {
        self.born += 1;
        tab.born = self.born;
        self.order.push(id.clone());
        self.tabs.insert(id, tab);
    }
}

struct Inner {
    out: Out,
    store: Arc<Store>,
    state: Mutex<State>,
    idle: Duration,
    ticks: Mutex<u64>,
}

#[derive(Clone)]
pub struct Terminals(Arc<Inner>);

/// A lock that a panic elsewhere never poisons for good: the app must not
/// stop over a terminal.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Terminals {
    /// Brings back the tabs saved at the last quit, closed, and starts the
    /// checks that run every second.
    pub fn new(out: Out, store: Arc<Store>) -> Self {
        let idle = crate::home::app_env_text("TERMINAL_IDLE_SECONDS")
            .and_then(|s| s.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(IDLE);
        let terminals = Self::with(out, store, idle);
        terminals.restore();
        let weak = Arc::downgrade(&terminals.0);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(TICK).await;
                let Some(inner) = weak.upgrade() else { return };
                let terminals = Terminals(inner);
                let _ = tauri::async_runtime::spawn_blocking(move || terminals.tick()).await;
            }
        });
        terminals
    }

    fn with(out: Out, store: Arc<Store>, idle: Duration) -> Self {
        Terminals(Arc::new(Inner {
            out,
            store,
            state: Mutex::new(State::default()),
            idle,
            ticks: Mutex::new(0),
        }))
    }

    fn restore(&self) {
        let saved = self.0.store.load_terminals().unwrap_or_default();
        let mut state = lock(&self.0.state);
        state.panel_size = self.0.store.setting(PANEL_SIZE_KEY).and_then(|s| {
            let (cols, rows) = s.split_once('x')?;
            Some(size(cols.trim().parse().ok()?, rows.trim().parse().ok()?))
        });
        for row in saved {
            let project = PathBuf::from(&row.project);
            let worktree = row.worktree.map(PathBuf::from).filter(|w| w.is_dir());
            let folder = PathBuf::from(&row.folder);
            let folder = if folder.is_dir() { folder } else { project };
            let mut tab = Tab::new(&row.project, folder, worktree, row.label);
            tab.drawn = last_size_mark(&row.output).and_then(mark_size);
            tab.alt_screen = alt_screen_after(false, &row.output);
            tab.tail = row.output;
            tab.chat = row.chat;
            tab.line(&format!("restored {}", local_time(row.saved_at)));
            tab.dirty = false;
            state.add(row.id, tab);
        }
    }

    /// A project's tabs, in order.
    pub fn tabs(&self, project: &str) -> Vec<TabInfo> {
        infos(&lock(&self.0.state), project)
    }

    fn emit(&self, project: &str) {
        let tabs = self.tabs(project);
        (self.0.out)(Envelope {
            session_id: String::new(),
            event: AgentEvent::TerminalTabs { project: project.to_string(), tabs },
        });
    }

    /// Opens a tab with a fresh shell: in the project's main folder, or in
    /// `worktree` (a chat's worktree folder), named `label` when idle.
    pub fn open(
        &self,
        project: &str,
        worktree: Option<String>,
        label: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Result<TabInfo, String> {
        let id = self.add_tab(project, worktree, label, size(cols, rows))?;
        self.start_or_drop(project, &id)
    }

    /// A new tab with no shell yet. Returns its ID.
    fn add_tab(&self, project: &str, worktree: Option<String>, label: Option<String>, size: PtySize) -> Result<String, String> {
        let worktree = worktree.map(PathBuf::from).filter(|w| w.is_dir());
        let folder = worktree.clone().unwrap_or_else(|| PathBuf::from(project));
        if !folder.is_dir() {
            return Err(format!("Not a folder: {}", folder.display()));
        }
        let label = label.filter(|l| !l.trim().is_empty()).unwrap_or_else(|| folder_name(Path::new(project)));
        let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        let mut tab = Tab::new(project, folder, worktree, label);
        tab.size = size;
        lock(&self.0.state).add(id.clone(), tab);
        Ok(id)
    }

    /// Starts a new tab's shell; if it can't start, the tab goes.
    fn start_or_drop(&self, project: &str, id: &str) -> Result<TabInfo, String> {
        if let Err(e) = self.start(id) {
            let mut state = lock(&self.0.state);
            state.tabs.remove(id);
            state.order.retain(|t| t != id);
            drop(state);
            self.emit(project);
            return Err(e);
        }
        self.emit(project);
        self.tabs(project)
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "The tab closed at once".to_string())
    }

    /// An agent's `tilde-tab`, for the chat `chat`: a new tab in the chat's
    /// folder (`worktree`, or the project's main folder), with `command` typed into it once the
    /// shell has printed something (its prompt), up to `PROMPT_WAIT`. Typed
    /// sooner, the line still runs but shows twice: echoed before the prompt,
    /// then again at it. One line only, so nothing rides along unseen after it.
    /// Blocks while it waits.
    ///
    /// The tab is made first and handed to `show` (which puts it on screen),
    /// and its shell starts once the panel has given the tab its size, up to
    /// `SIZE_WAIT`. A shell started wider than the panel draws its prompt
    /// wrong (zsh's "%" mark shows), and that can't be fixed after.
    pub fn run_for_agent(
        &self,
        project: &str,
        chat: &str,
        worktree: Option<String>,
        command: &str,
        show: &dyn Fn(&TabInfo),
    ) -> Result<TabInfo, String> {
        let command = command.trim();
        if command.is_empty() {
            return Err("Give a command to run.".into());
        }
        if command.contains(['\n', '\r']) {
            return Err("Give one command line; join steps with && or ;.".into());
        }
        let guess = lock(&self.0.state).panel_size.unwrap_or(DEFAULT_SIZE);
        let id = self.add_tab(project, worktree, None, guess)?;
        if let Some(tab) = lock(&self.0.state).tabs.get_mut(&id) {
            tab.chat = Some(chat.to_string());
        }
        self.emit(project);
        if let Some(info) = self.tabs(project).into_iter().find(|t| t.id == id) {
            show(&info);
        }
        let deadline = Instant::now() + SIZE_WAIT;
        while Instant::now() < deadline && lock(&self.0.state).tabs.get(&id).is_some_and(|t| !t.sized && t.live.is_none()) {
            std::thread::sleep(Duration::from_millis(15));
        }
        let tab = self.start_or_drop(project, &id)?;
        let deadline = Instant::now() + PROMPT_WAIT;
        while Instant::now() < deadline && lock(&self.0.state).tabs.get(&tab.id).is_some_and(|t| !t.spoke) {
            std::thread::sleep(Duration::from_millis(30));
        }
        // A prompt is often drawn in pieces; let it finish.
        std::thread::sleep(Duration::from_millis(150));
        self.type_in(&tab.id, &format!("{command}\r"))?;
        Ok(tab)
    }

    /// Starts a shell in a tab whose shell isn't running.
    fn start(&self, id: &str) -> Result<(), String> {
        let (folder, size) = {
            let state = lock(&self.0.state);
            let tab = state.tabs.get(id).ok_or("That terminal tab is gone")?;
            if tab.live.is_some() {
                return Ok(());
            }
            let folder = if tab.folder.is_dir() { tab.folder.clone() } else { PathBuf::from(&tab.project) };
            (folder, tab.size)
        };
        let (live, reader) = start_shell(&folder, size)?;
        let mut state = lock(&self.0.state);
        let Some(tab) = state.tabs.get_mut(id) else {
            drop(state);
            end_shell(live, None, KILL_GRACE);
            return Err("That terminal tab is gone".to_string());
        };
        if tab.live.is_some() {
            drop(state);
            end_shell(live, None, KILL_GRACE);
            return Ok(());
        }
        // The window may have sized the tab while the shell was starting.
        let size = if tab.size == size || live.master.resize(tab.size).is_err() { size } else { tab.size };
        tab.mark(size);
        tab.spoke = false;
        tab.folder = folder;
        tab.run += 1;
        tab.live = Some(live);
        tab.running = None;
        tab.fg = None;
        tab.alt_screen = false;
        let run = tab.run;
        let weak = Arc::downgrade(&self.0);
        let id = id.to_string();
        #[cfg(windows)]
        {
            let (weak, id) = (weak.clone(), id.clone());
            std::thread::spawn(move || watch_exit(weak, id, run));
        }
        std::thread::spawn(move || read_loop(weak, id, run, reader));
        Ok(())
    }

    /// The window's sink for a tab's output. Returns everything kept so far,
    /// size marks included; what comes next goes to `channel`.
    pub fn attach(&self, id: &str, channel: Channel<InvokeResponseBody>) -> Result<Vec<u8>, String> {
        let mut state = lock(&self.0.state);
        let tab = state.tabs.get_mut(id).ok_or("That terminal tab is gone")?;
        tab.channel = Some(channel);
        tab.used = Instant::now();
        Ok(tab.tail.clone())
    }

    /// What the user typed. Starts a fresh shell first in a closed tab. A tab
    /// an agent opened is the user's from here on.
    pub fn write(&self, id: &str, data: &str) -> Result<(), String> {
        // Escape sequences aren't typing: the window's own answers to the
        // shell's questions (cursor place, focus) come this way too.
        let typed = !data.starts_with('\x1b') || data.starts_with(PASTE_START);
        if typed {
            if let Some(tab) = lock(&self.0.state).tabs.get_mut(id) {
                if tab.chat.take().is_some() {
                    tab.dirty = true;
                }
            }
        }
        self.type_in(id, data)
    }

    fn type_in(&self, id: &str, data: &str) -> Result<(), String> {
        let (project, started) = {
            let mut state = lock(&self.0.state);
            let tab = state.tabs.get_mut(id).ok_or("That terminal tab is gone")?;
            tab.used = Instant::now();
            (tab.project.clone(), tab.live.is_none())
        };
        if started {
            self.start(id)?;
            self.emit(&project);
        }
        let mut state = lock(&self.0.state);
        let tab = state.tabs.get_mut(id).ok_or("That terminal tab is gone")?;
        let live = tab.live.as_mut().ok_or("The shell isn't running")?;
        live.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        live.writer.flush().map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), String> {
        let mut state = lock(&self.0.state);
        let tab = state.tabs.get_mut(id).ok_or("That terminal tab is gone")?;
        tab.size = size(cols, rows);
        tab.sized = true;
        let new = tab.size;
        if let Some(live) = &tab.live {
            live.master.resize(new).map_err(|e| e.to_string())?;
            tab.mark(new);
        }
        state.panel_size = Some(new);
        Ok(())
    }

    /// Which tab is on screen, if any. A tab on screen counts as used, and
    /// what finished in it as seen.
    pub fn place(&self, id: Option<String>, visible: bool) {
        let seen = {
            let mut state = lock(&self.0.state);
            let shown = id.filter(|_| visible);
            let mut seen = None;
            if let Some(tab) = shown.as_ref().and_then(|id| state.tabs.get_mut(id)) {
                tab.used = Instant::now();
                if std::mem::take(&mut tab.done) {
                    seen = Some(tab.project.clone());
                }
            }
            state.shown = shown;
            seen
        };
        if let Some(project) = seen {
            self.emit(&project);
        }
    }

    /// What runs in a tab besides its shell, checked now: for asking before
    /// closing it.
    pub fn running(&self, id: &str) -> Option<String> {
        self.check_running();
        lock(&self.0.state).tabs.get(id).and_then(|t| t.running.clone())
    }

    /// The user dragged a tab to place `to` among its project's tabs.
    pub fn move_tab(&self, id: &str, to: usize) {
        let project = {
            let mut state = lock(&self.0.state);
            let Some(project) = state.tabs.get(id).map(|t| t.project.clone()) else { return };
            // The project's other tabs, in order; it goes before the one now at `to`.
            let others: Vec<String> = state
                .order
                .iter()
                .filter(|t| *t != id && state.tabs.get(*t).is_some_and(|tab| tab.project == project))
                .cloned()
                .collect();
            state.order.retain(|t| t != id);
            let at = match others.get(to) {
                Some(next) => state.order.iter().position(|t| t == next).unwrap_or(state.order.len()),
                None => others
                    .last()
                    .and_then(|last| state.order.iter().position(|t| t == last))
                    .map_or(state.order.len(), |p| p + 1),
            };
            state.order.insert(at, id.to_string());
            project
        };
        self.emit(&project);
    }

    /// Closes a tab for good, ending its shell and everything it started.
    pub fn close(&self, id: &str) {
        let (project, live, fg) = {
            let mut state = lock(&self.0.state);
            let Some(mut tab) = state.tabs.remove(id) else { return };
            state.order.retain(|t| t != id);
            if state.shown.as_deref() == Some(id) {
                state.shown = None;
            }
            (tab.project.clone(), tab.live.take(), tab.fg)
        };
        if let Some(live) = live {
            std::thread::spawn(move || end_shell(live, fg, KILL_GRACE));
        }
        self.emit(&project);
    }

    /// Ends a tab's shell but keeps the tab, its output and a line saying why.
    fn close_with(&self, id: &str, why: impl FnOnce(Option<&str>) -> String) {
        let (project, live, fg) = {
            let mut state = lock(&self.0.state);
            let Some(tab) = state.tabs.get_mut(id) else { return };
            let text = why(tab.running.as_deref());
            let live = tab.live.take();
            // Whatever the old shell still prints is dropped.
            tab.run += 1;
            let fg = tab.fg.take();
            tab.running = None;
            tab.line(&text);
            (tab.project.clone(), live, fg)
        };
        if let Some(live) = live {
            std::thread::spawn(move || end_shell(live, fg, KILL_GRACE));
        }
        self.emit(&project);
    }

    /// The tab's output as plain text: what the agent reads.
    pub fn text(&self, id: &str, lines: usize) -> Result<String, String> {
        let state = lock(&self.0.state);
        let tab = state.tabs.get(id).ok_or("That terminal tab is gone")?;
        Ok(tab_text(tab, lines))
    }

    /// Whether a running shell sits in this worktree folder (or below it).
    pub fn in_worktree(&self, root: &Path) -> bool {
        lock(&self.0.state)
            .tabs
            .values()
            .any(|t| t.live.is_some() && t.worktree.as_ref().is_some_and(|w| w.starts_with(root)))
    }

    /// A project's folder moved: its tabs go with it, and one that starts in
    /// the old folder starts in the new one. Shells already running stay as
    /// they are.
    pub fn project_moved(&self, from: &str, to: &str) {
        {
            let mut state = lock(&self.0.state);
            for tab in state.tabs.values_mut().filter(|tab| tab.project == from) {
                tab.project = to.to_string();
                if let Some(folder) = crate::store::swap_root(&tab.folder.to_string_lossy(), from, to) {
                    tab.folder = PathBuf::from(folder);
                }
            }
        }
        self.emit(from);
        self.emit(to);
    }

    /// A chat was deleted: the tabs its agent opened go with it, except those
    /// with something still running and those the user has typed in.
    pub fn chat_gone(&self, chat: &str) {
        self.check_running();
        let ids: Vec<String> = lock(&self.0.state)
            .tabs
            .iter()
            .filter(|(_, t)| t.chat.as_deref() == Some(chat) && t.running.is_none())
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.close(&id);
        }
    }

    /// A worktree folder is gone: its tabs close with a line saying why, and
    /// start again in the project's main folder.
    pub fn worktree_gone(&self, root: &Path, why: &str) {
        let ids: Vec<String> = {
            let mut state = lock(&self.0.state);
            let ids: Vec<String> = state
                .tabs
                .iter()
                .filter(|(_, t)| t.worktree.as_ref().is_some_and(|w| w.starts_with(root)))
                .map(|(id, _)| id.clone())
                .collect();
            for id in &ids {
                if let Some(tab) = state.tabs.get_mut(id) {
                    tab.worktree = None;
                    tab.folder = PathBuf::from(&tab.project);
                    tab.label = folder_name(&tab.folder);
                }
            }
            ids
        };
        for id in ids {
            self.close_with(&id, |_| format!("closed: worktree removed ({why})"));
        }
    }

    /// Checks what runs in each tab (every second), closes tabs left unused
    /// too long (every 30 seconds), and saves output now and then.
    pub fn tick(&self) {
        let ticks = {
            let mut ticks = lock(&self.0.ticks);
            *ticks += 1;
            *ticks
        };
        self.check_running();
        if ticks % SWEEP_TICKS == 0 {
            self.close_unused();
        }
        if ticks % SAVE_TICKS == 0 {
            let dirty = lock(&self.0.state).tabs.values().any(|t| t.dirty);
            if dirty {
                self.save();
            }
        }
    }

    fn check_running(&self) {
        // Foreground groups that changed: looked up outside the lock.
        let changed: Vec<(String, Option<i32>)> = {
            let state = lock(&self.0.state);
            state
                .tabs
                .iter()
                .filter_map(|(id, tab)| {
                    let fg = tab.live.as_ref().and_then(foreground);
                    (fg != tab.fg).then(|| (id.clone(), fg))
                })
                .collect()
        };
        if changed.is_empty() {
            return;
        }
        let looked: Vec<(String, Option<i32>, Option<String>)> = changed
            .into_iter()
            .map(|(id, fg)| {
                let line = fg.and_then(command_line);
                (id, fg, line)
            })
            .collect();
        let mut projects = Vec::new();
        {
            let mut state = lock(&self.0.state);
            let shown = state.shown.clone();
            for (id, fg, line) in looked {
                if let Some(tab) = state.tabs.get_mut(&id) {
                    tab.fg = fg;
                    let running = fg.map(|_| line.unwrap_or_else(|| "a command".to_string()));
                    if running != tab.running {
                        // A command ending off screen is marked until the tab is looked at.
                        tab.done = running.is_none() && shown.as_deref() != Some(id.as_str());
                        tab.running = running;
                        if !projects.contains(&tab.project) {
                            projects.push(tab.project.clone());
                        }
                    }
                }
            }
        }
        for project in projects {
            self.emit(&project);
        }
    }

    fn close_unused(&self) {
        let idle = self.0.idle;
        let unused: Vec<String> = {
            let mut state = lock(&self.0.state);
            let shown = state.shown.clone();
            let now = Instant::now();
            state
                .tabs
                .iter_mut()
                .filter_map(|(id, tab)| {
                    if shown.as_deref() == Some(id) {
                        tab.used = now;
                    }
                    (tab.live.is_some() && now.duration_since(tab.used) >= idle).then(|| id.clone())
                })
                .collect()
        };
        let span = idle_words(idle);
        for id in unused {
            self.close_with(&id, |running| match running {
                Some(line) => format!("closed after {span} unused: {line}"),
                None => format!("closed after {span} unused"),
            });
        }
    }

    /// Saves every tab's output, for the next start.
    pub fn save(&self) {
        let (rows, panel) = {
            let mut state = lock(&self.0.state);
            let panel = state.panel_size;
            let order = state.order.clone();
            let mut positions: HashMap<String, i64> = HashMap::new();
            let now = now_ms();
            let mut rows = Vec::new();
            for id in order {
                let Some(tab) = state.tabs.get_mut(&id) else { continue };
                tab.dirty = false;
                let position = positions.entry(tab.project.clone()).or_insert(0);
                let start = if tab.tail.len() > SAVE_MAX { trim_point(&tab.tail, tab.tail.len() - SAVE_MAX) } else { 0 };
                let mut output = last_size_mark(&tab.tail[..start]).map(|m| m.to_vec()).unwrap_or_default();
                output.extend_from_slice(&tab.tail[start..]);
                rows.push(SavedTerminal {
                    id: id.clone(),
                    project: tab.project.clone(),
                    folder: tab.folder.to_string_lossy().into_owned(),
                    worktree: tab.worktree.as_ref().map(|w| w.to_string_lossy().into_owned()),
                    label: tab.label.clone(),
                    position: *position,
                    output,
                    saved_at: now,
                    chat: tab.chat.clone(),
                });
                *position += 1;
            }
            (rows, panel)
        };
        let _ = self.0.store.save_terminals(&rows);
        if let Some(p) = panel {
            let _ = self.0.store.set_setting(PANEL_SIZE_KEY, &format!("{}x{}", p.cols, p.rows));
        }
    }

    /// Quitting: saves, then ends every shell, waiting briefly.
    pub fn shutdown(&self) {
        self.save();
        let lives: Vec<(Live, Option<i32>)> = {
            let mut state = lock(&self.0.state);
            state
                .tabs
                .values_mut()
                .filter_map(|tab| {
                    tab.channel = None;
                    tab.live.take().map(|live| (live, tab.fg))
                })
                .collect()
        };
        let handles: Vec<_> = lives
            .into_iter()
            .map(|(live, fg)| std::thread::spawn(move || end_shell(live, fg, QUIT_GRACE)))
            .collect();
        for handle in handles {
            let _ = handle.join();
        }
    }

    /// The agent's tools, for a chat in `project`.
    pub async fn call(&self, project: &str, tool: &str, args: &Value) -> Result<Vec<Value>, String> {
        let text = |s: String| Ok(vec![json!({ "type": "text", "text": s })]);
        match tool {
            "tabs" => {
                let state = lock(&self.0.state);
                let tabs = infos(&state, project);
                if tabs.is_empty() {
                    return text("No terminal tabs are open for this project.".to_string());
                }
                let lines: Vec<String> = tabs
                    .iter()
                    .map(|info| {
                        let tab = &state.tabs[&info.id];
                        let what = match (&info.running, info.closed) {
                            (_, true) => "closed".to_string(),
                            (Some(line), _) => format!("running: {line}"),
                            (None, _) => "at its prompt".to_string(),
                        };
                        let shown = if state.shown.as_deref() == Some(&info.id) { ", on screen" } else { "" };
                        format!("{} ({}) in {}{shown}", info.title, what, tab.folder.display())
                    })
                    .collect();
                text(lines.join("\n"))
            }
            "read" => {
                let lines = args["lines"].as_u64().map(|n| n as usize).unwrap_or(READ_LINES).clamp(1, MAX_READ_LINES);
                let mut state = lock(&self.0.state);
                let tabs = infos(&state, project);
                if tabs.is_empty() {
                    return text("No terminal tabs are open for this project.".to_string());
                }
                let id = match args["tab"].as_str().filter(|s| !s.is_empty()) {
                    Some(name) => tabs
                        .iter()
                        .find(|t| t.title == name || t.id == name)
                        .map(|t| t.id.clone())
                        .ok_or_else(|| {
                            let names: Vec<&str> = tabs.iter().map(|t| t.title.as_str()).collect();
                            format!("No tab called {name}. The tabs are: {}", names.join(", "))
                        })?,
                    None => state
                        .shown
                        .clone()
                        .filter(|s| tabs.iter().any(|t| &t.id == s))
                        .unwrap_or_else(|| tabs[tabs.len() - 1].id.clone()),
                };
                let title = tabs.iter().find(|t| t.id == id).map(|t| t.title.clone()).unwrap_or_default();
                let tab = state.tabs.get_mut(&id).ok_or("That terminal tab is gone")?;
                tab.used = Instant::now();
                let body = tab_text(tab, lines);
                text(format!("Tab {title}:\n{body}"))
            }
            _ => Err(format!("No such tool: {tool}")),
        }
    }
}

/// The tools, as MCP tool definitions.
pub fn tools() -> Value {
    json!([
        {
            "name": "tabs",
            "description": "List this project's terminal tabs in the side panel: each tab's name, what runs in it, and its folder.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "read",
            "description": "Read a terminal tab's recent output as plain text. Without a tab, reads the one on screen (or the newest).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tab": { "type": "string", "description": "The tab's name from tabs, or the ID tilde-tab gave" },
                    "lines": { "type": "number", "description": "How many of the last lines to read (default 200, at most 2000)" },
                },
            },
        },
    ])
}

/// A project's tabs, in order, with names made unique: where two share one,
/// the later opened is " 2", wherever it sits in the row.
fn infos(state: &State, project: &str) -> Vec<TabInfo> {
    let mut mine: Vec<(&String, &Tab)> =
        state.order.iter().filter_map(|id| Some((id, state.tabs.get(id).filter(|t| t.project == project)?))).collect();
    let mut by_age: Vec<&(&String, &Tab)> = mine.iter().collect();
    by_age.sort_by_key(|(_, tab)| tab.born);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut titles: HashMap<String, String> = HashMap::new();
    for (id, tab) in by_age {
        let base = tab.base_title();
        let n = seen.entry(base.clone()).or_insert(0);
        *n += 1;
        titles.insert((*id).clone(), if *n == 1 { base } else { format!("{base} {n}") });
    }
    mine.drain(..)
        .map(|(id, tab)| {
            let title = titles.remove(id).unwrap_or_default();
            TabInfo {
                id: id.clone(),
                title,
                running: tab.running.clone(),
                closed: tab.live.is_none(),
                done: tab.done,
                worktree: tab.worktree.as_ref().map(|w| w.to_string_lossy().into_owned()),
            }
        })
        .collect()
}

fn tab_text(tab: &Tab, lines: usize) -> String {
    let body = plain_text(&tab.tail, lines);
    if tab.alt_screen && tab.live.is_some() {
        format!("(A full-screen program is running; this is its raw output.)\n{body}")
    } else {
        body
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 }
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// "python3" from "/usr/bin/python3 -m http.server 8000"; "zsh" from "-zsh".
fn command_name(line: &str) -> String {
    let first = line.split_whitespace().next().unwrap_or(line);
    let name = first.rsplit('/').next().unwrap_or(first);
    name.trim_start_matches('-').to_string()
}

/// "a day" for the usual limit; seconds or minutes for tests.
fn idle_words(idle: Duration) -> String {
    let secs = idle.as_secs();
    match secs {
        86_400 => "a day".to_string(),
        s if s >= 86_400 && s % 86_400 == 0 => format!("{} days", s / 86_400),
        s if s >= 3600 && s % 3600 == 0 => format!("{} h", s / 3600),
        s if s >= 60 && s % 60 == 0 => format!("{} min", s / 60),
        s => format!("{s} s"),
    }
}

/// A line the app writes into a tab: leaves a full-screen program's screen if
/// one has it (`alt_screen`), resets colours, and draws a short ── text ──,
/// dimmed. Short, not across the tab, so it fits a panel of any width.
///
/// Leaving the full-screen screen also puts the cursor back where it was
/// saved. With no such screen to leave, that is the top of the window, and
/// the line would be written over the output there: so only when one is up.
fn line_bytes(text: &str, alt_screen: bool) -> Vec<u8> {
    let leave = if alt_screen { "\x1b[?1049l" } else { "" };
    format!("{leave}\x1b[0m\x1b[?25h\r\n\x1b[2m── {text} ──\x1b[0m\r\n").into_bytes()
}

/// How a size mark starts: an escape sequence of the app's own, which a
/// terminal that doesn't know it skips.
const SIZE_MARK: &[u8] = b"\x1b]7717;";

/// The mark in kept output that says the shell draws at `size` from there on.
fn size_mark(size: PtySize) -> Vec<u8> {
    let mut mark = SIZE_MARK.to_vec();
    mark.extend_from_slice(format!("{}x{}\x07", size.cols, size.rows).as_bytes());
    mark
}

/// The last size mark in `bytes`, whole.
fn last_size_mark(bytes: &[u8]) -> Option<&[u8]> {
    let mut end = bytes.len();
    while let Some(at) = bytes[..end].windows(SIZE_MARK.len()).rposition(|w| w == SIZE_MARK) {
        let rest = &bytes[at..];
        if let Some(len) = rest.iter().take(SIZE_MARK.len() + 12).position(|b| *b == 0x07) {
            if mark_size(&rest[..=len]).is_some() {
                return Some(&rest[..=len]);
            }
        }
        end = at;
    }
    None
}

/// The size a mark names.
fn mark_size(mark: &[u8]) -> Option<PtySize> {
    let text = std::str::from_utf8(mark.strip_prefix(SIZE_MARK)?.strip_suffix(b"\x07")?).ok()?;
    let (cols, rows) = text.split_once('x')?;
    Some(size(cols.parse().ok()?, rows.parse().ok()?))
}

/// Where to cut kept output so at least `at` bytes go: just after the next
/// line break, if one is near.
fn trim_point(bytes: &[u8], at: usize) -> usize {
    bytes[at..]
        .iter()
        .take(4096)
        .position(|b| *b == b'\n')
        .map(|p| at + p + 1)
        .unwrap_or(at)
}

/// Whether a full-screen program has the screen after this output.
fn alt_screen_after(was: bool, bytes: &[u8]) -> bool {
    let mut state = was;
    let mut last = 0;
    for (seq, on) in [
        (&b"\x1b[?1049h"[..], true),
        (b"\x1b[?1049l", false),
        (b"\x1b[?1047h", true),
        (b"\x1b[?1047l", false),
        (b"\x1b[?47h", true),
        (b"\x1b[?47l", false),
    ] {
        if let Some(at) = bytes.windows(seq.len()).rposition(|w| w == seq) {
            if at + 1 > last {
                last = at + 1;
                state = on;
            }
        }
    }
    state
}

/// A terminal's raw output as plain text, the way it reads on screen for
/// ordinary output: escape sequences dropped, a carriage return writes over
/// the line, backspace steps back, erase-line and clear-screen apply. The last
/// `max_lines` lines.
pub fn plain_text(raw: &[u8], max_lines: usize) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut lines: Vec<String> = Vec::new();
    let mut line: Vec<char> = Vec::new();
    let mut col = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                Some('[') => {
                    let mut params = String::new();
                    let mut end = None;
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            end = Some(c);
                            break;
                        }
                        params.push(c);
                    }
                    match end {
                        Some('K') => match params.as_str() {
                            "" | "0" => line.truncate(col),
                            "2" => line.clear(),
                            _ => {}
                        },
                        Some('J') if params == "2" || params == "3" => {
                            lines.clear();
                            line.clear();
                            col = 0;
                        }
                        Some('G') => col = params.parse::<usize>().unwrap_or(1).saturating_sub(1),
                        // Windows' ConPTY skips over blanks with "forward"
                        // and places text with row;col; only the column is
                        // kept, which reads right for ordinary output.
                        Some('C') => col += params.parse::<usize>().unwrap_or(1).max(1),
                        Some('H' | 'f') => {
                            let column = params.split(';').nth(1).and_then(|c| c.parse::<usize>().ok());
                            col = column.unwrap_or(1).saturating_sub(1);
                        }
                        _ => {}
                    }
                }
                Some(']') => {
                    // Up to BEL or ESC \.
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(' | ')' | '*' | '+') => {
                    chars.next();
                }
                _ => {}
            },
            '\r' => col = 0,
            '\n' => {
                lines.push(line.iter().collect::<String>().trim_end().to_string());
                line.clear();
                col = 0;
            }
            '\x08' => col = col.saturating_sub(1),
            '\t' => {
                let next = (col / 8 + 1) * 8;
                while line.len() < next {
                    line.push(' ');
                }
                col = next;
            }
            c if c.is_control() => {}
            c => {
                while line.len() < col {
                    line.push(' ');
                }
                if col < line.len() {
                    line[col] = c;
                } else {
                    line.push(c);
                }
                col += 1;
            }
        }
    }
    let last: String = line.iter().collect::<String>().trim_end().to_string();
    if !last.is_empty() {
        lines.push(last);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

/// A date for the "restored" line, in local time: "Nov 15 14:02".
fn local_time(ms: i64) -> String {
    #[cfg(unix)]
    {
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let secs = (ms / 1000) as libc::time_t;
        // SAFETY: localtime_r only writes the struct it is given.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
            return String::new();
        }
        let month = MONTHS.get(tm.tm_mon as usize).copied().unwrap_or("");
        format!("{month} {} {:02}:{:02}", tm.tm_mday, tm.tm_hour, tm.tm_min)
    }
    #[cfg(not(unix))]
    {
        let _ = ms;
        String::from("after quitting")
    }
}

/// Starts the user's shell, as a login shell so their profile sets PATH.
fn start_shell(folder: &Path, size: PtySize) -> Result<(Live, Box<dyn Read + Send>), String> {
    let pair = native_pty_system().openpty(size).map_err(|e| format!("Can't make a terminal: {e}"))?;
    let mut cmd = shell_command();
    cmd.cwd(folder);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", "Tilde");
    if std::env::var_os("LANG").is_none() {
        // Apps started from Finder get no locale.
        cmd.env("LANG", "en_US.UTF-8");
    }
    // So `claude` run here signs in the way Tilde's chats do.
    cmd.env_remove("ANTHROPIC_API_KEY");
    cmd.env_remove("ANTHROPIC_AUTH_TOKEN");
    let child = pair.slave.spawn_command(cmd).map_err(|e| format!("Can't start the shell: {e}"))?;
    // Only the shell keeps the other end open, so its exit ends the reader.
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let pid = child.process_id().unwrap_or(0) as i32;
    Ok((Live { master: pair.master, writer, child, pid }, reader))
}

fn shell_command() -> CommandBuilder {
    #[cfg(unix)]
    {
        let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new(shell);
        cmd.arg("-l");
        cmd
    }
    // PowerShell, as Windows Terminal opens: PowerShell 7 if installed, else
    // the one Windows ships. The profile still loads, so the user's prompt
    // and PSReadLine are as they know them. cmd.exe only if neither is found.
    #[cfg(not(unix))]
    {
        match windows_shell() {
            Some(shell) => {
                let mut cmd = CommandBuilder::new(shell);
                cmd.arg("-NoLogo");
                cmd
            }
            None => CommandBuilder::new_default_prog(),
        }
    }
}

#[cfg(not(unix))]
fn windows_shell() -> Option<PathBuf> {
    let on_path = |name: &str| {
        let paths = std::env::var_os("PATH")?;
        std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file())
    };
    let shipped = || {
        let root = std::env::var_os("SystemRoot")?;
        let p = PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        p.is_file().then_some(p)
    };
    on_path("pwsh.exe").or_else(shipped).or_else(|| on_path("powershell.exe"))
}

/// ConPTY, as portable-pty opens it, asks where the cursor is (`ESC[6n`)
/// before it draws anything, and waits for the answer. The window's xterm
/// would answer, but a tab can run before any window shows it (an agent's
/// command, the tests), and the question would stay in the tab's output to be
/// answered again on every replay, typing the answer into the shell. So the
/// core answers it once, "top left", and drops it from the output.
const CURSOR_QUESTION: &[u8] = b"\x1b[6n";

fn take_cursor_question(chunk: &[u8]) -> Option<Vec<u8>> {
    let at = chunk.windows(CURSOR_QUESTION.len()).position(|w| w == CURSOR_QUESTION)?;
    let mut rest = chunk[..at].to_vec();
    rest.extend_from_slice(&chunk[at + CURSOR_QUESTION.len()..]);
    Some(rest)
}

/// The process group in front, if it isn't the shell itself.
fn foreground(live: &Live) -> Option<i32> {
    #[cfg(unix)]
    {
        live.master.process_group_leader().filter(|g| *g > 0 && *g != live.pid)
    }
    #[cfg(not(unix))]
    {
        let _ = live;
        None
    }
}

/// A process's command line, as `ps` shows it.
fn command_line(pid: i32) -> Option<String> {
    #[cfg(unix)]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "args=", "-p", &pid.to_string()])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!line.is_empty()).then_some(line)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        None
    }
}

/// Reads a shell's output until it ends.
fn read_loop(weak: Weak<Inner>, id: String, run: u64, mut reader: Box<dyn Read + Send>) {
    let mut buf = vec![0u8; 64 * 1024];
    // Only ConPTY's opening question is answered here; see CURSOR_QUESTION.
    let mut asked = !cfg!(windows);
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let Some(inner) = weak.upgrade() else { return };
        let mut state = lock(&inner.state);
        let Some(tab) = state.tabs.get_mut(&id) else { return };
        if tab.run != run {
            return;
        }
        if !asked {
            if let Some(rest) = take_cursor_question(&buf[..n]) {
                asked = true;
                if let Some(live) = tab.live.as_mut() {
                    let _ = live.writer.write_all(b"\x1b[1;1R").and_then(|_| live.writer.flush());
                }
                tab.spoke = true;
                tab.push(&rest);
                continue;
            }
        }
        tab.spoke = true;
        tab.push(&buf[..n]);
    }
    shell_ended(&weak, &id, run);
}

/// ConPTY keeps its output open after the shell exits (until the pseudo
/// console is closed), so on Windows the reader never sees the end: this
/// checks on the shell instead.
#[cfg(windows)]
fn watch_exit(weak: Weak<Inner>, id: String, run: u64) {
    loop {
        std::thread::sleep(Duration::from_millis(250));
        let Some(inner) = weak.upgrade() else { return };
        let mut state = lock(&inner.state);
        let Some(tab) = state.tabs.get_mut(&id) else { return };
        if tab.run != run {
            return;
        }
        let Some(live) = tab.live.as_mut() else { return };
        if matches!(live.child.try_wait(), Ok(Some(_))) {
            drop(state);
            shell_ended(&weak, &id, run);
            return;
        }
    }
}

/// The shell ended. If nothing closed it on purpose, it exited by itself:
/// its tab goes too.
fn shell_ended(weak: &Weak<Inner>, id: &str, run: u64) {
    let id = id.to_string();
    let Some(inner) = weak.upgrade() else { return };
    let ended = {
        let mut state = lock(&inner.state);
        match state.tabs.get(&id) {
            Some(tab) if tab.run == run && tab.live.is_some() => {
                let tab = state.tabs.remove(&id);
                state.order.retain(|t| t != &id);
                if state.shown.as_deref() == Some(id.as_str()) {
                    state.shown = None;
                }
                tab
            }
            _ => None,
        }
    };
    if let Some(mut tab) = ended {
        Terminals(inner).emit(&tab.project);
        if let Some(live) = tab.live.take() {
            end_shell(live, tab.fg, KILL_GRACE);
        }
    }
}

/// Ends a shell and what it started: hang up its group (and the job in
/// front), wait up to `grace`, then kill what is left. Blocks; call it on a
/// thread of its own.
fn end_shell(live: Live, fg: Option<i32>, grace: Duration) {
    let Live { master, writer, mut child, pid } = live;
    let groups: Vec<i32> = [Some(pid), fg].into_iter().flatten().filter(|g| *g > 0).collect();
    #[cfg(unix)]
    for group in &groups {
        // SAFETY: plain signal sends to process groups this tab started.
        unsafe { libc::kill(-group, libc::SIGHUP) };
    }
    #[cfg(not(unix))]
    let _ = child.kill();
    let deadline = Instant::now() + grace;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    #[cfg(unix)]
    for group in &groups {
        // SAFETY: as above; a group already gone just fails.
        unsafe { libc::kill(-group, libc::SIGKILL) };
    }
    let _ = child.kill();
    let _ = child.wait();
    drop(writer);
    drop(master);
    let _ = groups;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prints the terminal's size as "rows cols" in the platform's shell.
    #[cfg(unix)]
    const SIZE: &str = "stty size";
    #[cfg(not(unix))]
    const SIZE: &str = "$s = $Host.UI.RawUI.WindowSize; \"$($s.Height) $($s.Width)\"";

    fn rig(idle: Duration) -> (Terminals, Arc<Mutex<Vec<Envelope>>>) {
        let seen: Arc<Mutex<Vec<Envelope>>> = Arc::default();
        let out: Out = {
            let seen = seen.clone();
            Arc::new(move |e: Envelope| seen.lock().unwrap().push(e))
        };
        (Terminals::with(out, Arc::new(Store::in_memory()), idle), seen)
    }

    fn folder() -> String {
        std::env::temp_dir().to_string_lossy().into_owned()
    }

    /// Waits until the tab's output holds `needle`.
    fn wait_for(t: &Terminals, id: &str, needle: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if t.text(id, 500).map(|s| s.contains(needle)).unwrap_or(false) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn plain_text_reads_like_the_screen() {
        assert_eq!(plain_text(b"\x1b[32mok\x1b[0m done\r\n", 10), "ok done");
        // A progress bar: each step writes over the last.
        assert_eq!(plain_text(b"10%\r50%\r\x1b[K100%\nnext\n", 10), "100%\nnext");
        // Backspace and erase to end of line.
        assert_eq!(plain_text(b"abcd\x08\x08\x1b[Kxy\n", 10), "abxy");
        // A title change (OSC) and a charset switch are dropped.
        assert_eq!(plain_text(b"\x1b]0;title\x07\x1b(Bhello\n", 10), "hello");
        // Clear screen drops what came before.
        assert_eq!(plain_text(b"old\n\x1b[H\x1b[2Jnew\n", 10), "new");
        // Only the last lines.
        assert_eq!(plain_text(b"1\n2\n3\n4\n", 2), "3\n4");
        // ConPTY's cursor forward and placement keep the words apart.
        assert_eq!(plain_text(b"a\x1b[3Cb\n\x1b[5;4Hc\n", 10), "a   b\n   c");
    }

    #[test]
    fn titles_and_lines() {
        assert_eq!(command_name("/usr/bin/python3 -m http.server 8000"), "python3");
        assert_eq!(command_name("-zsh"), "zsh");
        assert_eq!(idle_words(Duration::from_secs(86_400)), "a day");
        assert_eq!(idle_words(Duration::from_secs(5)), "5 s");
        let line = String::from_utf8(line_bytes("closed", false)).unwrap();
        assert!(line.contains("── closed ──"));
        // Only a full-screen program's screen is left: otherwise the cursor would jump.
        assert!(!line.contains("1049"));
        assert!(String::from_utf8(line_bytes("closed", true)).unwrap().starts_with("\x1b[?1049l"));
        assert!(alt_screen_after(false, b"x\x1b[?1049hvim"));
        assert!(!alt_screen_after(true, b"\x1b[?1049h..\x1b[?1049l"));
    }

    #[test]
    fn a_shell_runs_commands_and_reports_what_runs() {
        let (t, _) = rig(IDLE);
        let tab = t.open(&folder(), None, None, 80, 24).unwrap();
        t.write(&tab.id, "echo hel''lo\r").unwrap();
        assert!(wait_for(&t, &tab.id, "hello"));
        // What runs is only known on Unix (Windows has no foreground group).
        #[cfg(unix)]
        what_runs(&t, &tab.id);
        t.close(&tab.id);
        assert!(t.tabs(&folder()).is_empty());
    }

    #[cfg(unix)]
    fn what_runs(t: &Terminals, id: &str) {
        t.write(id, "sleep 30\r").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut running = None;
        while Instant::now() < deadline && running.is_none() {
            std::thread::sleep(Duration::from_millis(100));
            running = t.running(id);
        }
        assert_eq!(running.as_deref(), Some("sleep 30"));
        assert_eq!(t.tabs(&folder())[0].title, "sleep");

        // Ctrl+C: back at the prompt.
        t.write(id, "\x03").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && t.running(id).is_some() {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(t.running(id), None);
        // It ended off screen: marked until the tab is on screen.
        assert!(t.tabs(&folder())[0].done);
        t.place(Some(id.to_string()), true);
        assert!(!t.tabs(&folder())[0].done);
        t.place(None, false);
    }

    #[test]
    fn an_agent_opens_a_tab_with_one_command_typed_in() {
        let (t, _) = rig(IDLE);
        // Typed before the shell's first prompt; the shell still runs it.
        let tab = t.run_for_agent(&folder(), "chat-1", None, "echo from-the-ag''ent", &|_| {}).unwrap();
        assert!(wait_for(&t, &tab.id, "from-the-agent"));
        assert!(t.run_for_agent(&folder(), "chat-1", None, "echo a\nrm -rf x", &|_| {}).is_err());
        assert!(t.run_for_agent(&folder(), "chat-1", None, "  ", &|_| {}).is_err());
        assert_eq!(t.tabs(&folder()).len(), 1);
        // Once the window has given a size, the agent's next tab starts at it.
        t.resize(&tab.id, 52, 30).unwrap();
        let next = t.run_for_agent(&folder(), "chat-1", None, SIZE, &|_| {}).unwrap();
        assert!(wait_for(&t, &next.id, "30 52"));
        // Shown first, the tab's shell waits for the size the panel gives it.
        let shown = |info: &TabInfo| {
            let (t, id) = (t.clone(), info.id.clone());
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                t.resize(&id, 47, 19).unwrap();
            });
        };
        let third = t.run_for_agent(&folder(), "chat-1", None, SIZE, &shown).unwrap();
        assert!(wait_for(&t, &third.id, "19 47"));
        t.close(&tab.id);
        t.close(&next.id);
        t.close(&third.id);
    }

    #[test]
    fn an_unused_tab_closes_keeps_its_output_and_starts_again_when_typed_in() {
        let (t, _) = rig(Duration::from_millis(1));
        let tab = t.open(&folder(), None, None, 80, 24).unwrap();
        t.write(&tab.id, "echo before\r").unwrap();
        assert!(wait_for(&t, &tab.id, "before"));
        std::thread::sleep(Duration::from_millis(20));
        t.close_unused();
        let info = &t.tabs(&folder())[0];
        assert!(info.closed);
        let text = t.text(&tab.id, 50).unwrap();
        assert!(text.contains("before"));
        assert!(text.contains("closed after 0 s unused"));

        t.write(&tab.id, "echo after\r").unwrap();
        assert!(wait_for(&t, &tab.id, "after"));
        assert!(!t.tabs(&folder())[0].closed);
        t.close(&tab.id);
    }

    #[test]
    fn a_tab_on_screen_is_never_unused() {
        let (t, _) = rig(Duration::from_millis(1));
        let tab = t.open(&folder(), None, None, 80, 24).unwrap();
        t.place(Some(tab.id.clone()), true);
        std::thread::sleep(Duration::from_millis(20));
        t.close_unused();
        assert!(!t.tabs(&folder())[0].closed);
        t.close(&tab.id);
    }

    #[test]
    fn a_shell_that_exits_takes_its_tab() {
        let (t, seen) = rig(IDLE);
        let tab = t.open(&folder(), None, None, 80, 24).unwrap();
        t.write(&tab.id, "exit\r").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && !t.tabs(&folder()).is_empty() {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(t.tabs(&folder()).is_empty());
        let last = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last.event, AgentEvent::TerminalTabs { project: folder(), tabs: vec![] });
    }

    #[test]
    fn a_dragged_tab_moves_among_its_projects_and_keeps_its_name() {
        let (t, _) = rig(IDLE);
        let project = folder();
        let a = t.open(&project, None, Some("same".into()), 80, 24).unwrap();
        let b = t.open(&project, None, Some("same".into()), 80, 24).unwrap();
        let c = t.open(&project, None, Some("other".into()), 80, 24).unwrap();
        // Another project's tab sits between, in the shared order.
        lock(&t.0.state).order.insert(1, "elsewhere".into());
        t.move_tab(&b.id, 0);
        let names: Vec<_> = t.tabs(&project).into_iter().map(|t| (t.id, t.title)).collect();
        assert_eq!(names, [(b.id.clone(), "same 2".into()), (a.id.clone(), "same".into()), (c.id.clone(), "other".into())]);
        t.move_tab(&b.id, 9);
        let ids: Vec<_> = t.tabs(&project).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, [a.id.clone(), c.id.clone(), b.id.clone()]);
        t.move_tab(&a.id, 1);
        let ids: Vec<_> = t.tabs(&project).into_iter().map(|t| t.id).collect();
        assert_eq!(ids, [c.id.clone(), a.id.clone(), b.id.clone()]);
        assert!(lock(&t.0.state).order.contains(&"elsewhere".to_string()));
        t.shutdown();
    }

    #[test]
    fn kept_output_keeps_its_size_when_its_start_is_cut() {
        let mut tab = Tab::new("p", PathBuf::from("."), None, "p".into());
        tab.mark(size(130, 40));
        tab.push(&vec![b'a'; TAIL_MAX]);
        tab.push(b"more\n");
        assert!(tab.tail.starts_with(&size_mark(size(130, 40))));
        // The same size again adds nothing; a new one is noted at the end.
        let before = tab.tail.len();
        tab.mark(size(130, 40));
        assert_eq!(tab.tail.len(), before);
        tab.mark(size(77, 61));
        assert_eq!(last_size_mark(&tab.tail).and_then(mark_size), Some(size(77, 61)));
        assert_eq!(last_size_mark(b"\x1b]7717;junk\x07 and \x1b]7717;9"), None);
    }

    #[test]
    fn tabs_come_back_after_a_quit_closed_with_their_output() {
        let store = Arc::new(Store::in_memory());
        let project = folder();
        store.touch_session("chat-1", &project, "hi", "default", "default").unwrap();
        let out: Out = Arc::new(|_| {});
        let t = Terminals::with(out.clone(), store.clone(), IDLE);
        let tab = t.open(&project, None, Some("scratch".into()), 80, 24).unwrap();
        t.resize(&tab.id, 52, 30).unwrap();
        t.write(&tab.id, "echo kept\r").unwrap();
        assert!(wait_for(&t, &tab.id, "kept"));
        t.shutdown();

        let again = Terminals::with(out, store, IDLE);
        again.restore();
        let tabs = again.tabs(&project);
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].closed);
        assert_eq!(tabs[0].title, "scratch");
        let text = again.text(&tabs[0].id, 50).unwrap();
        assert!(text.contains("kept"));
        assert!(text.contains("restored"));
        // The output says what size it was drawn at, and the agent never sees that.
        let raw = lock(&again.0.state).tabs[&tabs[0].id].tail.clone();
        assert_eq!(last_size_mark(&raw).and_then(mark_size), Some(size(52, 30)));
        assert!(!text.contains("7717"));
        // The panel's size comes back too: the agent's first tab starts at it.
        let tab = again.run_for_agent(&project, "chat-1", None, SIZE, &|_| {}).unwrap();
        assert!(wait_for(&again, &tab.id, "30 52"));
        again.close(&tab.id);
    }

    #[test]
    fn a_deleted_chats_agent_tabs_go_unless_busy_or_typed_in() {
        let (t, _) = rig(IDLE);
        let project = folder();
        let mine = t.open(&project, None, None, 80, 24).unwrap();
        let idle = t.run_for_agent(&project, "chat-1", None, "echo o''ne", &|_| {}).unwrap();
        let typed = t.run_for_agent(&project, "chat-1", None, "echo t''wo", &|_| {}).unwrap();
        let busy = t.run_for_agent(&project, "chat-1", None, "sleep 30", &|_| {}).unwrap();
        let other = t.run_for_agent(&project, "chat-2", None, "echo th''ree", &|_| {}).unwrap();
        assert!(wait_for(&t, &idle.id, "one"));
        assert!(wait_for(&t, &typed.id, "two"));
        assert!(wait_for(&t, &other.id, "three"));
        // The window answering the shell isn't the user typing.
        t.write(&idle.id, "\x1b[I").unwrap();
        t.write(&typed.id, "echo fo''ur\r").unwrap();
        assert!(wait_for(&t, &typed.id, "four"));
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && (t.running(&busy.id).is_none() || t.running(&typed.id).is_some()) {
            std::thread::sleep(Duration::from_millis(50));
        }

        t.chat_gone("chat-1");
        let left: Vec<String> = t.tabs(&project).into_iter().map(|x| x.id).collect();
        assert_eq!(left, vec![mine.id.clone(), typed.id.clone(), busy.id.clone(), other.id.clone()]);
        for id in left {
            t.close(&id);
        }
    }

    #[test]
    fn a_worktree_going_closes_only_its_tabs() {
        let (t, _) = rig(IDLE);
        let base = std::env::temp_dir().join(format!("tilde-term-{}", uuid::Uuid::new_v4().simple()));
        let wt = base.join("wt");
        std::fs::create_dir_all(wt.join("sub")).unwrap();
        let project = folder();
        let main = t.open(&project, None, None, 80, 24).unwrap();
        let inside = t.open(&project, Some(wt.join("sub").to_string_lossy().into_owned()), Some("idle-timeout".into()), 80, 24).unwrap();
        assert!(t.in_worktree(&wt));
        assert!(!t.in_worktree(&base.join("other")));

        t.worktree_gone(&wt, "chat deleted");
        let tabs = t.tabs(&project);
        let gone = tabs.iter().find(|x| x.id == inside.id).unwrap();
        assert!(gone.closed);
        assert_eq!(gone.worktree, None);
        assert!(t.text(&inside.id, 10).unwrap().contains("closed: worktree removed (chat deleted)"));
        assert!(!tabs.iter().find(|x| x.id == main.id).unwrap().closed);
        assert!(!t.in_worktree(&wt));
        t.close(&main.id);
        t.close(&inside.id);
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn the_agent_reads_a_tab_by_name() {
        let (t, _) = rig(IDLE);
        let project = folder();
        let none = t.call(&project, "read", &json!({})).await.unwrap();
        assert!(none[0]["text"].as_str().unwrap().contains("No terminal tabs"));
        let tab = t.open(&project, None, Some("scratch".into()), 80, 24).unwrap();
        t.write(&tab.id, "echo for-claude\r").unwrap();
        assert!(wait_for(&t, &tab.id, "for-claude"));
        let read = t.call(&project, "read", &json!({ "tab": "scratch" })).await.unwrap();
        assert!(read[0]["text"].as_str().unwrap().contains("for-claude"));
        let list = t.call(&project, "tabs", &json!({})).await.unwrap();
        assert!(list[0]["text"].as_str().unwrap().contains("scratch (at its prompt)"));
        assert!(t.call(&project, "read", &json!({ "tab": "nope" })).await.is_err());
        // Another project sees none of it.
        let other = t.call("/elsewhere", "read", &json!({})).await.unwrap();
        assert!(other[0]["text"].as_str().unwrap().contains("No terminal tabs"));
        t.close(&tab.id);
    }

    #[test]
    fn every_tool_has_a_schema() {
        for tool in tools().as_array().unwrap() {
            assert!(tool["name"].is_string());
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }
}
