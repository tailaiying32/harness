//! Find the Claude Code program and check that the user is signed in.
//! The app never reads or stores credentials; it only asks the program.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

const BINARY: &str = if cfg!(windows) { "claude.exe" } else { "claude" };

#[derive(Debug, Clone, Serialize)]
pub struct SetupInfo {
    pub binary: Option<String>,
    pub logged_in: bool,
    pub subscription: Option<String>,
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
fn from_login_shell() -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let output = Command::new(shell)
        .args(["-lc", "command -v claude"])
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
fn from_login_shell() -> Option<PathBuf> {
    use std::os::windows::process::CommandExt;
    let output = Command::new("where")
        .arg("claude")
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
        .or_else(from_login_shell)
        .or_else(|| home().and_then(|h| existing(h.join(".local").join("bin").join(BINARY))))
        .or_else(from_path)
}

pub fn check_setup() -> SetupInfo {
    let Some(binary) = find_claude() else {
        return SetupInfo {
            binary: None,
            logged_in: false,
            subscription: None,
            problem: Some("Claude Code isn't installed, or couldn't be found.".into()),
        };
    };

    // `claude auth status` prints JSON with `loggedIn` and `subscriptionType`.
    let mut cmd = Command::new(&binary);
    cmd.args(["auth", "status"])
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let status: serde_json::Value = cmd
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or_default();

    let logged_in = status["loggedIn"].as_bool().unwrap_or(false);
    SetupInfo {
        binary: Some(binary.to_string_lossy().into_owned()),
        logged_in,
        subscription: status["subscriptionType"].as_str().map(str::to_owned),
        problem: (!logged_in)
            .then(|| "You're not signed in. Run `claude` once in a terminal to sign in.".into()),
    }
}
