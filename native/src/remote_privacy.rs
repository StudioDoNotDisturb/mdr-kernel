//! Verifies that the push destination is private and approved. Port of `bin/remote-privacy.ps1`
//! and `bin/remote-identity.ps1`: the same statuses, messages and pin rules. GitHub remotes are
//! checked live: `gh repo view` when available, else the public API (via curl) plus an
//! authenticated `git ls-remote`. A 404 with git access proves "not publicly visible" only; it
//! cannot tell PRIVATE from Enterprise INTERNAL.
//!
//!   mdr-kernel remote-privacy [-RemoteUrl <url>] [-Approve] [-RequireApproved]
//!                             [-ConfirmPrivateRemote] [-Quiet]
//!   mdr-kernel hook pre-push <remote> <url>

use crate::args::Args;
use crate::git::{git, lossy, top};
use regex::Regex;
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

const UPSTREAM: &str = "studiodonotdisturb/mdr-kernel";

fn github_repo(url: &str) -> Option<String> {
    static GITHUB: OnceLock<Regex> = OnceLock::new();
    GITHUB
        .get_or_init(|| {
            Regex::new(r"(?i)^(?:https?://github\.com/|ssh://git@github\.com/|git@github\.com:)(?P<repo>[^/]+/[^/]+?)(?:\.git)?/?$").unwrap()
        })
        .captures(url)
        .map(|c| c["repo"].trim_end_matches('/').to_lowercase())
}

/// Canonical destination identity for the approval pin. Only GitHub coordinates and the scheme
/// and host are case-folded: paths can be case-sensitive, and folding them could merge two
/// destinations into one pin. Anything unparsed is kept verbatim, so a mismatch fails closed.
pub fn identity(url: &str) -> String {
    static URL: OnceLock<Regex> = OnceLock::new();
    static SCP: OnceLock<Regex> = OnceLock::new();
    let url = url.trim();
    if let Some(repo) = github_repo(url) {
        return format!("github:{repo}");
    }
    let form = URL.get_or_init(|| {
        Regex::new(r"^(?P<scheme>[A-Za-z][A-Za-z0-9+.-]*://)(?P<host>[^/]+)(?P<rest>/.*)?$")
            .unwrap()
    });
    if let Some(c) = form.captures(url) {
        let rest = c
            .name("rest")
            .map_or("", |m| m.as_str())
            .trim_end_matches('/');
        return format!(
            "remote:{}{}{rest}",
            c["scheme"].to_lowercase(),
            c["host"].to_lowercase()
        );
    }
    // scp-like user@host:path; a single-letter Windows drive (C:\ or C:/) is not a host.
    let scp = SCP.get_or_init(|| {
        Regex::new(r"^(?P<user>[^@/:\\]+@)?(?P<host>[^:/\\]{2,}):(?P<rest>.*)$").unwrap()
    });
    if let Some(c) = scp.captures(url) {
        let user = c.name("user").map_or("", |m| m.as_str()).to_lowercase();
        return format!(
            "remote:{user}{}:{}",
            c["host"].to_lowercase(),
            c["rest"].trim_end_matches('/')
        );
    }
    format!("remote:{}", url.trim_end_matches('/'))
}

/// The three network probes, behind a seam so the tests can replace them.
pub trait Probes {
    fn gh_available(&self) -> bool;
    /// (exit code, trimmed stdout) of `gh repo view <repo> --json visibility,isFork,nameWithOwner`.
    fn gh_view(&self, repo: &str) -> (i32, String);
    /// Ok((private, fork)) from the public API, or Err(HTTP status, None when unknown).
    fn public_lookup(&self, repo: &str) -> Result<(bool, bool), Option<u16>>;
    /// Exit code of `git ls-remote <url>` with prompts disabled.
    fn git_probe(&self, url: &str) -> i32;
}

pub struct Live;

