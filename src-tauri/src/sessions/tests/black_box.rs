//! Black-box tests for "several pull requests per chat", written from the
//! behaviour spec only. Each test is one row of the spec (R1.., B1..) and
//! checks only what a user could see: folders, branches, git status, the
//! chat's saved pull request list, the mark, and events sent to the window.

use super::*;
use crate::events::Block;
use crate::worktree::{summary, PrInfo, Worktree};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

/// The second branch a split task moves to.
const B: &str = "part-b";

// ---- A fake GitHub ----

/// Knows a set of pull requests and answers by branch or by link, as the real
/// one would. Can be unreachable, and can be slow for one pull request.
#[derive(Clone, Default)]
struct Gh {
    prs: Arc<Mutex<Vec<PrInfo>>>,
    down: Arc<AtomicBool>,
    /// Lookups that would answer with this pull request wait while it is set.
    slow: Arc<Mutex<Option<i64>>>,
    /// A lookup is waiting right now.
    waiting: Arc<AtomicBool>,
}

fn link(number: i64) -> String {
    format!("https://github.com/o/r/pull/{number}")
}

impl Gh {
    fn lookup(&self) -> PrLookup {
        let (by_branch, by_url) = (self.clone(), self.clone());
        PrLookup {
            by_branch: Arc::new(move |_: &Path, branch: &str| by_branch.answer(|pr| pr.branch == branch)),
            by_url: Arc::new(move |url: &str| by_url.answer(|pr| pr.url == url.trim())),
        }
    }

    fn answer(&self, pick: impl Fn(&PrInfo) -> bool) -> Result<Option<PrInfo>, String> {
        if self.down.load(Ordering::SeqCst) {
            return Err("could not reach github.com".into());
        }
        let find = || self.prs.lock().unwrap().iter().rev().find(|pr| pick(pr)).cloned();
        if let Some(pr) = find() {
            // A slow answer: wait until released (or give up after a while).
            for _ in 0..300 {
                if *self.slow.lock().unwrap() != Some(pr.number) {
                    break;
                }
                self.waiting.store(true, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(10));
            }
            self.waiting.store(false, Ordering::SeqCst);
        }
        Ok(find())
    }

    fn add(&self, number: i64, state: &str, branch: &str, head: &str) {
        self.prs.lock().unwrap().push(PrInfo {
            number,
            state: state.into(),
            url: link(number),
            head: head.into(),
            branch: branch.into(),
        });
    }

    fn set(&self, number: i64, state: &str) {
        let mut prs = self.prs.lock().unwrap();
        prs.iter_mut().find(|pr| pr.number == number).expect("a known pull request").state = state.into();
    }
}

// ---- Small helpers ----

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

/// Writes a file and commits it. Returns the new tip.
fn commit(dir: &Path, file: &str, text: &str) -> String {
    std::fs::write(dir.join(file), text).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", file]);
    git(dir, &["rev-parse", "HEAD"])
}

fn on_branch(dir: &Path) -> String {
    git(dir, &["symbolic-ref", "--short", "HEAD"])
}

fn has_branch(repo: &Path, name: &str) -> bool {
    !git(repo, &["branch", "--list", name]).is_empty()
}

fn prs(rig: &Rig) -> Vec<PrInfo> {
    let saved = rig.store.session("s1").unwrap().unwrap().prs;
    assert_eq!(saved, rig.store.chat_git("s1").unwrap().unwrap().prs, "both views of the list agree");
    saved
}

fn numbers(rig: &Rig) -> Vec<i64> {
    prs(rig).iter().map(|pr| pr.number).collect()
}

fn states(rig: &Rig) -> Vec<String> {
    prs(rig).iter().map(|pr| pr.state.clone()).collect()
}

fn mark(rig: &Rig) -> Option<&'static str> {
    summary(&prs(rig))
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(250)).await;
}

/// The agent runs a shell command and it prints `output`.
fn bash(rig: &Rig, id: &str, command: &str, output: serde_json::Value, is_error: bool) {
    rig.agent_says(AgentEvent::AssistantBlock {
        message_id: format!("msg-{id}"),
        block: Block::ToolCall {
            tool_id: id.into(),
            name: "Bash".into(),
            input: serde_json::json!({ "command": command }),
        },
        parent_tool_id: None,
    });
    rig.agent_says(AgentEvent::ToolResult {
        tool_id: id.into(),
        content: output,
        is_error,
        parent_tool_id: None,
    });
}

/// The agent opens a pull request from the branch `dir` is on: GitHub now
/// has it, and `gh pr create` prints its link.
fn opens(rig: &Rig, gh: &Gh, number: i64, state: &str, dir: &Path) {
    gh.add(number, state, &on_branch(dir), &git(dir, &["rev-parse", "HEAD"]));
    let flag = if state == "draft" { " --draft" } else { "" };
    bash(
        rig,
        &format!("pr-{number}"),
        &format!("gh pr create --title x --body y{flag}"),
        serde_json::json!(format!("{}\n", link(number))),
        false,
    );
}

