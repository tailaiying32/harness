//! Find the Claude Code program. Sign-in isn't checked here: a chat finds out
//! from Claude Code itself (see `SignInNeeded`). Checking at startup with
//! `claude auth status` reported a working login as signed out.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

const BINARY: &str = if cfg!(windows) { "claude.exe" } else { "claude" };

#[derive(Debug, Clone, Serialize)]
pub struct SetupInfo {
    pub binary: Option<String>,
    /// What to tell the user when something is missing.
    pub problem: Option<String>,
}

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

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

/// GitHub's command line tool, if installed. Without it, pull request status is off.
pub fn gh() -> Option<&'static Path> {
    static GH: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    GH.get_or_init(|| find_tool("gh", &["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]))
        .as_deref()
}

pub fn check_setup() -> SetupInfo {
    match find_claude() {
        Some(binary) => SetupInfo {
            binary: Some(binary.to_string_lossy().into_owned()),
            problem: None,
        },
        None => SetupInfo {
            binary: None,
            problem: Some("Claude Code isn't installed, or couldn't be found.".into()),
        },
    }
}
