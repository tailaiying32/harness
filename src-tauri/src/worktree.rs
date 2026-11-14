//! Git worktrees for chats: make one, put it away (the folder goes, uncommitted
//! edits are kept in a hidden ref), bring it back, and tell when its branch is
//! merged. Everything here is plain git run as a program; no AI is involved.
//! See PLAN.md, "Worktrees".

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::locate;

/// A chat's worktree, as saved with the chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Worktree {
    /// The main repo's top folder.
    pub repo_root: PathBuf,
    /// The chat's folder inside the repo ("" when it is the top folder).
    pub subpath: String,
    /// The worktree's own folder.
    pub path: PathBuf,
    pub branch: String,
    /// The commit the worktree started from.
    pub base_commit: String,
    /// The branch it started from, when there was one.
    pub base_branch: Option<String>,
}

impl Worktree {
    /// Where the chat's agent runs: the same subfolder, inside the worktree.
    pub fn work_path(&self) -> PathBuf {
        if self.subpath.is_empty() {
            self.path.clone()
        } else {
            self.path.join(&self.subpath)
        }
    }
}

/// A pull request for a worktree's branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrInfo {
    pub number: i64,
    /// "open", "draft", "merged" or "closed".
    pub state: String,
    pub url: String,
    /// The PR's head commit.
    pub head: String,
}

/// Whether a worktree's work has landed, and how.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Merged {
    No,
    /// Its tip is part of a main branch.
    ByGit,
    /// Its pull request was merged with the same tip (covers squash merges,
    /// which git alone can't see).
    ByPr,
}