/// Ends the turn and waits for the app's background lookups.
async fn end_turn(rig: &Rig) {
    rig.manager.0.pr_checked.lock().unwrap().clear();
    rig.turn_ends();
    settle().await;
}

/// Closes the chat's process, so cleanup may touch its folder.
async fn close(rig: &Rig) {
    rig.manager.close("s1");
    tokio::time::sleep(Duration::from_millis(20)).await;
}

/// One cleanup pass that may ask GitHub again.
async fn cleanup(rig: &Rig) {
    rig.manager.0.pr_checked.lock().unwrap().clear();
    rig.sweep().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
}

/// Like `rig_with`, but over a data folder the test keeps (for restarts).
fn rig_in(dir: &Path, worktree_idle: Duration, keep: Duration, lookup: PrLookup) -> Rig {
    let fake = Arc::new(Fake::default());
    let store = Arc::new(Store::open(dir).unwrap());
    let seen: Arc<Mutex<Vec<AgentEvent>>> = Arc::default();
    let out: Out = {
        let seen = seen.clone();
        Arc::new(move |envelope: Envelope| seen.lock().unwrap().push(envelope.event))
    };
    let timings = Timings {
        idle: Some(LONG),
        close_grace: Duration::from_millis(100),
        worktree_idle: Some(worktree_idle),
        keep: Some(keep),
    };
    let manager = SessionManager::build(fake_agents(&fake), store.clone(), out, timings, dir.to_path_buf(), Home::new(dir.join("home")), lookup);
    Rig {
        fake,
        store,
        seen,
        manager,
    }
}

fn data_dir() -> PathBuf {
    std::env::temp_dir().join(format!("harness-bb-{}", uuid::Uuid::new_v4()))
}

/// A chat in its own worktree, with the fake GitHub.
async fn chat(worktree_idle: Duration) -> (Rig, Gh, PathBuf, Worktree) {
    let gh = Gh::default();
    let rig = rig_with(LONG, worktree_idle, LONG, gh.lookup());
    let repo = scratch_repo();
    let wt = rig.into_worktree(&repo).await;
    (rig, gh, repo, wt)
}

/// A task split in two: a commit and pull request 1 on the chat's first
/// branch (A), then the folder switches to a new branch (B, built on A when
/// `stacked`, else cut from main) with a commit and pull request 2. The turn
/// then ends. Returns both tips.
async fn two_parts(rig: &Rig, gh: &Gh, wt: &Worktree, stacked: bool) -> (String, String) {
    let tip_a = commit(&wt.path, "a.txt", "part one\n");
    opens(rig, gh, 1, "open", &wt.path);
    // The second part takes the agent a while (R3b covers no pause at all).
    settle().await;
    if stacked {
        git(&wt.path, &["checkout", "-q", "-b", B]);
    } else {
        git(&wt.path, &["checkout", "-q", "-b", B, "main"]);
    }
    let tip_b = commit(&wt.path, "b.txt", "part two\n");
    opens(rig, gh, 2, "open", &wt.path);
    end_turn(rig).await;
    assert_eq!(numbers(rig), [1, 2], "both pull requests are tracked");
    (tip_a, tip_b)
}

// ---- R1: following the folder's branch ----

/// A commits, switch to B, B commits, one file left edited. Returns both tips.
fn split_mid_task(wt: &Worktree) -> (String, String) {
    let tip_a = commit(&wt.path, "a.txt", "part one\n");
    git(&wt.path, &["checkout", "-q", "-b", B]);
    let tip_b = commit(&wt.path, "b.txt", "part two\n");
    std::fs::write(wt.path.join("app.py"), "edited\n").unwrap();
    (tip_a, tip_b)
}

fn assert_back_on_b(repo: &Path, wt: &Worktree, tip_a: &str, tip_b: &str) {
    assert!(wt.path.exists(), "the folder is back");
    assert_eq!(on_branch(&wt.path), B, "the folder is back on B");
    assert_eq!(git(&wt.path, &["rev-parse", "HEAD"]), tip_b, "B's commits are there, and nothing else");
    assert_eq!(std::fs::read_to_string(wt.path.join("b.txt")).unwrap(), "part two\n");
    assert_eq!(git(&wt.path, &["status", "--porcelain"]), " M app.py", "only the edited file shows");
    assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "edited\n");
    assert_eq!(git(repo, &["rev-parse", &wt.branch]), tip_a, "A is unchanged");
    assert_eq!(git(repo, &["rev-parse", B]), tip_b, "B is unchanged");
}

