Title: mdr-kernel changes
Type: Release Notes / Changelog
Context: Describes portable changes in the distribution without treating a source version as proof of publication or host runtime verification.

# Changes

## Unreleased on main (after 0.2.0)

- The generated contract block is cut from 10.3 KB to about 3.3 KB, so a set-up door costs about
  2k fewer tokens per session. It keeps startup, STATE wins, the never-rules (secrets, operator
  rulings, STATE.lock, machine-local lock), recall, the least-code ladder, checkpoint commits,
  the Markdown envelope, context budget and close. Mechanics that hooks and scripts enforce are
  left to them. A new Rule detail section in `protocol/CONTRACT.md` points each rule to the doc
  that explains it in full. If you edited your contract, expect a larger merge when upgrading.
- `bin/start-session.ps1` prints doctor warnings, failures, the lock line and the verdict, and
  counts the `[ ok ]` lines instead of printing them. `-FullDoctor` or
  `BRAIN_STARTUP_FULL_DOCTOR=1` prints the full output.
- `bin/update-node-map.ps1` now really sorts the file list (the sort keys did not read the
  dictionary records) and no longer stores file modification times, so regenerating the map
  without a Markdown change leaves `maps/` unchanged.
- `docs/upgrading.md` is now an exact, rehearsed 0.1-to-0.2 walkthrough for template copies and
  retained-history clones: read-only release fetch, path-by-path copy with three-way merges of
  your edits, decision-log preflight before the new hooks arrive, explicit authority migration,
  expected refusals and rollback by revert. The README no longer suggests merging the public
  repository as a second remote.
- `tests/decision-authority.ps1` clears an inherited session identity and lock token before it
  runs, so it passes inside a memory where a session holds the lock.
- `bin/start-session.ps1` closing lines now name `reference/INDEX.md` for the reading slice and the
  recall hook or `bin/prepare-work.ps1` for recall, matching the 0.2 contract.

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