impl Probes for Live {
    fn gh_available(&self) -> bool {
        Command::new("gh")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    fn gh_view(&self, repo: &str) -> (i32, String) {
        match Command::new("gh")
            .args([
                "repo",
                "view",
                repo,
                "--json",
                "visibility,isFork,nameWithOwner",
            ])
            .output()
        {
            Ok(o) => (
                o.status.code().unwrap_or(1),
                lossy(o.stdout).trim().to_string(),
            ),
            Err(_) => (1, String::new()),
        }
    }

    fn public_lookup(&self, repo: &str) -> Result<(bool, bool), Option<u16>> {
        let out = Command::new("curl")
            .args(["-sSL", "--max-time", "10", "-w", "\n%{http_code}"])
            .args([
                "-H",
                "User-Agent: mdr-kernel-private-remote-check",
                "-H",
                "Accept: application/vnd.github+json",
            ])
            .arg(format!("https://api.github.com/repos/{repo}"))
            .output()
            .map_err(|_| None)?;
        let text = lossy(out.stdout);
        let (body, status) = text.rsplit_once('\n').ok_or(None)?;
        match status.trim().parse::<u16>() {
            Ok(200..=299) => {
                let info: Value = serde_json::from_str(body).map_err(|_| None)?;
                Ok((
                    info["private"].as_bool().unwrap_or(false),
                    info["fork"].as_bool().unwrap_or(false),
                ))
            }
            Ok(0) | Err(_) => Err(None),
            Ok(code) => Err(Some(code)),
        }
    }

    fn git_probe(&self, url: &str) -> i32 {
        Command::new("git")
            .args(["ls-remote", url])
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_or(1, |s| s.code().unwrap_or(1))
    }
}

#[derive(Debug, PartialEq)]
pub struct Verdict {
    pub status: &'static str,
    pub message: String,
    pub identity: String,
    pub code: i32,
}

fn verdict(
    status: &'static str,
    message: impl Into<String>,
    identity: impl Into<String>,
    code: i32,
) -> Verdict {
    Verdict {
        status,
        message: message.into(),
        identity: identity.into(),
        code,
    }
}

/// The decision for a remote URL, given the approved pin (`brain.approvedRemote`). No side effects.
pub fn check(
    url: &str,
    approved: &str,
    require_approved: bool,
    confirm_private: bool,
    approve: bool,
    probes: &dyn Probes,
) -> Verdict {
    static CREDENTIALS: OnceLock<Regex> = OnceLock::new();
    if CREDENTIALS
        .get_or_init(|| Regex::new(r"(?i)^https?://[^/@]+:[^/@]+@").unwrap())
        .is_match(url)
    {
        return verdict("UNSAFE_URL", "remote URL contains embedded credentials; replace it with a credential-free URL before continuing", "", 1);
    }
    let repo = github_repo(url);
    let id = identity(url);
    if repo.as_deref() == Some(UPSTREAM) {
        return verdict("UPSTREAM_TEMPLATE", "this is the public template, not your private memory repo; use the GitHub template button to create a PRIVATE repo, then clone that repo", id, 1);
    }
    let message = if let Some(repo) = repo {
        let (mut visibility, mut fork, mut proof, mut api_status) =
            (String::new(), false, "", None);
        if probes.gh_available() {
            let (code, raw) = probes.gh_view(&repo);
            if code == 0 && !raw.is_empty() {
                if let Ok(info) = serde_json::from_str::<Value>(&raw) {
                    visibility = info["visibility"].as_str().unwrap_or("").to_uppercase();
                    fork = info["isFork"].as_bool().unwrap_or(false);
                    proof = "GitHub CLI";
                }
            }
        }
        if visibility.is_empty() {
            match probes.public_lookup(&repo) {
                Ok((private, is_fork)) => {
                    visibility = if private { "PRIVATE" } else { "PUBLIC" }.into();
                    fork = is_fork;
                    proof = "GitHub public API";
                }
                Err(status) => {
                    api_status = status;
                    // Forks of a public repository must stay public, so a 404 plus authenticated git
                    // access proves non-public visibility (not PRIVATE vs Enterprise INTERNAL).
                    if status == Some(404) && probes.git_probe(url) == 0 {
                        visibility = "PRIVATE".into();
                        proof = "not publicly visible via GitHub API + authenticated git access";
                    }
                }
            }
        }
        if fork {
            return verdict("FORK", "forks are contribution lanes, not memory repos; create a new PRIVATE repository from the template", id, 1);
        }
        if visibility != "PRIVATE" {
            let why = if !visibility.is_empty() {
                format!("GitHub reports visibility {visibility}")
            } else if let Some(code) = api_status {
                format!("GitHub privacy lookup returned HTTP {code}")
            } else {
                "GitHub privacy could not be verified live".into()
            };
            return verdict(
                "NOT_PRIVATE",
                format!("{why}; refusing to treat this remote as a memory destination"),
                id,
                1,
            );
        }
        format!("GitHub visibility PRIVATE (verified by {proof})")
    } else {
        // Pin comparisons are case-sensitive: identities are already canonical.
        if !confirm_private && approved != id {
            return verdict("UNVERIFIED_HOST", "this host has no built-in visibility check; rerun setup-private.ps1 with -ConfirmPrivateRemote only after verifying access controls", id, 1);
        }
        "non-GitHub remote privacy explicitly confirmed and URL pinned".into()
    };
    if require_approved && approved != id {
        let detail = if approved.is_empty() {
            "the private destination has not been approved yet"
        } else {
            "the push destination changed since approval"
        };
        return verdict(
            "UNAPPROVED",
            format!("{detail}; rerun bin/setup-private.ps1 to verify and approve it"),
            id,
            1,
        );
    }
    let pinned = approve || approved == id;
    let suffix = if pinned {
        "; destination pin matches"
    } else {
        "; run setup-private.ps1 to approve it"
    };
    verdict("PRIVATE", format!("{message}{suffix}"), id, 0)
}

fn config(root: &Path, key: &str) -> String {
    git(root, &["config", "--local", "--get", key])
        .map(lossy)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn set_config(root: &Path, args: &[&str]) {
    let mut all = vec!["config", "--local"];
    all.extend_from_slice(args);
    let _ = git(root, &all);
}

/// Full command: resolve the remote, decide, and record the pin on -Approve.
pub fn run(
    root: &Path,
    url: Option<&str>,
    approve: bool,
    require_approved: bool,
    confirm_private: bool,
    probes: &dyn Probes,
) -> Verdict {
    let url = url
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .or_else(|| {
            git(root, &["remote", "get-url", "--push", "origin"])
                .ok()
                .map(lossy)
                .map(|u| u.trim().to_string())
                .filter(|u| !u.is_empty())
        });
    let Some(url) = url else {
        if approve {
            set_config(root, &["--unset-all", "brain.approvedRemote"]);
            set_config(root, &["brain.mode", "local-only"]);
            set_config(root, &["--unset-all", "brain.pendingSync"]);
            return verdict(
                "LOCAL_ONLY",
                "no push remote configured; memory will stay on this machine",
                "local-only",
                0,
            );
        }
        // No remote is LOCAL_ONLY only when local-only was chosen; a pinned private memory that
        // lost origin has silently stopped backing up.
        let pinned = config(root, "brain.approvedRemote");
        if config(root, "brain.mode") == "private-remote" || !pinned.is_empty() {
            return verdict("MISSING_ORIGIN", "this memory was approved for a private remote but origin is missing; restore it (git remote add origin <approved-url>) or deliberately rerun bin/setup-private.ps1 -LocalOnly", pinned, 1);
        }
        return verdict(
            "LOCAL_ONLY",
            "no push remote configured; memory will stay on this machine",
            "local-only",
            0,
        );
    };
    let result = check(
        &url,
        &config(root, "brain.approvedRemote"),
        require_approved,
        confirm_private,
        approve,
        probes,
    );
    if approve && result.code == 0 {
        set_config(root, &["brain.approvedRemote", &result.identity]);
        set_config(root, &["brain.remoteVisibility", "private"]);
        set_config(root, &["brain.mode", "private-remote"]);
    }
    result
}

pub fn report(v: &Verdict) -> String {
    let mut out = format!("REMOTE_PRIVACY_STATUS={}\n", v.status);
    if !v.identity.is_empty() {
        out.push_str(&format!("REMOTE_ID={}\n", v.identity));
    }
    out.push_str(&format!("REMOTE_PRIVACY_MESSAGE={}\n", v.message));
    out
}

pub fn cli(argv: &[String]) -> i32 {
    let run_it = || -> Result<i32, String> {
        let args = Args::parse(
            argv,
            &["RemoteUrl", "Root"],
            &[
                "Approve",
                "RequireApproved",
                "ConfirmPrivateRemote",
                "Quiet",
            ],
        )?;
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = args
            .value("Root")
            .map_or_else(|| top(&cwd), |r| Ok(r.into()))?;
        crate::hook::fresh(&root)?;
        let v = run(
            &root,
            args.value("RemoteUrl"),
            args.switch("Approve"),
            args.switch("RequireApproved"),
            args.switch("ConfirmPrivateRemote"),
            &Live,
        );
        if !args.switch("Quiet") {
            print!("{}", report(&v));
        }
        Ok(v.code)
    };
    run_it().unwrap_or_else(|e| {
        eprintln!("X mdr-kernel remote-privacy: {e}");
        1
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        gh: Option<(&'static str, bool)>,
        api: Result<(bool, bool), Option<u16>>,
        probe: i32,
    }

    impl Probes for Fake {
        fn gh_available(&self) -> bool {
            self.gh.is_some()
        }
        fn gh_view(&self, _: &str) -> (i32, String) {
            let (visibility, fork) = self.gh.unwrap();
            (0, format!("{{\"visibility\":\"{visibility}\",\"isFork\":{fork},\"nameWithOwner\":\"owner/repo\"}}"))
        }
        fn public_lookup(&self, _: &str) -> Result<(bool, bool), Option<u16>> {
            self.api
        }
        fn git_probe(&self, _: &str) -> i32 {
            self.probe
        }
    }

    fn gh(visibility: &'static str, fork: bool) -> Fake {
        Fake {
            gh: Some((visibility, fork)),
            api: Err(None),
            probe: 1,
        }
    }
    fn api(result: Result<(bool, bool), Option<u16>>, probe: i32) -> Fake {
        Fake {
            gh: None,
            api: result,
            probe,
        }
    }
    fn status(url: &str, probes: &Fake) -> &'static str {
        check(url, "", false, false, false, probes).status
    }

    // The cases of tests/remote-privacy-unit.ps1, one for one.
    #[test]
    fn github_visibility_cases() {
        assert_eq!(
            status("https://github.com/Owner/Repo.git", &gh("PRIVATE", false)),
            "PRIVATE"
        );
        assert_eq!(
            status("https://github.com/owner/repo", &gh("PUBLIC", false)),
            "NOT_PRIVATE"
        );
        assert_eq!(
            status(
                "https://github.com/owner/flipped.git",
                &gh("PRIVATE", false)
            ),
            "PRIVATE"
        );
        assert_eq!(
            status("https://github.com/owner/flipped.git", &gh("PUBLIC", false)),
            "NOT_PRIVATE"
        );
        assert_eq!(
            status("git@github.com:owner/repo.git", &gh("INTERNAL", false)),
            "NOT_PRIVATE"
        );
        assert_eq!(
            status("ssh://git@github.com/owner/repo.git", &gh("PRIVATE", true)),
            "FORK"
        );
        assert_eq!(
            status(
                "https://github.com/owner/repo.git",
                &api(Ok((false, false)), 1)
            ),
            "NOT_PRIVATE"
        );
        assert_eq!(
            status(
                "https://github.com/owner/repo.git",
                &api(Ok((true, false)), 1)
            ),
            "PRIVATE"
        );
        assert_eq!(
            status(
                "https://github.com/owner/repo.git",
                &api(Ok((true, true)), 1)
            ),
            "FORK"
        );
        assert_eq!(
            status("https://github.com/owner/repo.git", &api(Err(Some(404)), 0)),
            "PRIVATE"
        );
        assert_eq!(
            status("https://github.com/owner/repo.git", &api(Err(Some(404)), 1)),
            "NOT_PRIVATE"
        );
        assert_eq!(
            status("https://github.com/owner/repo.git", &api(Err(Some(403)), 0)),
            "NOT_PRIVATE"
        );
        assert_eq!(
            status("https://github.com/owner/repo.git", &api(Err(None), 0)),
            "NOT_PRIVATE"
        );
        let why = check(
            "https://github.com/o/r",
            "",
            false,
            false,
            false,
            &api(Err(Some(403)), 0),
        )
        .message;
        assert!(
            why.starts_with("GitHub privacy lookup returned HTTP 403;"),
            "{why}"
        );
    }

    #[test]
    fn identities_urls_and_pins() {
        let private = gh("PRIVATE", false);
        for url in [
            "https://github.com/Owner/Repo.git",
            "git@github.com:Owner/Repo.git",
            "ssh://git@github.com/Owner/Repo.git",
        ] {
            assert_eq!(
                check(url, "", false, false, false, &private).identity,
                "github:owner/repo"
            );
        }
        assert_eq!(
            status("https://user:token@github.com/owner/repo.git", &private),
            "UNSAFE_URL"
        ); // secret-audit:allow synthetic test URL
        assert_eq!(
            status(
                "https://github.com/StudioDoNotDisturb/mdr-kernel.git",
                &private
            ),
            "UPSTREAM_TEMPLATE"
        );
        assert_eq!(
            identity("HTTPS://Git.Example.COM/Team/Repo/"),
            "remote:https://git.example.com/Team/Repo"
        );
        assert_eq!(
            identity("Git@Host.Example:Team/Repo.git"),
            "remote:git@host.example:Team/Repo.git"
        );
        assert_eq!(identity(r"C:\memories\repo"), r"remote:C:\memories\repo");
        let other = "https://git.example.com/team/repo.git";
        assert_eq!(status(other, &private), "UNVERIFIED_HOST");
        assert_eq!(
            check(other, "", false, true, false, &private).status,
            "PRIVATE"
        );
        let pinned = identity(other);
        assert_eq!(
            check(other, &pinned, true, false, false, &private).status,
            "PRIVATE"
        );
        assert_eq!(
            check(other, &pinned.to_uppercase(), true, true, false, &private).status,
            "UNAPPROVED"
        );
        let unapproved = check("https://github.com/o/r", "", true, false, false, &private);
        assert_eq!(
            (
                unapproved.status,
                unapproved
                    .message
                    .starts_with("the private destination has not been approved yet")
            ),
            ("UNAPPROVED", true)
        );
    }
}