#[tokio::test]
async fn r1_put_away_mid_split_task_comes_back_on_the_new_branch() {
    let (rig, _gh, repo, wt) = chat(Duration::ZERO).await;
    let (tip_a, tip_b) = split_mid_task(&wt);
    end_turn(&rig).await;

    close(&rig).await;
    cleanup(&rig).await;
    assert!(!wt.path.exists(), "the idle folder is put away");

    rig.send_at(&repo, "auto", "back to it").await;
    assert_back_on_b(&repo, &wt, &tip_a, &tip_b);
}

#[tokio::test]
async fn r1b_put_away_right_after_the_switch_comes_back_on_the_new_branch() {
    let (rig, _gh, repo, wt) = chat(LONG).await;
    let (tip_a, tip_b) = split_mid_task(&wt);

    // Mid-turn: no turn has ended since the switch.
    rig.manager.delete("s1").await.unwrap();
    assert!(!wt.path.exists(), "the folder is put away with the trashed chat");
    rig.manager.restore_chat("s1").unwrap();

    rig.send_at(&repo, "auto", "back to it").await;
    assert_back_on_b(&repo, &wt, &tip_a, &tip_b);
}

#[tokio::test]
async fn r1c_fresh_branch_with_no_commits_is_kept() {
    let (rig, _gh, repo, wt) = chat(LONG).await;
    git(&wt.path, &["checkout", "-q", "-b", "fresh", "main"]);
    end_turn(&rig).await;

    close(&rig).await;
    for _ in 0..3 {
        cleanup(&rig).await;
    }
    assert!(wt.path.exists(), "the folder stays");
    assert!(has_branch(&repo, "fresh"), "the branch stays");
    assert_eq!(on_branch(&wt.path), "fresh");
}

#[tokio::test]
async fn r1d_detached_folder_is_left_alone() {
    let (rig, _gh, _repo, wt) = chat(Duration::ZERO).await;
    commit(&wt.path, "a.txt", "on the branch\n");
    git(&wt.path, &["checkout", "-q", "--detach"]);
    let tip = commit(&wt.path, "loose.txt", "on no branch\n");
    std::fs::write(wt.path.join("app.py"), "edited\n").unwrap();
    end_turn(&rig).await;

    close(&rig).await;
    for _ in 0..2 {
        cleanup(&rig).await;
    }
    assert!(wt.path.exists(), "the folder is not put away or removed");
    assert_eq!(git(&wt.path, &["rev-parse", "HEAD"]), tip, "the loose commit is still there");
    assert_eq!(git(&wt.path, &["status", "--porcelain"]), " M app.py");
    assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "edited\n");
    assert_eq!(std::fs::read_to_string(wt.path.join("loose.txt")).unwrap(), "on no branch\n");
}

// ---- R2: data saved before the upgrade ----

/// A chat with a worktree and one commit, whose one pull request is saved the
/// old way (a `pr` column, no `prs` column, no branch name). Returns the data
/// folder, the repo, the worktree and the fake GitHub, which knows the same
/// pull request. Nothing is open on the data folder when this returns.
async fn old_chat(state: &str) -> (PathBuf, PathBuf, Worktree, Gh) {
    let dir = data_dir();
    let repo = scratch_repo();
    let (wt, tip) = {
        let rig = rig_in(&dir, LONG, LONG, PrLookup::none());
        let wt = rig.into_worktree(&repo).await;
        let tip = commit(&wt.path, "a.txt", "work\n");
        end_turn(&rig).await;
        close(&rig).await;
        (wt, tip)
    };
    let db = rusqlite::Connection::open(dir.join("harness.db")).unwrap();
    db.busy_timeout(Duration::from_secs(5)).unwrap();
    let has_old_column = db.prepare("SELECT pr FROM sessions").is_ok();
    if !has_old_column {
        db.execute("ALTER TABLE sessions ADD COLUMN pr TEXT", []).unwrap();
    }
    let old = serde_json::json!({ "number": 1, "state": state, "url": link(1), "head": tip }).to_string();
    assert_eq!(db.execute("UPDATE sessions SET pr = ?1", [&old]).unwrap(), 1);
    db.execute("ALTER TABLE sessions DROP COLUMN prs", []).unwrap();
    drop(db);

    let gh = Gh::default();
    gh.add(1, state, &wt.branch, &tip);
    (dir, repo, wt, gh)
}

#[tokio::test]
async fn r2_old_chat_squash_merged_is_cleaned_up_after_the_upgrade() {
    let (dir, repo, wt, gh) = old_chat("merged").await;
    let rig = rig_in(&dir, LONG, LONG, gh.lookup());
    assert_eq!(numbers(&rig), [1], "the old pull request is kept by the upgrade");
    assert_eq!(mark(&rig), Some("merged"));

    cleanup(&rig).await;
    assert!(!wt.path.exists(), "the folder is removed");
    assert!(!has_branch(&repo, &wt.branch), "the branch is removed");
    assert_eq!(numbers(&rig), [1]);
    assert_eq!(mark(&rig), Some("merged"), "the mark stays purple");
}

