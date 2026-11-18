//! Pull request status for a chat, from GitHub's command line tool (`gh`).
//! Without `gh`, or for a repo not on GitHub, there is simply no status.
//! See PLAN.md, "Worktrees".

use std::collections::{BTreeMap, HashMap, HashSet};
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

/// Most pull requests asked about in one GraphQL query.
const PER_QUERY: usize = 50;

/// Repos `gh` couldn't answer for (no GitHub remote, not signed in). Not asked
/// again until the app restarts.
static NO_GITHUB: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// Repo folders, each with the GitHub repo its pull requests are in.
static REPOS: Mutex<Option<HashMap<PathBuf, Repo>>> = Mutex::new(None);

/// A repo moved: what was learned about it at its old place no longer holds.
pub fn forget_under(folder: &Path) {
    if let Some(repos) = NO_GITHUB.lock().unwrap().as_mut() {
        repos.retain(|repo| !repo.starts_with(folder));
    }
    if let Some(repos) = REPOS.lock().unwrap().as_mut() {
        repos.retain(|repo, _| !repo.starts_with(folder));
    }
}

/// One pull request to look up.
#[derive(Debug, Clone, PartialEq)]
pub enum PrAsk {
    /// The newest one whose head is `branch`, in the repo at `repo_root`.
    Branch { repo_root: PathBuf, branch: String },
    /// The one at this link.
    Url(String),
}

/// A repo on GitHub (or a GitHub Enterprise host).
#[derive(Debug, Clone, PartialEq)]
struct Repo {
    host: String,
    owner: String,
    name: String,
}

/// What one alias in a query asks for.
#[derive(Debug, Clone, PartialEq)]
enum Target {
    Branch(String),
    Number(i64),
}

/// Looks up many pull requests in one GraphQL query per host, instead of one
/// `gh` call each. The answers line up with `asks`; None where there is no
/// pull request or no way to ask.
pub fn prs(asks: &[PrAsk]) -> Vec<Option<PrInfo>> {
    let mut answers = vec![None; asks.len()];
    let mut by_host: BTreeMap<String, Vec<(usize, Repo, Target)>> = BTreeMap::new();
    for (i, ask) in asks.iter().enumerate() {
        let found = match ask {
            PrAsk::Branch { repo_root, branch } => repo_of(repo_root).map(|repo| (repo, Target::Branch(branch.clone()))),
            PrAsk::Url(url) => link_parts(url).map(|(repo, number)| (repo, Target::Number(number))),
        };
        if let Some((repo, target)) = found {
            by_host.entry(repo.host.clone()).or_default().push((i, repo, target));
        }
    }
    for (host, items) in by_host {
        for part in items.chunks(PER_QUERY) {
            let asked: Vec<(Repo, Target)> = part.iter().map(|(_, repo, target)| (repo.clone(), target.clone())).collect();
            let mut args = vec!["api".to_string(), "graphql".into(), "--hostname".into(), host.clone()];
            args.extend(query_args(&asked));
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            // A query with one bad part (a repo gone) still answers the rest,
            // though `gh` reports a failure: what it printed is read anyway.
            let Ok(Some((_, text))) = run(&args, None) else {
                continue;
            };
            for ((i, _, _), answer) in part.iter().zip(read_answers(&text, part.len())) {
                answers[*i] = answer;
            }
        }
    }
    answers
}

/// The GitHub repo whose pull requests a folder's `gh pr` commands go to.
/// Asked once per folder.
fn repo_of(repo_root: &Path) -> Option<Repo> {
    if NO_GITHUB.lock().unwrap().as_ref().is_some_and(|s| s.contains(repo_root)) {
        return None;
    }
    if let Some(repo) = REPOS.lock().unwrap().as_ref().and_then(|r| r.get(repo_root)) {
        return Some(repo.clone());
    }
    let text = ask(&["repo", "view", "--json", "url"], Some(repo_root)).ok()?;
    let repo = text
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| repo_at(v["url"].as_str()?));
    match &repo {
        Some(repo) => {
            REPOS
                .lock()
                .unwrap()
                .get_or_insert_with(HashMap::new)
                .insert(repo_root.to_path_buf(), repo.clone());
        }
        None => {
            NO_GITHUB
                .lock()
                .unwrap()
                .get_or_insert_with(HashSet::new)
                .insert(repo_root.to_path_buf());
        }
    }
    repo
}

/// The repo at `https://<host>/<owner>/<name>`.
fn repo_at(url: &str) -> Option<Repo> {
    let mut parts = url.strip_prefix("https://")?.trim_end_matches('/').split('/');
    let (host, owner, name) = (parts.next()?, parts.next()?, parts.next()?);
    if host.is_empty() || owner.is_empty() || name.is_empty() {
        return None;
    }
    Some(Repo {
        host: host.into(),
        owner: owner.into(),
        name: name.into(),
    })
}

