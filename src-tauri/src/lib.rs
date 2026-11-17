mod agent;
mod browser;
mod docs;
mod events;
mod github;
mod images;
mod locate;
mod notes;
mod sessions;
mod side_chat;
mod skills;
mod store;
mod terminal;
mod worktree;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{Manager, RunEvent, State};

use agent::claude_code::ClaudeCodeAdapter;
use agent::cursor::CursorAdapter;
use agent::{Agents, AppTools};
use browser::Browsers;
use events::{AgentEvent, Decision, Envelope, ModelInfo};
use locate::SetupInfo;
use sessions::{Out, SessionManager, TerminalHooks};
use side_chat::SideChats;
use store::{ProjectRow, SessionRow, Store};
use terminal::{TabInfo, Terminals};

/// How long quitting waits for chat processes to finish up.
const QUIT_WAIT: Duration = Duration::from_secs(3);

struct AppState {
    sessions: SessionManager,
    side: SideChats,
    /// Each chat's side-panel browser.
    browsers: Browsers,
    /// Each project's side-panel terminal tabs.
    terminals: Terminals,
    store: Arc<Store>,
    /// One app-wide channel to the window, so chats that aren't open still report status.
    channel: Arc<Mutex<Option<Channel<Envelope>>>>,
    /// Every agent and where its program is (found by `check_setup`).
    agents: Agents,
    /// Markdown files shown in the side panel, which the user may edit.
    docs: Arc<docs::Docs>,
}

/// The sidebar: every project with its chats.
#[tauri::command]
fn sidebar(state: State<'_, AppState>) -> Result<Vec<ProjectRow>, String> {
    state.store.sidebar()
}

/// A saved chat: its details, its history in order, and whether a process is
/// running for it right now.
#[derive(serde::Serialize)]
struct LoadedSession {
    session: SessionRow,
    events: Vec<AgentEvent>,
    live: bool,
}

#[tauri::command]
async fn load_session(state: State<'_, AppState>, session_id: String) -> Result<LoadedSession, String> {
    let session = state
        .store
        .session(&session_id)?
        .ok_or("That chat no longer exists")?;
    let store = state.store.clone();
    let id = session_id.clone();
    // A long chat is a lot of rows; read them off the main thread.
    let events = tauri::async_runtime::spawn_blocking(move || store.events(&id))
        .await
        .map_err(|e| e.to_string())??;
    Ok(LoadedSession {
        session,
        events,
        live: state.sessions.is_live(&session_id),
    })
}

/// Moves a chat to the trash.
#[tauri::command]
async fn delete_session(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state.side.end_chat(&session_id);
    state.browsers.close(&session_id);
    let sessions = state.sessions.clone();
    sessions.delete(&session_id).await
}

/// Chats in the trash, most recently trashed first.
#[tauri::command]
fn list_trash(state: State<'_, AppState>) -> Result<Vec<SessionRow>, String> {
    state.sessions.list_trash()
}

/// Takes a chat out of the trash.
#[tauri::command]
fn restore_chat(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state.sessions.restore_chat(&session_id)
}

/// Deletes every chat in the trash for good.
#[tauri::command]
async fn empty_trash(state: State<'_, AppState>) -> Result<(), String> {
    let sessions = state.sessions.clone();
    sessions.empty_trash().await
}

/// The answer to Claude's request for a worktree: "worktree" or "here".
#[tauri::command]
async fn choose_worktree(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    choice: String,
) -> Result<(), String> {
    let sessions = state.sessions.clone();
    sessions.choose_worktree(&session_id, &request_id, &choice).await
}

/// Gives a chat a new title.
#[tauri::command]
fn rename_session(state: State<'_, AppState>, session_id: String, title: String) -> Result<(), String> {
    state.sessions.rename(&session_id, &title)
}

#[tauri::command]
fn subscribe(state: State<'_, AppState>, on_event: Channel<Envelope>) {
    *state.channel.lock().unwrap() = Some(on_event);
}

