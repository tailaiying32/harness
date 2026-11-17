//! Pull request status for a chat, from GitHub's command line tool (`gh`).
//! Without `gh`, or for a repo not on GitHub, there is simply no status.
//! See PLAN.md, "Worktrees".

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

/// What is asked about each pull request.
const FIELDS: &str = "number,state,isDraft,url,headRefOid,headRefName";

/// Repos `gh` couldn't answer for (no GitHub remote, not signed in). Not asked
/// again until the app restarts.
static NO_GITHUB: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// A repo moved: what was learned about it at its old place no longer holds.
pub fn forget_under(folder: &Path) {
    if let Some(repos) = NO_GITHUB.lock().unwrap().as_mut() {
        repos.retain(|repo| !repo.starts_with(folder));
    }
}

/// The newest pull request whose head is `branch`, in any state. None when
/// there is none, or no way to ask.
pub fn pr_for(repo_root: &Path, branch: &str) -> Result<Option<PrInfo>, String> {
    if NO_GITHUB.lock().unwrap().as_ref().is_some_and(|s| s.contains(repo_root)) {
        return Ok(None);
    }
    let args = ["pr", "list", "--head", branch, "--state", "all", "--json", FIELDS, "--limit", "1"];
    let Some(text) = ask(&args, Some(repo_root))? else {
        NO_GITHUB
            .lock()
            .unwrap()
            .get_or_insert_with(HashSet::new)
            .insert(repo_root.to_path_buf());
        return Ok(None);
    };
    let list: Option<Value> = serde_json::from_str(&text).ok();
    Ok(list.as_ref().and_then(|l| l.as_array()?.first()).and_then(parse))
}

/// The pull request at `url`. None when it can't be read: a failure here may
/// be one bad link, so it doesn't stop later lookups.
pub fn pr_at(url: &str) -> Result<Option<PrInfo>, String> {
    let Some(text) = ask(&["pr", "view", url, "--json", FIELDS], None)? else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<Value>(&text).ok().as_ref().and_then(parse))
}

/// Runs `gh` and returns what it printed. None without `gh`, or when it fails.
fn ask(args: &[&str], folder: Option<&Path>) -> Result<Option<String>, String> {
    let Some(gh) = locate::gh() else {
        return Ok(None);
    };
    let mut cmd = Command::new(gh);
    cmd.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(folder) = folder {
        cmd.current_dir(folder);
    }
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
    if text.is_err() {
        let _ = child.kill();
    }
    let status = child.wait();
    let text = text.map_err(|_| "gh took too long".to_string())?;
    Ok(status.is_ok_and(|s| s.success()).then_some(text))
}

/// Reads one pull request from `gh`'s JSON (the `FIELDS` above).
fn parse(pr: &Value) -> Option<PrInfo> {
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
        branch: pr["headRefName"].as_str().unwrap_or_default().to_string(),
    })
}

/// The pull request links in a piece of text, each once, in order. A link is
/// `https://<host>/<owner>/<repo>/pull/<number>`; anything after the number
/// (a path, a query, punctuation) is dropped.
pub fn pr_links(text: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    for (at, _) in text.match_indices("https://") {
        let rest = &text[at + "https://".len()..];
        let end = rest.find(|c: char| c.is_whitespace() || "\"'<>()[]".contains(c)).unwrap_or(rest.len());
        let mut parts = rest[..end].split('/');
        let (Some(host), Some(owner), Some(repo), Some("pull"), Some(number)) =
            (parts.next(), parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let digits: String = number.chars().take_while(char::is_ascii_digit).collect();
        if host.is_empty() || owner.is_empty() || repo.is_empty() || digits.is_empty() {
            continue;
        }
        let link = format!("https://{host}/{owner}/{repo}/pull/{digits}");
        if !links.contains(&link) {
            links.push(link);
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_are_read_with_drafts_apart() {
        let pr = |state: &str, draft: bool| {
            let json = format!(
                r#"{{"number":7,"state":"{state}","isDraft":{draft},"url":"https://x/7","headRefOid":"abc","headRefName":"part-1"}}"#
            );
            parse(&serde_json::from_str(&json).unwrap()).unwrap()
        };
        assert_eq!(pr("OPEN", false).state, "open");
        assert_eq!(pr("OPEN", true).state, "draft");
        assert_eq!(pr("MERGED", false).state, "merged");
        assert_eq!(pr("CLOSED", false).state, "closed");
        let p = pr("OPEN", false);
        assert_eq!((p.number, p.url.as_str(), p.head.as_str(), p.branch.as_str()), (7, "https://x/7", "abc", "part-1"));
        assert_eq!(parse(&Value::Null), None);
    }

    #[test]
    fn pull_request_links_are_found_in_text() {
        let made = "Creating pull request for part-1 into main in o/r\n\nhttps://github.com/o/r/pull/12\n";
        assert_eq!(pr_links(made), vec!["https://github.com/o/r/pull/12"]);
        let exists = "a pull request for branch \"x\" into branch \"main\" already exists:\nhttps://github.com/o/r/pull/5";
        assert_eq!(pr_links(exists), vec!["https://github.com/o/r/pull/5"]);
        // Trimmed to the pull request itself, each once, other hosts too.
        let mixed = "see https://github.com/o/r/pull/5/files, (https://ghe.corp/a/b/pull/9) and https://github.com/o/r/pull/5.";
        assert_eq!(pr_links(mixed), vec!["https://github.com/o/r/pull/5", "https://ghe.corp/a/b/pull/9"]);
        // Not pull requests.
        assert!(pr_links("https://github.com/o/r/issues/5 https://github.com/o/r/pull/new/x https://github.com/o/r").is_empty());
    }
}
