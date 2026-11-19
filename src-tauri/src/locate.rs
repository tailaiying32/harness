//! Find the agents' programs: Claude Code, Codex, Antigravity's `agy` and Cursor's agent. Sign-in isn't checked here: a chat finds out
//! from Claude Code itself (see `SignInNeeded`). Checking at startup with
//! `claude auth status` reported a working login as signed out.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

const BINARY: &str = if cfg!(windows) { "claude.exe" } else { "claude" };

#[derive(Debug, Clone, Serialize)]
pub struct SetupInfo {
    /// Claude Code's program.
    pub binary: Option<String>,
    /// Cursor's agent program.
    pub cursor: Option<String>,
    /// Codex's program.
    pub codex: Option<String>,
    /// Antigravity's program (`agy`).
    pub antigravity: Option<String>,
    /// The agents found, in `agent::ALL`'s order.
    pub agents: Vec<String>,
    /// What to tell the user when no agent was found.
    pub problem: Option<String>,
}

use crate::home::user_home as home;

fn existing(path: impl AsRef<Path>) -> Option<PathBuf> {
    let path = path.as_ref();
    path.is_file().then(|| path.to_path_buf())
}

/// Ask the user's login shell. Apps started from Finder don't get the terminal's
/// PATH, so a plain PATH lookup misses anything set up in shell config files.
#[cfg(unix)]
fn from_login_shell(name: &str) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = Command::new(shell)
        .args(["-lc", &format!("command -v {name}")])
        .output()
        .ok()?;
    // Shell config files may print their own lines; the answer is the last one.
    // An alias prints `alias claude=...`, which is not a file, so it is skipped.
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .map(str::trim)
        .find_map(existing)
}

#[cfg(windows)]
fn from_login_shell(name: &str) -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    let output = Command::new("where")
        .arg(name)
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find_map(existing)
}

fn from_path() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|dir| existing(dir.join(BINARY)))
}

/// Order: explicit override, login shell, the installer's default spot, PATH.
pub fn find_claude() -> Option<PathBuf> {
    std::env::var_os("HARNESS_CLAUDE_PATH")
        .and_then(existing)
        .or_else(|| from_login_shell("claude"))
        .or_else(|| home().and_then(|h| existing(h.join(".local").join("bin").join(BINARY))))
        .or_else(from_path)
}

/// Cursor's agent. It installs as `agent` and `cursor-agent` in
/// `~/.local/bin`; `cursor-agent` is looked for first, since `agent` is a
/// common name another program could have.
pub fn find_cursor() -> Option<PathBuf> {
    let exe = |name: &str| if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let local = |name: &str| home().and_then(|h| existing(h.join(".local").join("bin").join(exe(name))));
    std::env::var_os("HARNESS_CURSOR_PATH")
        .and_then(existing)
        .or_else(|| local("cursor-agent"))
        .or_else(|| from_login_shell("cursor-agent"))
        .or_else(|| local("agent"))
}

/// Codex: Homebrew's spots first (its cask links there), then `~/.local/bin`
/// and the login shell (npm's global folder is on its PATH).
pub fn find_codex() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "codex.exe" } else { "codex" };
    std::env::var_os("HARNESS_CODEX_PATH")
        .and_then(existing)
        .or_else(|| existing("/opt/homebrew/bin/codex"))
        .or_else(|| existing("/usr/local/bin/codex"))
        .or_else(|| home().and_then(|h| existing(h.join(".local").join("bin").join(exe))))
        .or_else(|| from_login_shell("codex"))
}

/// Antigravity's CLI, `agy`. Its installer puts it in `~/.local/bin`.
pub fn find_antigravity() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "agy.exe" } else { "agy" };
    std::env::var_os("HARNESS_ANTIGRAVITY_PATH")
        .and_then(existing)
        .or_else(|| home().and_then(|h| existing(h.join(".local").join("bin").join(exe))))
        .or_else(|| from_login_shell("agy"))
}

/// A helper program (git, gh): its usual install spots first, then the login
/// shell. Looked up once per run.
fn find_tool(name: &str, usual: &[&str]) -> Option<PathBuf> {
    usual
        .iter()
        .find_map(existing)
        .or_else(|| from_login_shell(name))
}

/// git. On a Mac `/usr/bin/git` is always there (it offers to install the
/// command line tools if they are missing).
pub fn git() -> &'static Path {
    static GIT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    GIT.get_or_init(|| {
        find_tool("git", &["/usr/bin/git", "/opt/homebrew/bin/git", "/usr/local/bin/git"])
            .unwrap_or_else(|| PathBuf::from("git"))
    })
}

/// GitHub's command line tool, if installed. Without it, pull request status
/// is off. `HARNESS_GH` names another program to use (tests put a stand-in
/// for GitHub there).
pub fn gh() -> Option<&'static Path> {
    static GH: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    GH.get_or_init(|| {
        std::env::var_os("HARNESS_GH")
            .map(PathBuf::from)
            .or_else(|| find_tool("gh", &["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]))
    })
    .as_deref()
}

pub fn check_setup() -> SetupInfo {
    let claude = find_claude();
    let cursor = find_cursor();
    let codex = find_codex();
    let antigravity = find_antigravity();
    let agents = crate::agent::ALL
        .iter()
        .filter(|id| match **id {
            crate::agent::CLAUDE => claude.is_some(),
            crate::agent::CODEX => codex.is_some(),
            crate::agent::ANTIGRAVITY => antigravity.is_some(),
            crate::agent::CURSOR => cursor.is_some(),
            _ => false,
        })
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let shown = |p: Option<PathBuf>| p.map(|p| p.to_string_lossy().into_owned());
    SetupInfo {
        problem: agents
            .is_empty()
            .then(|| "Claude Code isn't installed, or couldn't be found.".into()),
        binary: shown(claude),
        cursor: shown(cursor),
        codex: shown(codex),
        antigravity: shown(antigravity),
        agents,
    }
}