#[tauri::command]
async fn check_setup(state: State<'_, AppState>) -> Result<SetupInfo, String> {
    // Runs a login shell; keep it off the main thread.
    let info = tauri::async_runtime::spawn_blocking(locate::check_setup)
        .await
        .map_err(|e| e.to_string())?;
    state.agents.set_binary(agent::CLAUDE, info.binary.as_ref().map(PathBuf::from));
    state.agents.set_binary(agent::CURSOR, info.cursor.as_ref().map(PathBuf::from));
    Ok(info)
}

#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    // The agent a new chat runs on; a saved chat keeps its own.
    agent: Option<String>,
    project_path: String,
    mode: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    text: String,
    // Numbers of the chat's saved images to send with it.
    images: Option<Vec<u32>>,
    message_id: String,
    // Things the window tells the agent with this message; not shown or saved.
    app_notes: Option<Vec<String>>,
) -> Result<(), String> {
    let project_path = PathBuf::from(project_path);
    if !project_path.is_dir() {
        return Err(format!("Not a folder: {}", project_path.display()));
    }
    let agent = agent.unwrap_or_else(|| agent::CLAUDE.to_string());

    // The manager is cloned so nothing borrowed from `state` is held while waiting.
    let sessions = state.sessions.clone();
    sessions
        .send_message(&session_id, &agent, project_path, mode, model, effort, &text, &images.unwrap_or_default(), &message_id, &app_notes.unwrap_or_default())
        .await
}

/// Ask one of a chat's side chats something. Starts it as a copy of the chat
/// on its first question.
#[tauri::command]
async fn side_send(
    state: State<'_, AppState>,
    side_id: String,
    project_path: String,
    model: Option<String>,
    effort: Option<String>,
    text: String,
    message_id: String,
) -> Result<(), String> {
    // Side chats run on Claude Code, which can copy a conversation.
    let (_, binary) = state.agents.get(agent::CLAUDE)?;
    let project_path = PathBuf::from(project_path);
    if !project_path.is_dir() {
        return Err(format!("Not a folder: {}", project_path.display()));
    }
    state
        .side
        .send(&side_id, binary, &project_path, model, effort, &text, &message_id)
}

#[tauri::command]
fn side_withdraw_message(state: State<'_, AppState>, side_id: String, message_id: String) -> Result<(), String> {
    state.side.withdraw_message(&side_id, &message_id)
}

#[tauri::command]
fn side_interrupt(state: State<'_, AppState>, side_id: String) -> Result<(), String> {
    state.side.interrupt(&side_id)
}

/// End a side chat and forget it.
#[tauri::command]
async fn side_end(state: State<'_, AppState>, side_id: String) -> Result<(), String> {
    state.side.end(&side_id);
    Ok(())
}

/// A fresh window: end the side chats an earlier one left running.
#[tauri::command]
async fn side_end_all(state: State<'_, AppState>) -> Result<(), String> {
    state.side.end_all();
    Ok(())
}

#[tauri::command]
fn answer_approval(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    decision: Decision,
    message: Option<String>,
    answers: Option<BTreeMap<String, String>>,
    // A plan the user edited before approving it.
    plan: Option<String>,
) -> Result<(), String> {
    state
        .sessions
        .answer_approval(&session_id, &request_id, decision, message, answers, plan)
}