fn git_cmd(dir: &Path) -> Command {
    let mut cmd = Command::new(locate::git());
    cmd.arg("-C").arg(dir).env("GIT_TERMINAL_PROMPT", "0");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

fn run(mut cmd: Command) -> Result<String, String> {
    let out = cmd.output().map_err(|e| format!("Could not run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_string())
    } else {
        Err(format!("git: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn git<I, S>(dir: &Path, args: I) -> Result<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = git_cmd(dir);
    cmd.args(args);
    run(cmd)
}

/// Whether `rev` names an existing object; its full ID if so.
fn resolve(dir: &Path, rev: &str) -> Option<String> {
    git(dir, ["rev-parse", "--verify", "-q", rev]).ok().filter(|s| !s.is_empty())
}

/// Where a chat's uncommitted edits wait while its folder is put away.
fn saved_ref(chat_id: &str) -> String {
    format!("refs/harness/saved/{chat_id}")
}

/// The repo a folder belongs to: its top folder and the folder's place inside
/// it. None for a folder outside any repo.
pub fn repo_of(folder: &Path) -> Option<(PathBuf, String)> {
    let top = git(folder, ["rev-parse", "--show-toplevel"]).ok()?;
    let root = PathBuf::from(top).canonicalize().ok()?;
    let folder = folder.canonicalize().ok()?;
    let sub = folder.strip_prefix(&root).ok()?.to_string_lossy().into_owned();
    Some((root, sub))
}

/// The folder's current branch (None when detached) and commit. None outside a
/// repo or in one with no commits yet.
pub fn head_of(folder: &Path) -> Option<(Option<String>, String)> {
    let commit = resolve(folder, "HEAD")?;
    let branch = git(folder, ["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty());
    Some((branch, commit))
}

/// A chat title as a branch-name piece: lowercase words joined by dashes.
fn slug(title: &str, max: usize) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= max {
            break;
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "chat".to_string() } else { out }
}

/// Makes a worktree for a chat on a new branch, then copies in the main
/// folder's ignored files (builds, packages, secrets).
///
/// It starts from the latest commit of `start_branch`, the branch the main
/// folder was on when the chat began, so another chat switching the main
/// folder's branch since then doesn't change where this one starts. If that
/// branch is gone: `start_commit`, then the main folder's current commit.
pub fn create(
    repo_root: &Path,
    subpath: &str,
    title: &str,
    start_branch: Option<&str>,
    start_commit: Option<&str>,
    worktrees_dir: &Path,
) -> Result<Worktree, String> {
    let from_branch = start_branch.and_then(|b| resolve(repo_root, &format!("refs/heads/{b}")).map(|c| (b, c)));
    let (base_branch, base_commit) = match from_branch {
        Some((b, c)) => (Some(b.to_string()), c),
        None => {
            let commit = start_commit
                .and_then(|c| resolve(repo_root, &format!("{c}^{{commit}}")))
                .or_else(|| resolve(repo_root, "HEAD"))
                .ok_or("This repo has no commits yet")?;
            (None, commit)
        }
    };
    let id: String = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());
    let path = worktrees_dir.join(format!("{repo_name}-{id}"));
    let branch = format!("harness/{}-{id}", slug(title, 30));
    std::fs::create_dir_all(worktrees_dir).map_err(|e| format!("Can't make the worktrees folder: {e}"))?;
    git(
        repo_root,
        [
            OsStr::new("worktree"),
            OsStr::new("add"),
            OsStr::new("-b"),
            OsStr::new(&branch),
            path.as_os_str(),
            OsStr::new(&base_commit),
        ],
    )?;
    copy_ignored(repo_root, &path);
    Ok(Worktree {
        repo_root: repo_root.to_path_buf(),
        subpath: subpath.to_string(),
        path,
        branch,
        base_commit,
        base_branch,
    })
}

/// Copies every file git ignores in `main` into the worktree: build output,
/// packages, and secrets like `.env`. On a Mac they are copy-on-write clones,
/// which take no space until changed. Where cloning fails (another drive, or
/// not a Mac), single files are copied plainly and folders are skipped, as
/// they are usually large and can be rebuilt.
///
/// Never copied: anything holding a nested repo or worktree (a `.git` in the
/// folder or one level down), since a copy of one would act on the original.
/// Best effort: a file that can't be copied is skipped.
pub fn copy_ignored(main: &Path, wt: &Path) {
    let Ok(list) = git(
        main,
        ["ls-files", "--others", "--ignored", "--exclude-standard", "--directory", "-z"],
    ) else {
        return;
    };
    for entry in list.split('\0').filter(|e| !e.is_empty()) {
        let is_dir = entry.ends_with('/');
        let rel = entry.trim_end_matches('/');
        let src = main.join(rel);
        let dst = wt.join(rel);
        if dst.symlink_metadata().is_ok() {
            continue;
        }
        if is_dir && holds_a_repo(&src) {
            continue;
        }
        if let Some(parent) = dst.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        if !clone_path(&src, &dst) && !is_dir {
            let _ = std::fs::copy(&src, &dst);
        }
    }
}

fn holds_a_repo(dir: &Path) -> bool {
    if dir.join(".git").exists() {
        return true;
    }
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().any(|e| e.path().join(".git").exists()))
        .unwrap_or(false)
}

/// A copy-on-write clone of a file or a whole folder. False if the system
/// can't (another drive, not APFS, not a Mac).
#[cfg(target_os = "macos")]
fn clone_path(src: &Path, dst: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    /// Copy a link as a link instead of what it points to.
    const CLONE_NOFOLLOW: u32 = 0x0001;
    let (Ok(src), Ok(dst)) = (
        CString::new(src.as_os_str().as_bytes()),
        CString::new(dst.as_os_str().as_bytes()),
    ) else {
        return false;
    };
    // SAFETY: two valid NUL-terminated paths; the call only reads them.
    unsafe { libc::clonefile(src.as_ptr(), dst.as_ptr(), CLONE_NOFOLLOW) == 0 }
}

#[cfg(not(target_os = "macos"))]
fn clone_path(_src: &Path, _dst: &Path) -> bool {
    false
}

/// Saves a worktree's uncommitted edits (changed, new and deleted files; not
/// ignored ones) under a hidden ref, without touching the branch or the
/// worktree's own staging area. Returns whether there was anything to save.
pub fn save_edits(wt: &Path, chat_id: &str) -> Result<bool, String> {
    let git_dir = PathBuf::from(git(wt, ["rev-parse", "--absolute-git-dir"])?);
    let index = git_dir.join("harness-index");
    struct Remove<'a>(&'a Path);
    impl Drop for Remove<'_> {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0);
        }
    }
    let _cleanup = Remove(&index);
    let with_index = |args: &[&str]| {
        let mut cmd = git_cmd(wt);
        cmd.env("GIT_INDEX_FILE", &index).args(args);
        run(cmd)
    };
    with_index(&["read-tree", "HEAD"])?;
    with_index(&["add", "-A"])?;
    let tree = with_index(&["write-tree"])?;
    if tree == git(wt, ["rev-parse", "HEAD^{tree}"])? {
        return Ok(false);
    }
    let commit = git(
        wt,
        [
            "-c",
            "user.name=Harness",
            "-c",
            "user.email=harness@localhost",
            "commit-tree",
            &tree,
            "-p",
            "HEAD",
            "-m",
            "Harness: saved edits",
        ],
    )?;
    git(wt, ["update-ref", &saved_ref(chat_id), &commit])?;
    Ok(true)
}

/// Removes the worktree's folder after saving its uncommitted edits. The
/// branch stays. If saving fails, nothing is removed.
pub fn put_away(wt: &Worktree, chat_id: &str) -> Result<(), String> {
    if wt.path.join(".git").exists() {
        save_edits(&wt.path, chat_id)?;
        remove_folder(wt)?;
    }
    let _ = git(&wt.repo_root, ["worktree", "prune"]);
    Ok(())
}

/// Removes the worktree's folder without saving anything: only for one whose
/// work is merged and has no uncommitted edits.
pub fn remove_folder(wt: &Worktree) -> Result<(), String> {
    if wt.path.exists() {
        git(
            &wt.repo_root,
            [OsStr::new("worktree"), OsStr::new("remove"), OsStr::new("--force"), wt.path.as_os_str()],
        )?;
    }
    let _ = git(&wt.repo_root, ["worktree", "prune"]);
    Ok(())
}

/// Brings a put-away worktree back at the same place, with its saved edits as
/// uncommitted edits again and fresh copies of the main folder's ignored files.
pub fn restore(wt: &Worktree, chat_id: &str) -> Result<(), String> {
    let _ = git(&wt.repo_root, ["worktree", "prune"]);
    if wt.path.join(".git").exists() {
        return Ok(()); // already there
    }
    if wt.path.exists() {
        // Only an empty leftover folder is cleared; anything else is left alone.
        std::fs::remove_dir(&wt.path)
            .map_err(|_| format!("Something is in the way at {}", wt.path.display()))?;
    }
    if resolve(&wt.repo_root, &format!("refs/heads/{}", wt.branch)).is_none() {
        return Err(format!("This chat's branch {} no longer exists", wt.branch));
    }
    git(
        &wt.repo_root,
        [OsStr::new("worktree"), OsStr::new("add"), wt.path.as_os_str(), OsStr::new(&wt.branch)],
    )?;
    let saved = saved_ref(chat_id);
    if resolve(&wt.repo_root, &saved).is_some() {
        // No-overlay: files deleted before saving stay deleted.
        git(&wt.path, ["restore", "--source", &saved, "--worktree", "--", "."])?;
        git(&wt.repo_root, ["update-ref", "-d", &saved])?;
    }
    copy_ignored(&wt.repo_root, &wt.path);
    Ok(())
}

/// The branches a worktree's work counts as merged into: the one it started
/// from, the remote's default, and main or master.
fn merge_targets(wt: &Worktree) -> Vec<String> {
    let mut targets = Vec::new();
    if let Some(b) = &wt.base_branch {
        targets.push(format!("refs/heads/{b}"));
    }
    if let Ok(remote_default) = git(&wt.repo_root, ["symbolic-ref", "-q", "refs/remotes/origin/HEAD"]) {
        targets.push(remote_default);
    }
    targets.push("refs/heads/main".to_string());
    targets.push("refs/heads/master".to_string());
    let own = format!("refs/heads/{}", wt.branch);
    targets.retain(|t| *t != own);
    targets.dedup();
    targets
}

/// Whether a put-away worktree has uncommitted edits waiting.
pub fn has_saved_edits(repo_root: &Path, chat_id: &str) -> bool {
    resolve(repo_root, &saved_ref(chat_id)).is_some()
}

/// Whether the worktree's work has landed. A branch with no commits of its
/// own never counts, nor does one with uncommitted edits (in its folder, or
/// saved while put away for `chat_id`).
pub fn is_merged(wt: &Worktree, pr: Option<&PrInfo>, chat_id: &str) -> Result<Merged, String> {
    let tip = resolve(&wt.repo_root, &format!("refs/heads/{}", wt.branch))
        .ok_or_else(|| format!("The branch {} no longer exists", wt.branch))?;
    if tip == wt.base_commit {
        return Ok(Merged::No);
    }
    if wt.path.join(".git").exists() && !git(&wt.path, ["status", "--porcelain"])?.is_empty() {
        return Ok(Merged::No);
    }
    if has_saved_edits(&wt.repo_root, chat_id) {
        return Ok(Merged::No);
    }
    for target in merge_targets(wt) {
        if resolve(&wt.repo_root, &target).is_some()
            && git(&wt.repo_root, ["merge-base", "--is-ancestor", &tip, &target]).is_ok()
        {
            return Ok(Merged::ByGit);
        }
    }
    if pr.is_some_and(|pr| pr.state == "merged" && pr.head == tip) {
        return Ok(Merged::ByPr);
    }
    Ok(Merged::No)
}

/// Deletes a worktree's branch and its saved edits. Only called once the work
/// has landed (checked by `is_merged`) or its time in the trash is up, so the
/// force delete is intended: git's own merged check looks only at the main
/// folder's current branch, and can't see squash merges.
pub fn drop_branch(repo_root: &Path, branch: &str, chat_id: &str) {
    let _ = git(repo_root, ["branch", "-D", branch]);
    let _ = git(repo_root, ["update-ref", "-d", &saved_ref(chat_id)]);
    let _ = git(repo_root, ["worktree", "prune"]);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch repo with one commit on `main`.
    struct Repo {
        root: PathBuf,
        dir: PathBuf,
    }

    impl Repo {
        fn new() -> Self {
            let dir = std::env::temp_dir()
                .join(format!("harness-wt-{}", uuid::Uuid::new_v4().simple()))
                .canonicalize_or_create();
            let root = dir.join("app");
            std::fs::create_dir_all(&root).unwrap();
            let repo = Repo { root, dir };
            repo.git(&["init", "-q", "-b", "main"]);
            repo.git(&["config", "user.name", "t"]);
            repo.git(&["config", "user.email", "t@t"]);
            repo.write("app.py", "one\n");
            repo.write("old.txt", "old\n");
            repo.write(".gitignore", ".env\ntarget/\n");
            repo.git(&["add", "-A"]);
            repo.git(&["commit", "-qm", "init"]);
            repo
        }
        fn git(&self, args: &[&str]) -> String {
            git(&self.root, args).unwrap()
        }
        fn write(&self, rel: &str, text: &str) {
            let path = self.root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        fn worktrees(&self) -> PathBuf {
            self.dir.join("worktrees")
        }
        fn create(&self, start_branch: Option<&str>) -> Worktree {
            let (_, commit) = head_of(&self.root).unwrap();
            create(&self.root, "", "Fix the login bug!", start_branch, Some(&commit), &self.worktrees()).unwrap()
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    trait CanonicalizeOrCreate {
        fn canonicalize_or_create(self) -> PathBuf;
    }
    impl CanonicalizeOrCreate for PathBuf {
        fn canonicalize_or_create(self) -> PathBuf {
            std::fs::create_dir_all(&self).unwrap();
            self.canonicalize().unwrap()
        }
    }

    fn commit_in(dir: &Path, file: &str, text: &str) {
        std::fs::write(dir.join(file), text).unwrap();
        git(dir, ["add", "-A"]).unwrap();
        git(dir, ["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "work"]).unwrap();
    }

    #[test]
    fn slugs_are_short_lowercase_words() {
        assert_eq!(slug("Fix the login bug!", 30), "fix-the-login-bug");
        assert_eq!(slug("  ***  ", 30), "chat");
        assert_eq!(slug("a very long title that goes on and on", 10), "a-very-lon");
    }

    #[test]
    fn repo_of_finds_the_top_and_the_subfolder() {
        let repo = Repo::new();
        repo.write("web/src/x.ts", "x");
        let (root, sub) = repo_of(&repo.root.join("web/src")).unwrap();
        assert_eq!((root, sub.as_str()), (repo.root.clone(), "web/src"));
        assert!(repo_of(&repo.dir).is_none());
    }

    #[test]
    fn a_worktree_starts_from_the_chats_branch_even_after_a_switch() {
        let repo = Repo::new();
        // The chat began on main; another chat then switched the folder.
        repo.git(&["checkout", "-qb", "other"]);
        commit_in(&repo.root, "app.py", "other\n");
        // Main moved on since the chat began, too.
        repo.git(&["checkout", "-q", "main"]);
        commit_in(&repo.root, "app.py", "two\n");
        let main_tip = repo.git(&["rev-parse", "main"]);
        repo.git(&["checkout", "-q", "other"]);

        let wt = repo.create(Some("main"));
        assert_eq!(wt.base_commit, main_tip);
        assert_eq!(wt.base_branch.as_deref(), Some("main"));
        assert!(wt.branch.starts_with("harness/fix-the-login-bug-"));
        assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "two\n");

        // A deleted start branch falls back to the commit.
        let wt2 = repo.create(Some("gone"));
        assert_eq!(wt2.base_branch, None);
    }

    #[test]
    fn ignored_files_are_copied_but_nested_repos_are_not() {
        let repo = Repo::new();
        repo.write(".env", "SECRET=1\n");
        repo.write("target/debug/big.bin", "build output");
        // An ignored folder holding someone's worktree.
        repo.write(".gitignore", ".env\ntarget/\nnest/\n");
        repo.git(&["commit", "-qam", "ignore nest"]);
        repo.write("nest/wt/.git", "gitdir: /elsewhere");
        repo.write("nest/wt/file", "x");

        let wt = repo.create(Some("main"));
        assert_eq!(std::fs::read_to_string(wt.path.join(".env")).unwrap(), "SECRET=1\n");
        assert_eq!(
            std::fs::read_to_string(wt.path.join("target/debug/big.bin")).unwrap(),
            "build output"
        );
        assert!(!wt.path.join("nest").exists());
    }

    #[test]
    fn put_away_and_restore_keep_uncommitted_edits() {
        let repo = Repo::new();
        let wt = repo.create(Some("main"));
        std::fs::write(wt.path.join("app.py"), "changed\n").unwrap();
        std::fs::write(wt.path.join("new.txt"), "new\n").unwrap();
        std::fs::remove_file(wt.path.join("old.txt")).unwrap();
        repo.write(".env", "SECRET=2\n");
        let tip = repo.git(&["rev-parse", &wt.branch]);

        put_away(&wt, "chat1").unwrap();
        assert!(!wt.path.exists());
        // The branch is untouched; the edits wait in the hidden ref.
        assert_eq!(repo.git(&["rev-parse", &wt.branch]), tip);
        assert!(resolve(&repo.root, &saved_ref("chat1")).is_some());

        restore(&wt, "chat1").unwrap();
        assert_eq!(std::fs::read_to_string(wt.path.join("app.py")).unwrap(), "changed\n");
        assert_eq!(std::fs::read_to_string(wt.path.join("new.txt")).unwrap(), "new\n");
        assert!(!wt.path.join("old.txt").exists());
        assert_eq!(std::fs::read_to_string(wt.path.join(".env")).unwrap(), "SECRET=2\n");
        // Uncommitted again, and the hidden ref is gone.
        assert_eq!(repo.git(&["rev-parse", &wt.branch]), tip);
        assert!(!git(&wt.path, ["status", "--porcelain"]).unwrap().is_empty());
        assert!(resolve(&repo.root, &saved_ref("chat1")).is_none());
    }

    #[test]
    fn a_clean_worktree_is_put_away_without_a_saved_ref() {
        let repo = Repo::new();
        let wt = repo.create(Some("main"));
        put_away(&wt, "chat1").unwrap();
        assert!(resolve(&repo.root, &saved_ref("chat1")).is_none());
        restore(&wt, "chat1").unwrap();
        assert!(wt.path.join("app.py").exists());
    }

    #[test]
    fn restore_fails_cleanly_when_the_branch_is_gone() {
        let repo = Repo::new();
        let wt = repo.create(Some("main"));
        put_away(&wt, "chat1").unwrap();
        drop_branch(&repo.root, &wt.branch, "chat1");
        assert!(restore(&wt, "chat1").unwrap_err().contains("no longer exists"));
    }

    fn pr(state: &str, head: &str) -> PrInfo {
        PrInfo {
            number: 1,
            state: state.into(),
            url: "u".into(),
            head: head.into(),
        }
    }

    #[test]
    fn merged_means_own_commits_that_landed() {
        let repo = Repo::new();
        let wt = repo.create(Some("main"));
        // No commits of its own: not merged, even though main contains it.
        assert_eq!(is_merged(&wt, None, "chat1").unwrap(), Merged::No);

        commit_in(&wt.path, "app.py", "work\n");
        assert_eq!(is_merged(&wt, None, "chat1").unwrap(), Merged::No);
        let tip = repo.git(&["rev-parse", &wt.branch]);

        // A squash-merged PR with the same head counts; a later commit doesn't.
        assert_eq!(is_merged(&wt, Some(&pr("merged", &tip)), "chat1").unwrap(), Merged::ByPr);
        assert_eq!(is_merged(&wt, Some(&pr("merged", "abc")), "chat1").unwrap(), Merged::No);
        assert_eq!(is_merged(&wt, Some(&pr("open", &tip)), "chat1").unwrap(), Merged::No);

        // Uncommitted edits keep it from counting.
        std::fs::write(wt.path.join("app.py"), "more\n").unwrap();
        assert_eq!(is_merged(&wt, Some(&pr("merged", &tip)), "chat1").unwrap(), Merged::No);
        git(&wt.path, ["checkout", "--", "."]).unwrap();

        // Merged into main with git.
        repo.git(&["merge", "-q", "--no-edit", &wt.branch]);
        assert_eq!(is_merged(&wt, None, "chat1").unwrap(), Merged::ByGit);

        // Put away with uncommitted edits saved: not done, even though merged.
        std::fs::write(wt.path.join("left.txt"), "unfinished\n").unwrap();
        put_away(&wt, "chat1").unwrap();
        assert_eq!(is_merged(&wt, None, "chat1").unwrap(), Merged::No);
        assert_eq!(is_merged(&wt, None, "other-chat").unwrap(), Merged::ByGit);
        restore(&wt, "chat1").unwrap();
        std::fs::remove_file(wt.path.join("left.txt")).unwrap();

        // And once its folder is gone too.
        remove_folder(&wt).unwrap();
        assert_eq!(is_merged(&wt, None, "chat1").unwrap(), Merged::ByGit);
        drop_branch(&repo.root, &wt.branch, "chat1");
        assert!(resolve(&repo.root, &format!("refs/heads/{}", wt.branch)).is_none());
    }

    #[test]
    fn a_subfolder_chat_works_in_the_same_subfolder() {
        let repo = Repo::new();
        repo.write("web/a.ts", "a");
        repo.git(&["add", "-A"]);
        repo.git(&["commit", "-qm", "web"]);
        let (root, sub) = repo_of(&repo.root.join("web")).unwrap();
        let wt = create(&root, &sub, "t", Some("main"), None, &repo.worktrees()).unwrap();
        assert!(wt.work_path().join("a.ts").exists());
    }
}