#[tokio::test]
async fn r2b_old_chat_still_open_keeps_its_mark_and_folder() {
    let (dir, repo, wt, gh) = old_chat("open").await;
    let rig = rig_in(&dir, LONG, LONG, gh.lookup());
    assert_eq!(numbers(&rig), [1]);
    assert_eq!(mark(&rig), Some("open"));

    cleanup(&rig).await;
    assert!(wt.path.exists(), "cleanup leaves the folder");
    assert!(has_branch(&repo, &wt.branch));
    assert_eq!(numbers(&rig), [1]);
    assert_eq!(mark(&rig), Some("open"));
}

// ---- R3, R4: noticing new pull requests ----

#[tokio::test]
async fn r3_new_pull_request_during_a_slow_refresh_keeps_both() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    commit(&wt.path, "a.txt", "part one\n");
    opens(&rig, &gh, 1, "draft", &wt.path);
    end_turn(&rig).await;
    assert_eq!(states(&rig), ["draft"]);

    // Pull request 1 is marked ready on GitHub, and GitHub is slow to say so.
    gh.set(1, "open");
    *gh.slow.lock().unwrap() = Some(1);
    rig.manager.0.pr_checked.lock().unwrap().clear();
    rig.turn_ends();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(gh.waiting.load(Ordering::SeqCst), "a refresh of pull request 1 is waiting on GitHub");

    // Meanwhile the next turn opens pull request 2.
    rig.send_at(&repo, "auto", "now part two").await;
    git(&wt.path, &["checkout", "-q", "-b", B]);
    commit(&wt.path, "b.txt", "part two\n");
    opens(&rig, &gh, 2, "open", &wt.path);
    tokio::time::sleep(Duration::from_millis(150)).await;

    *gh.slow.lock().unwrap() = None;
    settle().await;
    assert_eq!(numbers(&rig), [1, 2]);
    assert_eq!(states(&rig), ["open", "open"], "pull request 1 has its refreshed state");

    // Still so once the turn ends.
    end_turn(&rig).await;
    assert_eq!(numbers(&rig), [1, 2]);
}

#[tokio::test]
async fn r3b_two_pull_requests_in_one_turn_are_both_listed_oldest_first() {
    let (rig, gh, _repo, wt) = chat(LONG).await;
    // No pause between them, and the turn ends right after.
    commit(&wt.path, "a.txt", "part one\n");
    opens(&rig, &gh, 1, "open", &wt.path);
    git(&wt.path, &["checkout", "-q", "-b", B]);
    commit(&wt.path, "b.txt", "part two\n");
    opens(&rig, &gh, 2, "open", &wt.path);
    end_turn(&rig).await;
    assert_eq!(numbers(&rig), [1, 2]);

    // Later refreshes don't add copies.
    end_turn(&rig).await;
    close(&rig).await;
    cleanup(&rig).await;
    assert_eq!(numbers(&rig), [1, 2]);
    assert!(rig.saw(|e| matches!(e, AgentEvent::PrsChanged { prs } if prs.len() == 2)) >= 1, "the window was told");
}

#[tokio::test]
async fn r4_already_exists_adds_the_pull_request() {
    let (rig, gh, _repo, wt) = chat(LONG).await;
    let tip = commit(&wt.path, "a.txt", "work\n");
    gh.add(5, "open", &wt.branch, &tip);
    let said = format!(
        "a pull request for branch \"{}\" into branch \"main\" already exists:\n{}",
        wt.branch,
        link(5)
    );
    bash(
        &rig,
        "t1",
        "gh pr create --title x --body y",
        serde_json::json!([{ "type": "text", "text": said }]),
        true,
    );
    // Noticed from the command's output, before the turn ends.
    settle().await;
    assert_eq!(numbers(&rig), [5]);

    end_turn(&rig).await;
    assert_eq!(numbers(&rig), [5]);
    assert_eq!(mark(&rig), Some("open"));
}

// ---- Other behaviour ----

#[tokio::test]
async fn b1_listing_viewing_or_merging_pull_requests_adds_nothing() {
    let (rig, gh, _repo, wt) = chat(LONG).await;
    commit(&wt.path, "a.txt", "work\n");
    // Other people's pull requests, which GitHub knows by link.
    for number in [7, 8, 9] {
        gh.add(number, "open", &format!("someone-else-{number}"), "0000000000000000000000000000000000000000");
    }
    let text = |text: String| serde_json::json!(text);
    bash(&rig, "t1", "gh pr list", text(format!("7\tA fix\tsomeone-else-7\tOPEN\n{}\n{}\n", link(7), link(8))), false);
    bash(&rig, "t2", "gh pr view 8", text(format!("A fix #8\nOpen\n{}\n", link(8))), false);
    bash(&rig, "t3", "gh pr view 8 --json url -q .url", text(format!("{}\n", link(8))), false);
    bash(&rig, "t4", "gh pr merge 9 --squash", text(format!("Merged pull request #9\n{}\n", link(9))), false);
    settle().await;
    assert_eq!(numbers(&rig), [] as [i64; 0]);

    end_turn(&rig).await;
    assert_eq!(numbers(&rig), [] as [i64; 0]);
    assert_eq!(mark(&rig), None);
    assert_eq!(rig.saw(|e| matches!(e, AgentEvent::PrsChanged { .. })), 0);
}

