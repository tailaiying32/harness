mod agent;
mod events;
mod github;
mod locate;
mod sessions;
mod store;
mod worktree;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::{Manager, RunEvent, State};

use agent::claude_code::ClaudeCodeAdapter;
use events::{AgentEvent, Decision, Envelope, ModelInfo};
use locate::SetupInfo;
use sessions::{Out, SessionManager};
use store::{ProjectRow, SessionRow, Store};

/// How long quitting waits for chat processes to finish up.
const QUIT_WAIT: Duration = Duration::from_secs(3);

struct AppState {
    sessions: SessionManager,
    store: Arc<Store>,
    /// One app-wide channel to the window, so chats that aren't open still report status.
    channel: Arc<Mutex<Option<Channel<Envelope>>>>,
    binary: Mutex<Option<PathBuf>>,
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
    let binary = state
        .binary
        .lock()
        .unwrap()
        .clone()
        .ok_or("Claude Code couldn't be found")?;
    let sessions = state.sessions.clone();
    sessions
        .choose_worktree(&session_id, &request_id, &choice, binary)
        .await
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
    // Runs a login shell and `claude auth status`; keep it off the main thread.
    let info = tauri::async_runtime::spawn_blocking(locate::check_setup)
        .await
        .map_err(|e| e.to_string())?;
    *state.binary.lock().unwrap() = info.binary.as_ref().map(PathBuf::from);
    Ok(info)
}

#[tauri::command]
async fn send_message(
    state: State<'_, AppState>,
    session_id: String,
    project_path: String,
    mode: Option<String>,
    model: Option<String>,
    text: String,
    message_id: String,
) -> Result<(), String> {
    let binary = state
        .binary
        .lock()
        .unwrap()
        .clone()
        .ok_or("Claude Code couldn't be found")?;
    let project_path = PathBuf::from(project_path);
    if !project_path.is_dir() {
        return Err(format!("Not a folder: {}", project_path.display()));
    }

    // The manager is cloned so nothing borrowed from `state` is held while waiting.
    let sessions = state.sessions.clone();
    sessions
        .send_message(&session_id, binary, project_path, mode, model, &text, &message_id)
        .await
}

#[tauri::command]
fn answer_approval(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    decision: Decision,
    message: Option<String>,
    answers: Option<BTreeMap<String, String>>,
) -> Result<(), String> {
    state
        .sessions
        .answer_approval(&session_id, &request_id, decision, message, answers)
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

/// The models the agent offers. Starts a short-lived agent process, so it runs
/// off the main thread and takes about a second.
#[tauri::command]
async fn list_models(state: State<'_, AppState>) -> Result<Vec<ModelInfo>, String> {
    let binary = state
        .binary
        .lock()
        .unwrap()
        .clone()
        .ok_or("Claude Code couldn't be found")?;
    let adapter = state.sessions.adapter();
    tauri::async_runtime::spawn_blocking(move || adapter.list_models(&binary))
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
            let sessions = SessionManager::new(Arc::new(ClaudeCodeAdapter), store.clone(), out, dir);
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
                store,
                channel,
                binary: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
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
            interrupt,
            set_mode,
            set_model,
            list_models,
            close_session,
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
                    state.sessions.shutdown(QUIT_WAIT);
                }
            }
        });
}
