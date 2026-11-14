//! Pull request status for a worktree's branch, from GitHub's command line
//! tool (`gh`). Without `gh`, or for a repo not on GitHub, there is simply no
//! status. See PLAN.md, "Worktrees".

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use crate::locate;
use crate::worktree::PrInfo;

/// How long one lookup may take.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Repos `gh` couldn't answer for (no GitHub remote, not signed in). Not asked
/// again until the app restarts.
static NO_GITHUB: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// The newest pull request whose head is `branch`, in any state. None when
/// there is none, or no way to ask.
pub fn pr_for(repo_root: &Path, branch: &str) -> Result<Option<PrInfo>, String> {
    let Some(gh) = locate::gh() else {
        return Ok(None);
    };
    if NO_GITHUB.lock().unwrap().as_ref().is_some_and(|s| s.contains(repo_root)) {
        return Ok(None);
    }
    let mut cmd = Command::new(gh);
    cmd.args([
        "pr",
        "list",
        "--head",
        branch,
        "--state",
        "all",
        "--json",
        "number,state,isDraft,url,headRefOid",
        "--limit",
        "1",
    ])
    .current_dir(repo_root)
    .env("GH_PROMPT_DISABLED", "1")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().map_err(|e| format!("Could not run gh: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("gh has no output")?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let text = rx.recv_timeout(TIMEOUT);
    let status = child.wait();
    let _ = child.kill();
    let text = text.map_err(|_| "gh took too long".to_string())?;
    if !status.is_ok_and(|s| s.success()) {
        NO_GITHUB
            .lock()
            .unwrap()
            .get_or_insert_with(HashSet::new)
            .insert(repo_root.to_path_buf());
        return Ok(None);
    }
    Ok(parse(&text))
}

/// Reads `gh pr list --json number,state,isDraft,url,headRefOid` output.
fn parse(text: &str) -> Option<PrInfo> {
    let list: Value = serde_json::from_str(text).ok()?;
    let pr = list.as_array()?.first()?;
    let state = match (pr["state"].as_str()?, pr["isDraft"].as_bool().unwrap_or(false)) {
        ("OPEN", true) => "draft",
        ("OPEN", false) => "open",
        ("MERGED", _) => "merged",
        _ => "closed",
    };
    Some(PrInfo {
        number: pr["number"].as_i64()?,
        state: state.to_string(),
        url: pr["url"].as_str().unwrap_or_default().to_string(),
        head: pr["headRefOid"].as_str().unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_are_read_with_drafts_apart() {
        let pr = |state: &str, draft: bool| {
            parse(&format!(
                r#"[{{"number":7,"state":"{state}","isDraft":{draft},"url":"https://x/7","headRefOid":"abc"}}]"#
            ))
            .unwrap()
        };
        assert_eq!(pr("OPEN", false).state, "open");
        assert_eq!(pr("OPEN", true).state, "draft");
        assert_eq!(pr("MERGED", false).state, "merged");
        assert_eq!(pr("CLOSED", false).state, "closed");
        let p = pr("OPEN", false);
        assert_eq!((p.number, p.url.as_str(), p.head.as_str()), (7, "https://x/7", "abc"));
        assert_eq!(parse("[]"), None);
        assert_eq!(parse("not json"), None);
    }
}