#[tokio::test]
async fn b2_stacked_first_part_merged_keeps_the_folder_and_both_branches() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, true).await;
    close(&rig).await;

    gh.set(1, "merged");
    cleanup(&rig).await;
    assert!(wt.path.exists(), "the folder stays while pull request 2 is open");
    assert_eq!(on_branch(&wt.path), B);
    assert!(has_branch(&repo, &wt.branch), "A stays");
    assert!(has_branch(&repo, B), "B stays");
    assert_eq!(states(&rig), ["merged", "open"]);
    assert_eq!(mark(&rig), Some("open"));
}

#[tokio::test]
async fn b2b_stacked_both_parts_merged_removes_the_folder_and_both_branches() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, true).await;
    close(&rig).await;

    gh.set(1, "merged");
    cleanup(&rig).await;
    assert!(wt.path.exists());
    gh.set(2, "merged");
    cleanup(&rig).await;
    assert!(!wt.path.exists(), "the folder is removed");
    assert!(!has_branch(&repo, &wt.branch), "A is gone");
    assert!(!has_branch(&repo, B), "B is gone");
    assert_eq!(states(&rig), ["merged", "merged"]);
    assert_eq!(mark(&rig), Some("merged"));
}

#[tokio::test]
async fn b3_independent_parts_merged_in_reverse_order_end_the_same() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, false).await;
    // Back to the first part and over to the second again.
    git(&wt.path, &["checkout", "-q", &wt.branch]);
    end_turn(&rig).await;
    git(&wt.path, &["checkout", "-q", B]);
    end_turn(&rig).await;
    close(&rig).await;

    gh.set(2, "merged");
    cleanup(&rig).await;
    assert!(wt.path.exists(), "the folder stays while pull request 1 is open");
    assert!(has_branch(&repo, &wt.branch), "A stays while its pull request is open");
    assert_eq!(mark(&rig), Some("open"));

    gh.set(1, "merged");
    cleanup(&rig).await;
    assert!(!wt.path.exists(), "the folder is removed");
    assert!(!has_branch(&repo, &wt.branch), "A is gone");
    assert!(!has_branch(&repo, B), "B is gone");
    assert_eq!(states(&rig), ["merged", "merged"]);
    assert_eq!(mark(&rig), Some("merged"));
}

#[tokio::test]
async fn b4_one_merged_one_closed_is_purple_and_removed() {
    let (rig, gh, _repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, false).await;
    close(&rig).await;

    // The folder is on B, which landed; A's pull request was closed.
    gh.set(1, "closed");
    gh.set(2, "merged");
    cleanup(&rig).await;
    assert_eq!(states(&rig), ["closed", "merged"]);
    assert_eq!(mark(&rig), Some("merged"));
    assert!(!wt.path.exists(), "the folder is removed");
}

#[tokio::test]
async fn b5_all_closed_is_red() {
    let (rig, gh, _repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, false).await;
    close(&rig).await;

    gh.set(1, "closed");
    gh.set(2, "closed");
    cleanup(&rig).await;
    assert_eq!(states(&rig), ["closed", "closed"]);
    assert_eq!(mark(&rig), Some("closed"));
}

#[tokio::test]
async fn b6_one_merged_one_draft_is_dim_and_keeps_the_folder() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, true).await;
    close(&rig).await;

    gh.set(1, "merged");
    gh.set(2, "draft");
    cleanup(&rig).await;
    assert_eq!(states(&rig), ["merged", "draft"]);
    assert_eq!(mark(&rig), Some("draft"));
    assert!(wt.path.exists(), "the folder stays");
    assert!(has_branch(&repo, B));
}

#[tokio::test]
async fn b7_uncommitted_edits_keep_the_folder_until_they_are_gone() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, true).await;
    std::fs::write(wt.path.join("app.py"), "not saved yet\n").unwrap();
    close(&rig).await;

    gh.set(1, "merged");
    gh.set(2, "merged");
    cleanup(&rig).await;
    cleanup(&rig).await;
    assert!(wt.path.exists(), "the folder stays while it has edits");
    assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "not saved yet\n");
    assert_eq!(mark(&rig), Some("merged"));

    git(&wt.path, &["checkout", "-q", "--", "app.py"]);
    cleanup(&rig).await;
    assert!(!wt.path.exists(), "a later cleanup removes it");
    assert!(!has_branch(&repo, B));
}

