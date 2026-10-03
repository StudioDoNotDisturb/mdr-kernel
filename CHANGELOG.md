Title: mdr-kernel changes
Type: Release Notes / Changelog
Context: Describes portable changes in the distribution without treating a source version as proof of publication or host runtime verification.

# Changes

## 0.2.0

- Explicit session identity and sealed tokens for lock mutations, ownership checks and commit
  attribution. Stale heartbeat recovery requires recorded independent evidence or operator direction.
- Closeout keeps to its owned paths and refuses foreign staged work. Bootstrap proves ownership
  before shared writes and refuses a preexisting index.
- Prompt recall accepts bounded typed input, validates gate data, surfaces all matches in compact
  output, and visibly reports degraded adapters. Manual fallback uses the same recall authority.
- Decision proposals, quoted operator rulings, append-only history and human hash-bound
  ratification. Generated rulings retain the full rule and verify source freshness at recall.
- An explicit migration boundary preserves older logs without ratifying their contents.
- Missing required hook utilities/helpers fail closed. Relevant regression suites run against
  staged candidate scripts. Git hook executable checks are included on POSIX systems.
- Decision tools read staged files safely when they start with a byte-order mark; migration
  requires the old log to be committed; a fixture compile no longer reads the live ratification
  ledger; the oldest entry's hash no longer depends on blank lines before the footer; and the
  initializer writes the authority policy in compact form.
- Pre-commit refuses when `bin/test-staged.ps1` differs from its staged copy.
- Close-session refuses foreign staged paths before it takes the lock or repairs a handoff.
- Bootstrap no longer writes `git config brain.model`.
- The recall hook and `-FromStdin` decode stdin as UTF-8; before this, non-ASCII triggers matched
  nothing on Windows.
- `brain-recall -Increment` keeps the gate file's field order and its deeply nested extension
  fields.
- The CI doctor workflow fails when any suite fails; on Windows the step previously checked only
  the last suite's exit code.
- The doctor no longer fails below PowerShell 7.3, where `GetUnixFileMode` is missing.
- The doctor reports prompt recall as three separate claims: configured, adapter tested, and
  firing unverified.
- `brain-doctor -HardOnly` also covers Git hooks, the recall gate schema and rulings freshness.
- Markdown envelopes and focused reference-index reading carry forward into this distribution.
  Node maps remain generated link-health evidence. Durable artifacts require durable generators.

Upgrade notes for 0.1 adopters, in brief: every commit needs `BRAIN_MODEL` or a `Brain-Model:`
trailer; tokenless locks no longer prove ownership; `-Release -Force` needs `-AbandonmentReason`;
closeout refuses foreign staged paths and no longer adopts inbox notes; an existing decision log
blocks commits until `bin/initialize-decision-authority.ps1 -PreserveLegacy` runs; pre-commit
needs `pwsh` and every gate helper, and refuses when `bin/test-staged.ps1` differs from its
staged copy; `protocol/coordination-files.txt` gained entries. Details are
in [the upgrade guide](docs/upgrading.md#upgrade-notes-01-to-02).

The distribution source version is not a publication receipt. Hook configuration, fixture tests,
and observed host firing are separate evidence. Private studio media/NAS services are excluded.
See [the upgrade guide](docs/upgrading.md) before merging into an existing memory.

## 0.1.x baseline

Shared memory contract, canonical startup, heartbeat coordination, private-destination setup,
remote privacy checks, local-only and offline recovery, recall gates, generated maps and Markdown
envelopes formed the earlier baseline. The July privacy and onboarding repairs are not new 0.2
features. Historical public commits remain the release record.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: docs/upgrading.md
Related Workflows: README.md, docs/upgrading.md, .github/workflows/doctor.yml
AI Execution Prompts: protocol/CONTRACT.md