/// The repo and number of a pull request link.
fn link_parts(url: &str) -> Option<(Repo, i64)> {
    let link = pr_links(url).into_iter().next()?;
    let (repo, number) = link.rsplit_once("/pull/")?;
    Some((repo_at(repo)?, number.parse().ok()?))
}

/// The arguments after `gh api graphql --hostname <host>` that ask about each
/// of `asked`, answered as `q0`, `q1`... Names and branches go in as
/// variables, so no text is ever pasted into the query itself.
fn query_args(asked: &[(Repo, Target)]) -> Vec<String> {
    let (mut vars, mut body, mut fields) = (Vec::new(), String::new(), Vec::new());
    for (k, (repo, target)) in asked.iter().enumerate() {
        vars.push(format!("$o{k}:String!,$r{k}:String!"));
        fields.extend(["-f".to_string(), format!("o{k}={}", repo.owner), "-f".into(), format!("r{k}={}", repo.name)]);
        let pick = match target {
            Target::Branch(branch) => {
                vars.push(format!("$b{k}:String!"));
                fields.extend(["-f".to_string(), format!("b{k}={branch}")]);
                format!("pullRequests(headRefName:$b{k},first:1,orderBy:{{field:CREATED_AT,direction:DESC}}){{nodes{{...P}}}}")
            }
            Target::Number(number) => {
                vars.push(format!("$n{k}:Int!"));
                fields.extend(["-F".to_string(), format!("n{k}={number}")]);
                format!("pullRequest(number:$n{k}){{...P}}")
            }
        };
        body.push_str(&format!("q{k}:repository(owner:$o{k},name:$r{k}){{{pick}}} "));
    }
    let query = format!(
        "query({}){{{body}}} fragment P on PullRequest{{number state isDraft url headRefOid headRefName}}",
        vars.join(",")
    );
    let mut args = vec!["-f".to_string(), format!("query={query}")];
    args.extend(fields);
    args
}

/// The answers to a query from `query_args`, in order.
fn read_answers(text: &str, count: usize) -> Vec<Option<PrInfo>> {
    let data = serde_json::from_str::<Value>(text).map(|v| v["data"].clone()).unwrap_or(Value::Null);
    (0..count)
        .map(|k| {
            let answer = &data[format!("q{k}").as_str()];
            let pr = match answer.get("pullRequests") {
                Some(list) => &list["nodes"][0],
                None => &answer["pullRequest"],
            };
            parse(pr)
        })
        .collect()
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
    Ok(run(args, folder)?.and_then(|(ok, text)| ok.then_some(text)))
}

/// Runs `gh`: whether it succeeded, and what it printed. None without `gh`.
fn run(args: &[&str], folder: Option<&Path>) -> Result<Option<(bool, String)>, String> {
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
    Ok(Some((status.is_ok_and(|s| s.success()), text)))
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
    fn many_pull_requests_are_asked_in_one_query() {
        let repo = |host: &str| Repo {
            host: host.into(),
            owner: "o".into(),
            name: "r".into(),
        };
        assert_eq!(link_parts("https://ghe.corp/o/r/pull/12"), Some((repo("ghe.corp"), 12)));
        assert_eq!(link_parts("https://github.com/o/r/issues/12"), None);
        assert_eq!(repo_at("https://github.com/o/r"), Some(repo("github.com")));

        let args = query_args(&[
            (repo("github.com"), Target::Branch("a\"b".into())),
            (repo("github.com"), Target::Number(7)),
        ]);
        let query = args[1].strip_prefix("query=").unwrap();
        assert!(query.starts_with("query($o0:String!,$r0:String!,$b0:String!,$o1:String!,$r1:String!,$n1:Int!)"));
        assert!(query.contains("q0:repository(owner:$o0,name:$r0){pullRequests(headRefName:$b0,"));
        assert!(query.contains("q1:repository(owner:$o1,name:$r1){pullRequest(number:$n1){...P}}"));
        assert!(!query.contains("a\""), "branch names go in as variables");
        assert!(args.windows(2).any(|w| w == ["-f", "b0=a\"b"]));
        assert!(args.windows(2).any(|w| w == ["-F", "n1=7"]));

        let pr = r#"{"number":7,"state":"MERGED","isDraft":false,"url":"https://github.com/o/r/pull/7","headRefOid":"abc","headRefName":"x"}"#;
        let text = format!(r#"{{"data":{{"q0":{{"pullRequests":{{"nodes":[]}}}},"q1":{{"pullRequest":{pr}}},"q2":null}}}}"#);
        let answers = read_answers(&text, 3);
        assert_eq!(answers[0], None, "no pull request for the branch");
        assert_eq!(answers[1].as_ref().map(|p| p.state.as_str()), Some("merged"));
        assert_eq!(answers[2], None, "a repo that couldn't be read");
        assert_eq!(read_answers("not json", 2), vec![None, None]);
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