/// The chat's notes, written in the side panel; empty if there are none yet.
#[tauri::command]
async fn read_notes(state: State<'_, AppState>, chat_id: String) -> Result<String, String> {
    let sessions = state.sessions.clone();
    tauri::async_runtime::spawn_blocking(move || sessions.notes().read(&chat_id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn write_notes(state: State<'_, AppState>, chat_id: String, text: String) -> Result<(), String> {
    let sessions = state.sessions.clone();
    tauri::async_runtime::spawn_blocking(move || sessions.notes().write(&chat_id, &text))
        .await
        .map_err(|e| e.to_string())?
}

/// The app's own skills, for Settings.
#[tauri::command]
async fn list_skills(state: State<'_, AppState>) -> Result<Vec<skills::SkillInfo>, String> {
    let skills = state.sessions.skills().clone();
    tauri::async_runtime::spawn_blocking(move || skills.list()).await.map_err(|e| e.to_string())
}

/// Copies a skill in from a folder, a .zip or a .md file.
#[tauri::command]
async fn import_skill(state: State<'_, AppState>, path: String) -> Result<skills::SkillInfo, String> {
    let skills = state.sessions.skills().clone();
    tauri::async_runtime::spawn_blocking(move || skills.import(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn remove_skill(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let skills = state.sessions.skills().clone();
    tauri::async_runtime::spawn_blocking(move || skills.remove(&name))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn withdraw_message(state: State<'_, AppState>, session_id: String, message_id: String) -> Result<(), String> {
    state.sessions.withdraw_message(&session_id, &message_id)
}

#[tauri::command]
fn interrupt(state: State<'_, AppState>, session_id: String) -> Result<(), String> {
    state.sessions.interrupt(&session_id)
}

#[tauri::command]
fn set_mode(state: State<'_, AppState>, session_id: String, mode: String) -> Result<(), String> {
    state.sessions.set_mode(&session_id, &mode)
}

#[tauri::command]
fn set_model(state: State<'_, AppState>, session_id: String, model: String) -> Result<(), String> {
    state.sessions.set_model(&session_id, &model)
}

#[tauri::command]
fn set_effort(state: State<'_, AppState>, session_id: String, effort: String) -> Result<(), String> {
    state.sessions.set_effort(&session_id, &effort)
}

/// Keeps a pasted or dropped image for a chat. `data` is base64. When it was
/// shrunk to fit the agent's limits, `original` is the full-size one, kept for
/// looking at. Returns the image's number, which its `[image n]` word uses.
#[tauri::command]
async fn save_image(
    state: State<'_, AppState>,
    session_id: String,
    media_type: String,
    data: String,
    original: Option<OriginalImage>,
) -> Result<images::ImageRef, String> {
    let sessions = state.sessions.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = images::decode_base64(&data)?;
        let original = match &original {
            Some(o) => Some((o.media_type.as_str(), images::decode_base64(&o.data)?)),
            None => None,
        };
        let original = original.as_ref().map(|(t, b)| (*t, b.as_slice()));
        sessions.images().save_with_original(&session_id, &media_type, &bytes, original)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Deserialize)]
struct OriginalImage {
    media_type: String,
    data: String,
}

/// A chat's image as raw bytes, for looking at: the full-size original of a
/// shrunk paste.
#[tauri::command]
async fn read_image(state: State<'_, AppState>, session_id: String, n: u32) -> Result<tauri::ipc::Response, String> {
    let sessions = state.sessions.clone();
    let (bytes, _) = tauri::async_runtime::spawn_blocking(move || sessions.images().read_largest(&session_id, n))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Image files a drop turns into `[image n]`, by extension.
const IMAGE_EXTENSIONS: [&str; 11] = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "avif", "heic", "heif", "tif", "tiff"];
/// The largest image file read for the page; larger ones go as plain files.
const MAX_IMAGE_FILE: u64 = 45 * 1024 * 1024;

fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase()
}

/// A dropped image file's bytes. Only image files, and only up to a size.
fn read_image_file_at(path: &Path) -> Result<Vec<u8>, String> {
    if !IMAGE_EXTENSIONS.contains(&extension(path).as_str()) {
        return Err("That isn't an image file".to_string());
    }
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_IMAGE_FILE {
        return Err("That image is too large".to_string());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

/// A dropped image file as raw bytes, for the page to shrink and keep as `[image n]`.
#[tauri::command]
async fn read_image_file(path: String) -> Result<tauri::ipc::Response, String> {
    let bytes = tauri::async_runtime::spawn_blocking(move || read_image_file_at(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())??;
    Ok(tauri::ipc::Response::new(bytes))
}

/// "folder" or "file" (also for a path that isn't there).
#[tauri::command]
fn file_kind(path: String) -> &'static str {
    if Path::new(&path).is_dir() { "folder" } else { "file" }
}

/// Whether clicking a file's word should show it in its folder instead of
/// opening it: folders (apps are folders on the Mac) and anything that would
/// run. A click must never run something.
fn only_reveal(path: &Path) -> bool {
    if path.is_dir() {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        ["exe", "bat", "cmd", "com", "msi", "ps1", "vbs", "js", "lnk", "scr"].contains(&extension(path).as_str())
    }
}

/// Opens a dropped file in its own app, or shows it in its folder when
/// opening would run it.
#[tauri::command]
fn open_file(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let at = Path::new(&path);
    if !at.exists() {
        return Err(format!("{path} isn't there any more"));
    }
    if only_reveal(at) {
        app.opener().reveal_item_in_dir(at).map_err(|e| e.to_string())
    } else {
        app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
    }
}

/// Paths of the files on the system clipboard, as copied in Finder or
/// Explorer. Empty when it holds something else.
#[tauri::command]
async fn clipboard_files() -> Vec<String> {
    tauri::async_runtime::spawn_blocking(|| {
        arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.get().file_list())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|path| path.to_str().map(str::to_owned))
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// A markdown file's text, for a tab in the side panel.
#[tauri::command]
async fn read_doc(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let docs = state.docs.clone();
    tauri::async_runtime::spawn_blocking(move || docs.read(Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

/// Saves the user's edit to a markdown file shown in the side panel.
#[tauri::command]
async fn write_doc(state: State<'_, AppState>, path: String, text: String) -> Result<(), String> {
    let docs = state.docs.clone();
    tauri::async_runtime::spawn_blocking(move || docs.write(Path::new(&path), &text))
        .await
        .map_err(|e| e.to_string())?
}

/// Moves a doc from the chat's folder into the project's `docs/` folder
/// (`project` is the chat's worktree when it has one). Returns the new path,
/// which the panel may then save to like any doc it shows.
#[tauri::command]
async fn save_doc_to_project(state: State<'_, AppState>, chat_id: String, path: String, project: String) -> Result<String, String> {
    let sessions = state.sessions.clone();
    let docs = state.docs.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let notes = sessions.notes();
        let saved = docs::save_to_project(&notes.folder(&chat_id)?, &notes.path(&chat_id)?, Path::new(&path), Path::new(&project))?;
        docs.read(&saved)?;
        Ok(saved.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The user typed an address in a chat's browser: into `tab`, else the tab
/// on show, else a new tab.
#[tauri::command]
async fn browser_navigate(state: State<'_, AppState>, chat_id: String, tab: Option<String>, url: String) -> Result<(), String> {
    let browsers = state.browsers.clone();
    browsers.navigate(&chat_id, tab.as_deref(), &url).await
}

#[tauri::command]
async fn browser_reload(state: State<'_, AppState>, chat_id: String, tab: String) -> Result<(), String> {
    let browsers = state.browsers.clone();
    browsers.reload(&chat_id, &tab).await
}

/// A new empty tab in a chat's browser, shown. Returns its id.
#[tauri::command]
fn browser_new_tab(state: State<'_, AppState>, chat_id: String) -> String {
    state.browsers.new_tab(&chat_id)
}

#[tauri::command]
fn browser_pick(state: State<'_, AppState>, chat_id: String, tab: String) -> Result<(), String> {
    state.browsers.pick(&chat_id, &tab)
}

/// A tab dragged to place `to` in its row.
#[tauri::command]
fn browser_move(state: State<'_, AppState>, chat_id: String, tab: String, to: usize) -> Result<(), String> {
    state.browsers.move_tab(&chat_id, &tab, to)
}

#[tauri::command]
fn browser_close_tab(state: State<'_, AppState>, chat_id: String, tab: String) -> Result<(), String> {
    state.browsers.close_tab(&chat_id, &tab)
}

/// Where the side panel's body is, in the window's logical pixels, and whether
/// the chat's browser should show there (otherwise it is parked out of sight).
#[tauri::command]
fn browser_place(state: State<'_, AppState>, chat_id: String, rect: browser::Rect, visible: bool) {
    state.browsers.place(&chat_id, rect, visible);
}

#[tauri::command]
fn browser_close(state: State<'_, AppState>, chat_id: String) {
    state.browsers.close(&chat_id);
}

/// A chat's browser tabs and the one on show: for a chat the window loads later.
#[derive(serde::Serialize)]
struct BrowserTabs {
    tabs: Vec<events::BrowserTab>,
    shown: Option<String>,
}

#[tauri::command]
fn browser_current(state: State<'_, AppState>, chat_id: String) -> BrowserTabs {
    let (tabs, shown) = state.browsers.current(&chat_id);
    BrowserTabs { tabs, shown }
}

/// A project's terminal tabs, in order.
#[tauri::command]
fn terminal_tabs(state: State<'_, AppState>, project: String) -> Vec<TabInfo> {
    state.terminals.tabs(&project)
}

/// Opens a terminal tab in the project's main folder, or in a chat's worktree
/// folder (named `label` when nothing runs in it).
#[tauri::command]
async fn terminal_new(
    state: State<'_, AppState>,
    project: String,
    worktree: Option<String>,
    label: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<TabInfo, String> {
    let terminals = state.terminals.clone();
    tauri::async_runtime::spawn_blocking(move || terminals.open(&project, worktree, label, cols, rows))
        .await
        .map_err(|e| e.to_string())?
}

/// Where a tab's output goes: everything kept so far, then what comes.
#[tauri::command]
fn terminal_attach(state: State<'_, AppState>, tab: String, on_data: Channel<InvokeResponseBody>) -> Result<(), String> {
    state.terminals.attach(&tab, on_data)
}

/// What the user typed in a tab. Starts a fresh shell in a closed one.
#[tauri::command]
async fn terminal_write(state: State<'_, AppState>, tab: String, data: String) -> Result<(), String> {
    let terminals = state.terminals.clone();
    tauri::async_runtime::spawn_blocking(move || terminals.write(&tab, &data))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn terminal_resize(state: State<'_, AppState>, tab: String, cols: u16, rows: u16) -> Result<(), String> {
    state.terminals.resize(&tab, cols, rows)
}

/// Which tab is on screen, if any.
#[tauri::command]
fn terminal_place(state: State<'_, AppState>, tab: Option<String>, visible: bool) {
    state.terminals.place(tab, visible);
}

/// What runs in a tab besides its shell, for asking before closing it.
#[tauri::command]
async fn terminal_running(state: State<'_, AppState>, tab: String) -> Result<Option<String>, String> {
    let terminals = state.terminals.clone();
    tauri::async_runtime::spawn_blocking(move || terminals.running(&tab))
        .await
        .map_err(|e| e.to_string())
}

/// A tab dragged to place `to` among its project's tabs.
#[tauri::command]
fn terminal_move(state: State<'_, AppState>, tab: String, to: usize) {
    state.terminals.move_tab(&tab, to);
}

#[tauri::command]
fn terminal_close(state: State<'_, AppState>, tab: String) {
    state.terminals.close(&tab);
}

/// A tab's output as plain text, as the agent reads it. For the self-test.
#[tauri::command]
fn terminal_text(state: State<'_, AppState>, tab: String, lines: usize) -> Result<String, String> {
    state.terminals.text(&tab, lines)
}

/// Closes the window: Cmd+W outside a terminal, as the default menu did.
#[tauri::command]
fn close_window(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

/// The models an agent offers (Claude Code if none is named). Starts a
/// short-lived agent process, so it runs off the main thread: about 7 seconds
/// for Claude Code; for Cursor a few seconds plus about one per model named in
/// `efforts_for` (whose effort levels are read). The window shows the last
/// list meanwhile.
#[tauri::command]
async fn list_models(
    state: State<'_, AppState>,
    agent: Option<String>,
    efforts_for: Option<Vec<String>>,
) -> Result<Vec<ModelInfo>, String> {
    let (adapter, binary) = state.agents.get(agent.as_deref().unwrap_or(agent::CLAUDE))?;
    tauri::async_runtime::spawn_blocking(move || adapter.list_models(&binary, efforts_for.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

/// Runs an agent's own sign-in (Claude Code if none is named), which opens
/// the browser. Waits until the user is done there, up to a few minutes.
#[tauri::command]
async fn sign_in(state: State<'_, AppState>, agent: Option<String>) -> Result<(), String> {
    let (adapter, binary) = state.agents.get(agent.as_deref().unwrap_or(agent::CLAUDE))?;
    tauri::async_runtime::spawn_blocking(move || adapter.sign_in(&binary))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn close_session(state: State<'_, AppState>, session_id: String) {
    state.sessions.close(&session_id);
}

/// Self-test settings, read from the environment. Used to smoke-test and measure
/// the real window without a person typing:
///   HARNESS_AUTOTEST_FOLDER    folder to chat in (turns the self-test on)
///   HARNESS_AUTOTEST_MESSAGES  JSON array of messages (optional)
///   HARNESS_AUTOTEST_REPORT    file the window writes its transcript to
///   HARNESS_AUTOTEST_THEME     theme id to show (optional, not saved)
///   HARNESS_AUTOTEST_SIZE      compact, regular or roomy (optional, not saved)
///   HARNESS_AUTOTEST_MODEL     model to start on (optional, not saved)
/// Messages starting with `@` steer the test; see `runSelfTest` in App.svelte.
/// Two more variables help tests without touching real data:
///   HARNESS_DATA_DIR           where the saved chats live (default: the app data folder)
///   HARNESS_IDLE_SECONDS       idle time before a chat's process is closed
///   HARNESS_WORKTREE_IDLE_SECONDS  time without a message before a worktree is put away
///   HARNESS_TRASH_SECONDS      how long put-away branches and trashed chats are kept
///   HARNESS_GH                 a program to use in place of GitHub's `gh`
///   HARNESS_BROWSER_IDLE_SECONDS  time a browser sits off screen and unused before its page is put away
///   HARNESS_TERMINAL_IDLE_SECONDS  time a terminal tab sits unused before its shell is closed
#[derive(serde::Serialize)]
struct AutoTest {
    folder: String,
    messages: Vec<String>,
    theme: Option<String>,
    size: Option<String>,
    model: Option<String>,
}

#[tauri::command]
fn autotest() -> Option<AutoTest> {
    let folder = std::env::var("HARNESS_AUTOTEST_FOLDER").ok()?;
    let messages = std::env::var("HARNESS_AUTOTEST_MESSAGES")
        .ok()
        .and_then(|m| serde_json::from_str(&m).ok())
        .unwrap_or_else(|| vec!["Reply with the single word: one".to_string()]);
    Some(AutoTest {
        folder,
        messages,
        theme: std::env::var("HARNESS_AUTOTEST_THEME").ok(),
        size: std::env::var("HARNESS_AUTOTEST_SIZE").ok(),
        model: std::env::var("HARNESS_AUTOTEST_MODEL").ok(),
    })
}

/// A pause for the self-test, timed here in the core: a hidden window's own
/// timers are slowed down by the system and can run very late.
#[tauri::command]
async fn autotest_wait(ms: u64) {
    tokio::time::sleep(Duration::from_millis(ms.min(120_000))).await;
}

/// Writes only to the path named in the environment, never one the window picks.
#[tauri::command]
fn autotest_report(text: String) -> Result<(), String> {
    let path = std::env::var("HARNESS_AUTOTEST_REPORT")
        .map_err(|_| "HARNESS_AUTOTEST_REPORT is not set".to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// The Mac menu: Tauri's default one, except Close Window (Cmd+W) asks the
/// window first, so a terminal with focus can close a tab instead.
#[cfg(target_os = "macos")]
fn app_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let info = app.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..Default::default()
    };
    Menu::with_items(
        app,
        &[
            &Submenu::with_items(
                app,
                info.name.clone(),
                true,
                &[
                    &PredefinedMenuItem::about(app, None, Some(about))?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "File",
                true,
                &[
                    &MenuItem::with_id(app, TAB_KEYS[0].0, "New Tab", true, Some("CmdOrCtrl+T"))?,
                    &MenuItem::with_id(app, TAB_KEYS[1].0, "Close Tab or Window", true, Some("CmdOrCtrl+W"))?,
                    &PredefinedMenuItem::separator(app)?,
                    &MenuItem::with_id(app, TAB_KEYS[2].0, "Show Previous Tab", true, Some("CmdOrCtrl+Shift+["))?,
                    &MenuItem::with_id(app, TAB_KEYS[3].0, "Show Next Tab", true, Some("CmdOrCtrl+Shift+]"))?,
                ],
            )?,
            &Submenu::with_items(
                app,
                "Edit",
                true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?,
            &Submenu::with_items(app, "View", true, &[&PredefinedMenuItem::fullscreen(app, None)?])?,
            &Submenu::with_items(
                app,
                "Window",
                true,
                &[
                    &PredefinedMenuItem::minimize(app, None)?,
                    &PredefinedMenuItem::maximize(app, None)?,
                ],
            )?,
        ],
    )
}

/// The tab keys' menu items, and the action each sends to the window.
#[cfg(target_os = "macos")]
const TAB_KEYS: [(&str, &str); 4] = [
    ("tab-new", "new"),
    ("tab-close", "close"),
    ("tab-prev", "prev"),
    ("tab-next", "next"),
];

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = match std::env::var_os("HARNESS_DATA_DIR") {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_data_dir()?,
            };
            let store = Arc::new(Store::open(&dir)?);
            let channel: Arc<Mutex<Option<Channel<Envelope>>>> = Arc::default();
            let out: Out = {
                let channel = channel.clone();
                Arc::new(move |envelope: Envelope| {
                    if let Some(ch) = channel.lock().unwrap().as_ref() {
                        let _ = ch.send(envelope);
                    }
                })
            };
            #[cfg(target_os = "macos")]
            {
                app.set_menu(app_menu(app.handle())?)?;
                let out = out.clone();
                app.on_menu_event(move |_, event| {
                    if let Some((_, action)) = TAB_KEYS.iter().find(|(id, _)| event.id() == *id) {
                        out(Envelope {
                            session_id: String::new(),
                            event: AgentEvent::TabKey { action: action.to_string() },
                        });
                    }
                });
            }
            let claude = Arc::new(ClaudeCodeAdapter);
            let agents = Agents::default();
            agents.add(agent::CLAUDE, claude.clone(), None);
            agents.add(agent::CURSOR, Arc::new(CursorAdapter), None);
            let side = SideChats::new(claude, store.clone(), out.clone(), dir.join("notes"), dir.join("side_copies.json"));
            let browsers = Browsers::new(app.handle().clone(), out.clone());
            let terminals = Terminals::new(out.clone(), store.clone());
            let sessions = SessionManager::new(agents.clone(), store.clone(), out, dir);
            // Every chat's agent can drive that chat's browser and read its
            // project's terminal.
            {
                let browsers = browsers.clone();
                let terminals = terminals.clone();
                let skills = sessions.skills().clone();
                sessions.set_app_tools(Arc::new(move |chat_id: &str, project: &str| {
                    let browser_tools = {
                        let browsers = browsers.clone();
                        let chat_id = chat_id.to_string();
                        AppTools {
                            server: browser::SERVER,
                            instruction: browser::INSTRUCTION.into(),
                            list: browser::tools(),
                            call: Arc::new(move |tool, args| {
                                let browsers = browsers.clone();
                                let chat_id = chat_id.clone();
                                Box::pin(async move { browsers.call(&chat_id, &tool, &args).await })
                            }),
                        }
                    };
                    let terminal_tools = {
                        let terminals = terminals.clone();
                        let project = project.to_string();
                        AppTools {
                            server: terminal::SERVER,
                            instruction: terminal::INSTRUCTION.into(),
                            list: terminal::tools(),
                            call: Arc::new(move |tool, args| {
                                let terminals = terminals.clone();
                                let project = project.clone();
                                Box::pin(async move { terminals.call(&project, &tool, &args).await })
                            }),
                        }
                    };
                    let mut tools = vec![browser_tools, terminal_tools];
                    // Read on every start, so a new chat sees the skills
                    // imported since; none means no skills tool at all.
                    let list = skills.list();
                    if !list.is_empty() {
                        let skills = skills.clone();
                        tools.push(AppTools {
                            server: skills::SERVER,
                            instruction: skills::instruction(&list),
                            list: skills::tools(&list),
                            call: Arc::new(move |tool, args| {
                                let skills = skills.clone();
                                Box::pin(async move { skills.call(&tool, &args).await })
                            }),
                        });
                    }
                    tools
                }));
            }
            // Worktree cleanup checks for and tells the terminal.
            {
                let (a, b) = (terminals.clone(), terminals.clone());
                sessions.set_terminal_hooks(TerminalHooks {
                    in_use: Arc::new(move |root: &Path| a.in_worktree(root)),
                    gone: Arc::new(move |root: &Path, why: &str| b.worktree_gone(root, why)),
                });
            }
            // Images of chats that are gone, and of new chats never sent.
            {
                let sessions = sessions.clone();
                tauri::async_runtime::spawn_blocking(move || sessions.sweep_chat_files());
            }
            // The worktree cleanup pass: at start, then every hour.
            {
                let sessions = sessions.clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        sessions.sweep().await;
                        tokio::time::sleep(sessions::SWEEP_EVERY).await;
                    }
                });
            }
            app.manage(AppState {
                sessions,
                side,
                browsers,
                terminals,
                store,
                channel,
                agents,
                docs: Arc::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_skills,
            import_skill,
            remove_skill,
            subscribe,
            check_setup,
            sidebar,
            load_session,
            delete_session,
            list_trash,
            restore_chat,
            empty_trash,
            choose_worktree,
            rename_session,
            send_message,
            answer_approval,
            withdraw_message,
            interrupt,
            set_mode,
            set_model,
            set_effort,
            list_models,
            sign_in,
            side_send,
            side_withdraw_message,
            side_interrupt,
            side_end,
            side_end_all,
            close_session,
            read_doc,
            write_doc,
            save_doc_to_project,
            read_notes,
            write_notes,
            browser_navigate,
            browser_reload,
            browser_place,
            browser_close,
            browser_new_tab,
            browser_pick,
            browser_close_tab,
            browser_move,
            browser_current,
            terminal_tabs,
            terminal_new,
            terminal_attach,
            terminal_write,
            terminal_resize,
            terminal_place,
            terminal_running,
            terminal_close,
            terminal_move,
            terminal_text,
            close_window,
            save_image,
            read_image,
            read_image_file,
            file_kind,
            open_file,
            clipboard_files,
            autotest,
            autotest_wait,
            autotest_report,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Quitting: ask every chat's process to finish up, and wait briefly.
            if let RunEvent::Exit = event {
                if let Some(state) = app.try_state::<AppState>() {
                    state.terminals.shutdown();
                    state.side.shutdown();
                    state.sessions.shutdown(QUIT_WAIT);
                }
            }
        });
}
