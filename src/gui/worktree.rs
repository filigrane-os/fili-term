//! Git worktrees for tabs: a tab in a new worktree of the repository (a branch checked out in its own folder beside the
//! repository), the worktrees of a repository to open one in a tab, and removing one. git runs as a program with its
//! arguments, never through a shell, and its own error message is what the user reads.
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, PartialEq, Clone)]
pub struct Worktree {
    pub path: String,
    /// Branch checked out (`refs/heads/` removed), None for a detached head
    pub branch: Option<String>,
    pub head: String,
    /// The first one `git worktree list` gives: the repository itself
    pub main: bool,
}

/// Parse `git worktree list --porcelain`: blocks separated by an empty line, the first one is the main working tree
pub fn parse_porcelain(text: &str) -> Vec<Worktree> {
    let mut out: Vec<Worktree> = Vec::new();
    for block in text.split("\n\n") {
        let mut path = None;
        let (mut branch, mut head, mut bare) = (None, String::new(), false);
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(p.to_string());
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                head = h.chars().take(8).collect();
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = Some(b.strip_prefix("refs/heads/").unwrap_or(b).to_string());
            } else if line == "bare" {
                bare = true;
            }
        }
        if let Some(path) = path.filter(|_| !bare) {
            let main = out.is_empty();
            out.push(Worktree { path, branch, head, main });
        }
    }
    out
}

/// Run git in a folder; Ok(stdout), or Err(git's own message)
fn git(dir: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        // The reason is on the "fatal:" or "error:" lines; the progress lines before them ("Preparing worktree…") are noise
        let err = String::from_utf8_lossy(&out.stderr);
        let lines: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let reasons: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with("fatal:") || l.starts_with("error:")).collect();
        let msg = if reasons.is_empty() { lines } else { reasons }
            .iter()
            .map(|l| l.trim_start_matches("fatal: ").trim_start_matches("error: "))
            .collect::<Vec<_>>()
            .join(" ");
        Err(if msg.is_empty() { format!("git {} failed", args.first().unwrap_or(&"")) } else { msg })
    }
}

/// The worktrees of the repository holding `dir` (the main one first); empty outside a repository
pub fn list(dir: &str) -> Vec<Worktree> {
    if dir.is_empty() || !Path::new(dir).is_dir() {
        return Vec::new();
    }
    git(dir, &["worktree", "list", "--porcelain"]).map(|t| parse_porcelain(&t)).unwrap_or_default()
}

/// A branch name as a folder suffix: `feature/login` gives `feature-login`
pub fn folder_suffix(branch: &str) -> String {
    let s: String = branch.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' }).collect();
    s.trim_matches(|c| c == '-' || c == '.').to_string()
}

/// Where the worktree of a branch goes: beside the repository, `<repo>-<branch>`
pub fn target_path(main: &str, branch: &str) -> PathBuf {
    let main = Path::new(main);
    let name = main.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "repo".into());
    main.parent().unwrap_or(Path::new("/")).join(format!("{name}-{}", folder_suffix(branch)))
}

