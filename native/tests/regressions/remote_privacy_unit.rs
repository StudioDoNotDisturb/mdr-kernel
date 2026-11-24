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

// The native cases formerly covered by the shell suite.
#[test]
fn github_visibility_cases() {
    assert_eq!(
        status("https://github.com/Owner/Repo.git", &gh("PRIVATE", false)),
        "PRIVATE",
        "gh: PRIVATE accepted"
    );
    assert_eq!(
        status("https://github.com/owner/repo", &gh("PUBLIC", false)),
        "NOT_PRIVATE",
        "gh: PUBLIC refused"
    );
    assert_eq!(
        status(
            "https://github.com/owner/flipped.git",
            &gh("PRIVATE", false)
        ),
        "PRIVATE",
        "private baseline for a later visibility flip"
    );
    assert_eq!(
        status("https://github.com/owner/flipped.git", &gh("PUBLIC", false)),
        "NOT_PRIVATE",
        "live recheck refuses a remote flipped from private to public"
    );
    assert_eq!(
        status("git@github.com:owner/repo.git", &gh("INTERNAL", false)),
        "NOT_PRIVATE",
        "gh: INTERNAL refused"
    );
    assert_eq!(
        status("ssh://git@github.com/owner/repo.git", &gh("PRIVATE", true)),
        "FORK",
        "gh: fork refused"
    );
    assert_eq!(
        status(
            "https://github.com/owner/repo.git",
            &api(Ok((false, false)), 1)
        ),
        "NOT_PRIVATE",
        "no-gh API 200 public refused"
    );
    assert_eq!(
        status(
            "https://github.com/owner/repo.git",
            &api(Ok((true, false)), 1)
        ),
        "PRIVATE",
        "no-gh API 200 private accepted"
    );
    assert_eq!(
        status(
            "https://github.com/owner/repo.git",
            &api(Ok((true, true)), 1)
        ),
        "FORK",
        "no-gh API fork refused"
    );
    assert_eq!(
        status("https://github.com/owner/repo.git", &api(Err(Some(404)), 0)),
        "PRIVATE",
        "no-gh 404 plus authenticated git accepted as nonpublic"
    );
    assert_eq!(
        status("https://github.com/owner/repo.git", &api(Err(Some(404)), 1)),
        "NOT_PRIVATE",
        "no-gh 404 plus failed git refused"
    );
    assert_eq!(
        status("https://github.com/owner/repo.git", &api(Err(Some(403)), 0)),
        "NOT_PRIVATE",
        "no-gh 403 never falls through to git proof"
    );
    assert_eq!(
        status("https://github.com/owner/repo.git", &api(Err(None), 0)),
        "NOT_PRIVATE",
        "no-gh timeout/unknown failure refused"
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
    for (name, url) in [
        (
            "HTTPS parsing normalizes identity",
            "https://github.com/Owner/Repo.git",
        ),
        (
            "SSH scp parsing normalizes identity",
            "git@github.com:Owner/Repo.git",
        ),
        (
            "SSH URL parsing normalizes identity",
            "ssh://git@github.com/Owner/Repo.git",
        ),
    ] {
        assert_eq!(
            check(url, "", false, false, false, &private).identity,
            "github:owner/repo",
            "{name}"
        );
    }
    let embedded = "https://user:token@github.com/owner/repo.git"; // secret-audit:allow synthetic test URL
    assert_eq!(
        status(embedded, &private),
        "UNSAFE_URL",
        "embedded credentials refused"
    );
    assert_eq!(
        status(
            "https://github.com/StudioDoNotDisturb/mdr-kernel.git",
            &private
        ),
        "UPSTREAM_TEMPLATE",
        "upstream template refused before probes"
    );
    let upper = identity("ssh://git@host.example/srv/Repo.git");
    let lower = identity("ssh://git@host.example/srv/repo.git");
    let folded = identity("SSH://GIT@HOST.EXAMPLE/srv/Repo.git");
    let scp = identity("git@Host.example:Srv/Repo.git");
    assert!(
        upper != lower
            && upper == folded
            && scp == identity("git@host.example:Srv/Repo.git")
            && scp != identity("git@host.example:srv/repo.git"),
        "generic remote pins preserve path case and fold only scheme/host",
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
