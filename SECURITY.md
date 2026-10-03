Title: mdr-kernel security and privacy-bug reporting
Type: Security Policy
Context: How to report a vulnerability or a bug that could leak private memory content, privately and before public disclosure.

# Security

## Reporting a vulnerability or a privacy bug

This pattern's core promise is that `bin/setup-private.ps1` and `bin/remote-privacy.ps1` refuse to
let your memory land anywhere but a private destination you approved. **A bug that breaks that
promise — a way to push private content to a public or unintended remote, bypass a hook, or leak a
secret into a tracked file — is a privacy bug and should be reported privately, not as a public
issue.**

Use **GitHub's private vulnerability reporting**: this repository's **Security** tab →
**Report a vulnerability**. That opens a private draft advisory only you and the maintainer can
see until a fix ships. Do not open a public issue or PR that demonstrates the bypass.

If you cannot use GitHub Security Advisories, open a regular issue that says only "privacy/security
report, details sent separately" and asks for a contact method — do not include exploit details or
example private-repo URLs in the public issue itself.

## What counts as a privacy bug here

- Setup, doctor, close-session, or the pre-push hook accepting a public repo, a fork, or an
  unverifiable-visibility remote as an approved private destination.
- A code path that pushes before `remote-privacy.ps1` has verified the destination, or that skips
  verification on retry/recovery (`sync-private.ps1`, `-OfflineCheckpoint` recovery).
- A secret-shaped string (API key, token, credential) that the pre-commit scan or
  `bin/secret-audit.ps1` fails to catch.
- The maintainer's export step (it does not ship in this repository) writing outside its intended
  output root, or shipping private content into the public distribution.

## What is not a privacy bug

Ordinary correctness bugs (a doctor check reports the wrong wording, a test is flaky, a doc
dangles) belong in a normal public issue — see [CONTRIBUTING.md](CONTRIBUTING.md).

## Scope

This is a pattern of scripts, not a hosted service: there is no server-side attack surface, no
telemetry, and no account system. The threat model is a *cooperating* multi-agent workflow
protecting against accidental leaks (see the "Known Limits" section in [README.md](README.md)),
not a defense against a malicious actor with write access to your machine or repo.

## Pull request checklist

Before opening a PR that touches privacy/security-relevant code
(`bin/remote-privacy.ps1`, `bin/setup-private.ps1`, `bin/sync-private.ps1`,
`bin/secret-audit.ps1`, `.githooks/*`):

- [ ] `bin/brain-doctor.ps1` is green.
- [ ] `tests/remote-privacy-unit.ps1` passes (hermetic; no live network needed).
- [ ] If the change affects live GitHub visibility detection, `tests/remote-privacy-live.ps1`
      was run manually against a real private/public repo pair and the result is in the PR
      description.
- [ ] The change was tested in a throwaway clone with `origin` removed, not in a clone of the
      public template or any launch/production repo.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: CONTRIBUTING.md
Related Workflows: CONTRIBUTING.md, README.md, bin/remote-privacy.ps1, tests/remote-privacy-unit.ps1
AI Execution Prompts: protocol/CONTRACT.md
