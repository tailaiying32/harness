//! Find the agents' programs: Claude Code, Codex, Antigravity's `agy` and Cursor's agent. Sign-in isn't checked here: a chat finds out
//! from Claude Code itself (see `SignInNeeded`). Checking at startup with
//! `claude auth status` reported a working login as signed out.

use std::path::{Path, PathBuf};
#[cfg(unix)]
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
    (path.is_file() && runnable(path)).then(|| path.to_path_buf())
}

/// Whether Windows can start the file directly. npm puts an extensionless
/// `#!/bin/sh` script next to each `.cmd` it installs; starting that fails
/// with "not a valid Win32 application", so only programs and batch files
/// count. Elsewhere any file does (the execute bit is the user's business).
fn runnable(path: &Path) -> bool {
    if !cfg!(windows) {
        return true;
    }
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ["exe", "cmd", "bat"].iter().any(|x| e.eq_ignore_ascii_case(x)))
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

/// On Windows an app gets the user's full PATH, so there is no login shell to
/// ask: PATH is read here rather than through `where`, whose process start
/// costs more than the whole search. A real program is preferred to npm's
/// `.cmd` wrapper anywhere on PATH, then the wrapper.
#[cfg(windows)]
fn from_login_shell(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&paths).collect();
    ["exe", "cmd", "bat"]
        .iter()
        .find_map(|ext| dirs.iter().find_map(|dir| existing(dir.join(format!("{name}.{ext}")))))
}

fn from_path() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).find_map(|dir| existing(dir.join(BINARY)))
}

/// Order: explicit override, login shell, the installer's default spot, PATH.
/// On Windows the installer's spot goes first: it is a file check, and
/// the PATH may hold an older npm copy.
pub fn find_claude() -> Option<PathBuf> {
    let installed = || home().and_then(|h| existing(h.join(".local").join("bin").join(BINARY)));
    let found = crate::home::app_env("CLAUDE_PATH").and_then(existing);
    if cfg!(windows) {
        found.or_else(installed).or_else(|| from_login_shell("claude"))
    } else {
        found
            .or_else(|| from_login_shell("claude"))
            .or_else(installed)
            .or_else(from_path)
    }
}

/// Cursor's agent. It installs as `agent` and `cursor-agent` in
/// `~/.local/bin`; `cursor-agent` is looked for first, since `agent` is a
/// common name another program could have.
pub fn find_cursor() -> Option<PathBuf> {
    let exe = |name: &str| if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let local = |name: &str| home().and_then(|h| existing(h.join(".local").join("bin").join(exe(name))));
    crate::home::app_env("CURSOR_PATH")
        .and_then(existing)
        .or_else(|| local("cursor-agent"))
        .or_else(|| from_login_shell("cursor-agent"))
        .or_else(|| local("agent"))
}

/// Codex: its installer's spot first (Homebrew's on a Mac, where its cask
/// links; OpenAI's installer folder on Windows), then `~/.local/bin` and the
/// login shell (npm's global folder is on its PATH).
pub fn find_codex() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "codex.exe" } else { "codex" };
    let usual: Vec<PathBuf> = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(|d| PathBuf::from(d).join(r"Programs\OpenAI\Codex\bin\codex.exe"))
            .into_iter()
            .collect()
    } else {
        vec!["/opt/homebrew/bin/codex".into(), "/usr/local/bin/codex".into()]
    };
    crate::home::app_env("CODEX_PATH")
        .and_then(existing)
        .or_else(|| usual.iter().find_map(existing))
        .or_else(|| home().and_then(|h| existing(h.join(".local").join("bin").join(exe))))
        .or_else(|| from_login_shell("codex"))
}

/// Antigravity's CLI, `agy`. Its installer puts it in `~/.local/bin`.
pub fn find_antigravity() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "agy.exe" } else { "agy" };
    crate::home::app_env("ANTIGRAVITY_PATH")
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
        let usual: &[&str] = if cfg!(windows) {
            &[r"C:\Program Files\Git\cmd\git.exe"]
        } else {
            &["/usr/bin/git", "/opt/homebrew/bin/git", "/usr/local/bin/git"]
        };
        find_tool("git", usual)
            .unwrap_or_else(|| PathBuf::from("git"))
    })
}

/// GitHub's command line tool, if installed. Without it, pull request status
/// is off. `TILDE_GH` names another program to use (tests put a stand-in
/// for GitHub there).
pub fn gh() -> Option<&'static Path> {
    static GH: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    GH.get_or_init(|| {
        crate::home::app_env("GH")
            .map(PathBuf::from)
            .or_else(|| {
                let usual: &[&str] = if cfg!(windows) {
                    &[r"C:\Program Files\GitHub CLI\gh.exe"]
                } else {
                    &["/opt/homebrew/bin/gh", "/usr/local/bin/gh"]
                };
                find_tool("gh", usual)
            })
    })
    .as_deref()
}

pub fn check_setup() -> SetupInfo {
    // At the same time: each may start a login shell.
    let (claude, cursor, codex, antigravity) = std::thread::scope(|s| {
        let cursor = s.spawn(find_cursor);
        let codex = s.spawn(find_codex);
        let antigravity = s.spawn(find_antigravity);
        (
            find_claude(),
            cursor.join().ok().flatten(),
            codex.join().ok().flatten(),
            antigravity.join().ok().flatten(),
        )
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_shell_shims_are_not_taken_for_programs_on_windows() {
        let dir = std::env::temp_dir().join(format!("tilde-locate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["tool", "tool.cmd", "tool.exe"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        assert_eq!(existing(dir.join("tool")).is_some(), !cfg!(windows));
        assert!(existing(dir.join("tool.cmd")).is_some());
        assert!(existing(dir.join("tool.exe")).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_three_agents_are_looked_up_without_hanging() {
        let started = std::time::Instant::now();
        let info = check_setup();
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        eprintln!("{info:?} in {:?}", started.elapsed());
    }
}
