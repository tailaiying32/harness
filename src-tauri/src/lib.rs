mod agent;
mod events;
mod locate;
mod sessions;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::State;
use tauri::ipc::Channel;

use agent::EventSink;
use agent::claude_code::ClaudeCodeAdapter;
use events::{AgentEvent, Decision, Envelope};
use locate::SetupInfo;
use sessions::SessionManager;

struct AppState {
    sessions: SessionManager,
    /// One app-wide channel to the window, so chats that aren't open still report status.
    channel: Arc<Mutex<Option<Channel<Envelope>>>>,
    binary: Mutex<Option<PathBuf>>,
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
    text: String,
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

    let channel = state.channel.clone();
    let id = session_id.clone();
    let emit: EventSink = Arc::new(move |event: AgentEvent| {
        if let Some(ch) = channel.lock().unwrap().as_ref() {
            let _ = ch.send(Envelope {
                session_id: id.clone(),
                event,
            });
        }
    });
    state
        .sessions
        .send_message(&session_id, binary, project_path, mode, &text, emit)
}

#[tauri::command]
fn answer_approval(
    state: State<'_, AppState>,
    session_id: String,
    request_id: String,
    decision: Decision,
    message: Option<String>,
) -> Result<(), String> {
    state
        .sessions
        .answer_approval(&session_id, &request_id, decision, message)
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
#[derive(serde::Serialize)]
struct AutoTest {
    folder: String,
    messages: Vec<String>,
    theme: Option<String>,
    size: Option<String>,
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
    })
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
        .manage(AppState {
            sessions: SessionManager::new(Box::new(ClaudeCodeAdapter)),
            channel: Arc::default(),
            binary: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            subscribe,
            check_setup,
            send_message,
            answer_approval,
            interrupt,
            set_mode,
            close_session,
            autotest,
            autotest_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