#[tokio::test]
async fn b8_chat_without_a_worktree_goes_green_then_purple() {
    let gh = Gh::default();
    let rig = rig_with(LONG, LONG, LONG, gh.lookup());
    let repo = scratch_repo();
    rig.send_at(&repo, "auto", "small fix").await;
    assert!(rig.worktree().is_none());
    git(&repo, &["checkout", "-q", "-b", "small-fix"]);
    commit(&repo, "a.txt", "fix\n");
    opens(&rig, &gh, 1, "open", &repo);
    end_turn(&rig).await;
    assert_eq!(numbers(&rig), [1]);
    assert_eq!(mark(&rig), Some("open"));

    gh.set(1, "merged");
    close(&rig).await;
    cleanup(&rig).await;
    assert_eq!(numbers(&rig), [1]);
    assert_eq!(mark(&rig), Some("merged"), "purple within one cleanup pass");
    assert!(repo.join("a.txt").exists(), "the main folder is untouched");
}

#[tokio::test]
async fn b9_github_unreachable_changes_nothing() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    two_parts(&rig, &gh, &wt, true).await;
    let before = prs(&rig);

    // Both land on GitHub, but the app can't reach it to find out.
    gh.set(1, "merged");
    gh.set(2, "merged");
    gh.down.store(true, Ordering::SeqCst);
    end_turn(&rig).await;
    assert_eq!(prs(&rig), before, "a failed refresh leaves the list alone");

    close(&rig).await;
    cleanup(&rig).await;
    cleanup(&rig).await;
    assert_eq!(prs(&rig), before, "a failed cleanup lookup leaves the list alone");
    assert_eq!(mark(&rig), Some("open"));
    assert!(wt.path.exists(), "the folder is not removed");
    assert!(has_branch(&repo, &wt.branch) && has_branch(&repo, B), "no branch is removed");
}

#[tokio::test]
async fn b10_restart_keeps_the_list_and_the_mark() {
    let gh = Gh::default();
    let dir = data_dir();
    let repo = scratch_repo();
    let before = {
        let rig = rig_in(&dir, LONG, LONG, gh.lookup());
        let wt = rig.into_worktree(&repo).await;
        two_parts(&rig, &gh, &wt, true).await;
        gh.set(1, "merged");
        end_turn(&rig).await;
        close(&rig).await;
        assert_eq!(states(&rig), ["merged", "open"]);
        prs(&rig)
    };

    let rig = rig_in(&dir, LONG, LONG, gh.lookup());
    assert_eq!(prs(&rig), before);
    assert_eq!(mark(&rig), Some("open"));
}

#[tokio::test]
async fn b11_finished_chat_starts_new_work_with_an_empty_list() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    commit(&wt.path, "a.txt", "work\n");
    opens(&rig, &gh, 1, "open", &wt.path);
    end_turn(&rig).await;
    close(&rig).await;
    gh.set(1, "merged");
    cleanup(&rig).await;
    assert!(!wt.path.exists(), "the finished folder is removed");
    assert_eq!(mark(&rig), Some("merged"));

    // New work, in a new worktree.
    rig.send_at(&repo, "auto", "one more thing").await;
    rig.ask("one more thing").await;
    rig.manager.choose_worktree("s1", &rig.card(), "worktree").await.unwrap();
    let next = rig.worktree().expect("a new worktree");
    assert!(next.path.exists());
    assert_ne!(next.branch, wt.branch);
    assert_eq!(numbers(&rig), [] as [i64; 0], "the list starts empty");
    assert_eq!(mark(&rig), None, "the mark is gone");
    end_turn(&rig).await;
    assert_eq!(mark(&rig), None, "and stays gone");

    rig.send_at(&repo, "auto", "ship it").await;
    commit(&next.path, "c.txt", "more\n");
    opens(&rig, &gh, 2, "open", &next.path);
    end_turn(&rig).await;
    assert_eq!(numbers(&rig), [2]);
    assert_eq!(mark(&rig), Some("open"));
}

#[tokio::test]
async fn b12_merged_branch_checked_out_in_the_main_folder_is_not_deleted() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    let (tip_a, _) = two_parts(&rig, &gh, &wt, true).await;
    close(&rig).await;
    git(&repo, &["checkout", "-q", &wt.branch]);

    gh.set(1, "merged");
    gh.set(2, "merged");
    cleanup(&rig).await;
    assert!(has_branch(&repo, &wt.branch), "the branch the main folder is on stays");
    assert_eq!(on_branch(&repo), wt.branch);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), tip_a);
    assert!(!wt.path.exists(), "the folder is removed");
    assert!(!has_branch(&repo, B), "the other branch is deleted");
    assert_eq!(mark(&rig), Some("merged"));
}