/// Add a worktree for a branch next to the repository holding `dir`: the branch when it exists, else a new one from the
/// current head. Returns the folder of the worktree.
pub fn add(dir: &str, branch: &str) -> Result<String, String> {
    let branch = branch.trim();
    if branch.is_empty() || branch.starts_with('-') || folder_suffix(branch).is_empty() {
        return Err(format!("Not a valid branch name: {branch}"));
    }
    let main = list(dir).into_iter().next().ok_or("Not in a git repository")?.path;
    git(&main, &["check-ref-format", "--branch", branch]).map_err(|_| format!("Not a valid branch name: {branch}"))?;
    let path = target_path(&main, branch);
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    let path = path.to_string_lossy().into_owned();
    let exists = git(&main, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok();
    if exists {
        git(&main, &["worktree", "add", "--", &path, branch])?;
    } else {
        git(&main, &["worktree", "add", "-b", branch, "--", &path])?;
    }
    Ok(path)
}

/// Remove a linked worktree of the repository holding `dir`. Never forced: git refuses one with changes and says so.
pub fn remove(dir: &str, path: &str) -> Result<(), String> {
    let all = list(dir);
    let found = all.iter().find(|w| w.path == path).ok_or("Not a worktree of this repository")?;
    if found.main {
        return Err("This is the main working tree".into());
    }
    let main = &all[0].path;
    git(main, &["worktree", "remove", "--", path]).map(|_| ())
}

/// The worktrees as JSON for the QML: [{path, branch, head, main}]
pub fn list_json(dir: &str) -> String {
    let q = |s: &str| super::terminal::json_string(s);
    let items: Vec<String> = list(dir)
        .iter()
        .map(|w| format!("{{\"path\":{},\"branch\":{},\"head\":{},\"main\":{}}}", q(&w.path), q(w.branch.as_deref().unwrap_or("")), q(&w.head), w.main))
        .collect();
    format!("[{}]", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_SYSTEM", "/dev/null")
            .stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn porcelain() {
        let text = "worktree /w/repo\nHEAD 0123456789abcdef\nbranch refs/heads/main\n\nworktree /w/repo-feat\nHEAD fedcba9876543210\nbranch refs/heads/feature/x\n\nworktree /w/detached\nHEAD 1111111111111111\ndetached\n\nworktree /w/bare.git\nbare\n";
        let l = parse_porcelain(text);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].path.as_str(), l[0].branch.as_deref(), l[0].main, l[0].head.as_str()), ("/w/repo", Some("main"), true, "01234567"));
        assert_eq!((l[1].branch.as_deref(), l[1].main), (Some("feature/x"), false));
        assert_eq!(l[2].branch, None);
    }

    #[test]
    fn folders() {
        assert_eq!(folder_suffix("feature/login page"), "feature-login-page");
        assert_eq!(folder_suffix("../x"), "x");
        assert_eq!(target_path("/home/u/code/app", "fix/a"), PathBuf::from("/home/u/code/app-fix-a"));
    }

    #[test]
    fn add_list_and_remove_in_a_scratch_repo() {
        let base = std::env::temp_dir().join(format!("fili-term-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("app");
        std::fs::create_dir_all(&repo).unwrap();
        let r = repo.to_string_lossy().into_owned();
        sh(&repo, &["init", "-q", "-b", "main"]);
        sh(&repo, &["-c", "user.name=t", "-c", "user.email=t@example.com", "commit", "-q", "--allow-empty", "-m", "first"]);
        assert_eq!(list(&r).len(), 1);
        assert!(list(&base.to_string_lossy()).is_empty());

        // A new branch: a worktree beside the repository
        let wt = add(&r, "feature/login").unwrap();
        assert_eq!(wt, base.join("app-feature-login").to_string_lossy());
        let l = list(&wt);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].main, l[1].branch.as_deref()), (true, Some("feature/login")));
        // From inside the worktree the main one still comes first
        assert_eq!(l[0].path, std::fs::canonicalize(&repo).unwrap().to_string_lossy());

        // Errors are git's or ours, nothing is run twice
        assert!(add(&r, "feature/login").unwrap_err().contains("already exists"));
        // A branch checked out elsewhere: git's reason alone, without its progress lines
        let busy = add(&r, "main").unwrap_err();
        assert!(busy.contains("already") && !busy.contains("Preparing"), "{busy}");
        assert!(add(&r, "-x").unwrap_err().contains("Not a valid branch name"));
        assert!(add(&r, "bad name~").unwrap_err().contains("Not a valid branch name"));
        assert!(add(&base.to_string_lossy(), "x").unwrap_err().contains("Not in a git repository"));

        // An existing branch is checked out as it is
        sh(&repo, &["branch", "old"]);
        let old = add(&r, "old").unwrap();
        assert!(Path::new(&old).join(".git").exists());

        // Removing: never the main one, never a change
        assert!(remove(&r, &r).unwrap_err().contains("main working tree"));
        assert!(remove(&r, "/nowhere").unwrap_err().contains("Not a worktree"));
        std::fs::write(Path::new(&wt).join("dirty.txt"), "x").unwrap();
        let err = remove(&r, &wt).unwrap_err();
        assert!(err.contains("untracked") || err.contains("modified"), "{err}");
        assert!(Path::new(&wt).exists());
        std::fs::remove_file(Path::new(&wt).join("dirty.txt")).unwrap();
        remove(&r, &wt).unwrap();
        assert!(!Path::new(&wt).exists());
        assert_eq!(list(&r).len(), 2);
        let _ = std::fs::remove_dir_all(&base);
    }
}