#[tokio::test]
async fn b13_branch_with_commits_beyond_its_merged_pull_request_is_not_deleted() {
    let (rig, gh, repo, wt) = chat(LONG).await;
    commit(&wt.path, "a.txt", "part one\n");
    opens(&rig, &gh, 1, "open", &wt.path);
    // More work on A that the pull request never got.
    let extra = commit(&wt.path, "later.txt", "not in the pull request\n");
    git(&wt.path, &["checkout", "-q", "-b", B, "main"]);
    commit(&wt.path, "b.txt", "part two\n");
    opens(&rig, &gh, 2, "open", &wt.path);
    end_turn(&rig).await;
    close(&rig).await;

    gh.set(1, "merged");
    gh.set(2, "merged");
    cleanup(&rig).await;
    assert!(has_branch(&repo, &wt.branch), "A has work beyond its pull request, so it stays");
    assert_eq!(git(&repo, &["rev-parse", &wt.branch]), extra);
    // The rest is cleaned up: the folder was on B, which landed.
    assert!(!wt.path.exists(), "the folder is removed");
    assert!(!has_branch(&repo, B), "B is deleted");
}

// ---- A project folder that was moved or renamed ----

async fn send_in(rig: &Rig, chat: &str, folder: &Path, text: &str, id: &str) {
    rig.manager
        .send_message(chat, crate::agent::CLAUDE, folder.to_path_buf(), None, None, None, text, &[], id, &[])
        .await
        .unwrap();
}

/// Renames a project's folder on disk, as the user would in the Finder.
fn rename(folder: &Path) -> PathBuf {
    let renamed = folder.with_file_name(format!("{}-renamed", folder.file_name().unwrap().to_string_lossy()));
    std::fs::rename(folder, &renamed).unwrap();
    renamed
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[tokio::test]
async fn m1_a_moved_project_is_pointed_at_and_everything_kept_for_it_follows() {
    let rig = rig(LONG);
    let old = scratch_repo();
    // A chat in its own worktree, a chat in the main folder, and a saved doc.
    let wt = rig.into_worktree(&old).await;
    send_in(&rig, "s2", &old, "a second chat", "m2").await;
    let home = rig.manager.home().clone();
    std::fs::create_dir_all(home.project_docs(&text(&old))).unwrap();
    std::fs::write(home.project_docs(&text(&old)).join("plan.md"), "the plan").unwrap();

    // Still where it was: nothing to point at.
    let elsewhere = scratch_repo();
    assert_eq!(
        rig.manager.relocate_project(&text(&old), &text(&elsewhere)).await.unwrap_err(),
        "That project's folder is still there."
    );
    let new = rename(&old);
    assert!(rig.manager.relocate_project(&text(&old), "/no/such/folder").await.unwrap_err().starts_with("Not a folder"));
    assert_eq!(
        rig.manager.relocate_project("/never/a/project", &text(&new)).await.unwrap_err(),
        "That folder isn't one of the projects."
    );

    let mut moved = rig.manager.relocate_project(&text(&old), &text(&new)).await.unwrap();
    moved.sort();
    assert_eq!(moved, ["s1", "s2"]);
    // The chats are the new folder's, and know where it used to be.
    for chat in ["s1", "s2"] {
        let row = rig.store.session(chat).unwrap().unwrap();
        assert_eq!((row.project_path, row.former_paths), (text(&new), vec![text(&old)]));
    }
    assert!(!rig.store.has_project(&text(&old)).unwrap());
    // Nothing runs for the old folder any more.
    assert!(!rig.manager.is_live("s1") && !rig.manager.is_live("s2"));
    // The worktree is linked to the repo where it is now.
    let saved = rig.store.chat_git("s1").unwrap().unwrap().worktree.unwrap();
    assert_eq!(saved.repo_root, new.canonicalize().unwrap());
    assert_eq!(saved.path, wt.path);
    git(&wt.path, &["status", "--short"]);
    // Its docs are under the new folder's name.
    assert_eq!(std::fs::read_to_string(home.project_docs(&text(&new)).join("plan.md")).unwrap(), "the plan");
    assert!(!home.project(&text(&old)).exists());

    // Each chat's agent is told once, with the next message there.
    send_in(&rig, "s2", &new, "carry on", "m3").await;
    send_in(&rig, "s2", &new, "and again", "m4").await;
    let told = rig.told();
    let note = format!(
        "(From the app, not the user: This project's folder moved from {} to {}. Paths under the old folder are \
now under the new one.)",
        text(&old),
        text(&new)
    );
    assert!(told.contains(&format!("say {note}\n\ncarry on")), "{told:?}");
    assert!(told.contains(&"say and again".to_string()));
}

#[tokio::test]
async fn m2_a_chat_started_in_the_moved_folder_brings_the_project_along_by_itself() {
    let rig = rig(LONG);
    let old = scratch_repo();
    send_in(&rig, "s1", &old, "the first chat", "m1").await;
    rig.manager.close("s1");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let new = rename(&old);

    // No pointing: a new chat in the folder at its new place is enough.
    send_in(&rig, "s2", &new, "a chat after the move", "m2").await;
    assert_eq!(rig.store.session("s1").unwrap().unwrap().project_path, text(&new));
    assert!(!rig.store.has_project(&text(&old)).unwrap());
    // The chat that found it says so.
    let said = rig.store.events("s2").unwrap();
    assert!(said.contains(&AgentEvent::ProjectMoved { from: text(&old), to: text(&new) }), "{said:?}");
    // And the new chat, which never knew the old folder, is not told it moved.
    assert_eq!(rig.store.take_pending_note("s2").unwrap(), None);
    assert!(rig.store.take_pending_note("s1").unwrap().is_some());
}

#[tokio::test]
async fn m3_a_folder_is_only_followed_when_it_is_surely_the_same_project() {
    let rig = rig(LONG);
    // Two projects that are clones of one repo, both gone: which one is it?
    let first = scratch_repo();
    let second = first.with_file_name(format!("{}-clone", first.file_name().unwrap().to_string_lossy()));
    let cloned = Command::new("git").arg("clone").arg("-q").arg(&first).arg(&second).output().unwrap();
    assert!(cloned.status.success());
    send_in(&rig, "a", &first, "in the first", "m1").await;
    send_in(&rig, "b", &second, "in the clone", "m2").await;
    let third = second.with_file_name(format!("{}-third", first.file_name().unwrap().to_string_lossy()));
    let cloned = Command::new("git").arg("clone").arg("-q").arg(&first).arg(&third).output().unwrap();
    assert!(cloned.status.success());
    std::fs::remove_dir_all(&first).unwrap();
    std::fs::remove_dir_all(&second).unwrap();
    send_in(&rig, "c", &third, "in a third copy", "m3").await;
    assert_eq!(rig.store.session("a").unwrap().unwrap().project_path, text(&first));
    assert_eq!(rig.store.session("b").unwrap().unwrap().project_path, text(&second));

    // A folder that isn't the same repo is its own project.
    let other = scratch_repo();
    send_in(&rig, "d", &other, "somewhere else", "m4").await;
    assert_eq!(rig.store.session("d").unwrap().unwrap().project_path, text(&other));
    assert_eq!(rig.store.session("a").unwrap().unwrap().project_path, text(&first));
}

#[tokio::test]
async fn m4_moving_onto_a_project_that_is_already_there_makes_them_one() {
    let rig = rig(LONG);
    let old = scratch_repo();
    send_in(&rig, "s1", &old, "before the move", "m1").await;
    rig.manager.close("s1");
    tokio::time::sleep(Duration::from_millis(20)).await;
    // Not a repo the app can recognise it by: a plain copy made elsewhere,
    // already used for a chat before the old project is pointed at it.
    let new = std::env::temp_dir().join(format!("harness-plain-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&new).unwrap();
    send_in(&rig, "s2", &new, "already here", "m2").await;
    std::fs::remove_dir_all(&old).unwrap();

    rig.manager.relocate_project(&text(&old), &text(&new)).await.unwrap();
    let sidebar = rig.store.sidebar().unwrap();
    let project = sidebar.iter().find(|p| p.path == text(&new)).expect("the project");
    let mut chats: Vec<&str> = project.sessions.iter().map(|s| s.id.as_str()).collect();
    chats.sort();
    assert_eq!(chats, ["s1", "s2"]);
    assert!(sidebar.iter().all(|p| p.path != text(&old)));
    // Only the chat that moved is told.
    assert!(rig.store.take_pending_note("s1").unwrap().is_some());
    assert_eq!(rig.store.take_pending_note("s2").unwrap(), None);
}

#[tokio::test]
async fn m6_a_put_away_worktree_is_kept_while_its_project_is_missing() {
    let (rig, _gh, repo, wt) = chat(Duration::ZERO).await;
    let tip = commit(&wt.path, "work.txt", "unmerged work\n");
    end_turn(&rig).await;
    close(&rig).await;
    cleanup(&rig).await;
    assert!(!wt.path.exists(), "the idle folder is put away");

    // The repo is renamed, and the cleanup runs before anyone points at it.
    let new = rename(&repo);
    cleanup(&rig).await;
    let saved = rig.store.chat_git("s1").unwrap().unwrap().worktree;
    assert_eq!(saved.map(|saved| saved.branch), Some(wt.branch.clone()), "the chat still knows its branch");

    rig.manager.relocate_project(&text(&repo), &text(&new)).await.unwrap();
    rig.send_at(&new, "auto", "back to it").await;
    assert!(wt.path.exists(), "the folder is back");
    assert_eq!(git(&wt.path, &["rev-parse", "HEAD"]), tip, "with its work");
}

#[test]
fn m5_a_folder_on_a_drive_that_isnt_connected_is_not_gone() {
    assert!(!folder_gone(&std::env::temp_dir()));
    assert!(folder_gone(&std::env::temp_dir().join(format!("harness-none-{}", uuid::Uuid::new_v4()))));
    if !cfg!(windows) {
        assert!(!folder_gone(Path::new("/Volumes/harness-no-such-drive/code/app")));
    }
}
