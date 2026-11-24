Title: mdr-kernel changes
Type: Release Notes / Changelog
Context: Describes portable changes in the distribution without treating a source version as proof of publication or host runtime verification.

# Changes

## 2.0.0

`mdr-kernel` 2.0 replaces the PowerShell script suite with a single compiled Rust binary
(`mdr-kernel` / `mdr-kernel.exe`) for Windows and Linux, adds commit-ladder guards, repeat-suppressed
recall, document lifecycle management, and a built-in MCP server, and trims the generated contract
block by about two-thirds.

### Native Rust engine (replaces PowerShell)

- **One binary runs the full lifecycle:** `setup-private`, `bootstrap-clone`, `start-session`,
  `session-stop`, `close-session`, `sync-private`, `state-lock`, `remote-privacy`, `doctor`,
  `state-budget`, `secret-audit`, `sync-doors`, `brain-recall`, `hook-recall`, `prepare-work`,
  `add-decision`, `check-decision-history`, `build-rulings`, `ratify-decision`,
  `initialize-decision-authority`, `write-approval`, `update-node-map`, `envelope`, `find`,
  `find-stale`, `archive-adopted`, and `mcp`.
- **Direct hook execution and stale-binary guard:** `.githooks/pre-commit`,
  `.githooks/prepare-commit-msg`, `.githooks/commit-msg`, and `.githooks/pre-push` call
  `bin/mdr-kernel` directly. Each binary embeds a SHA-256 fingerprint of its `native/` sources and
  refuses to run if the staged `native/` sources differ. `mdr-kernel install` copies a compiled
  binary into `bin/`.
- **PowerShell removed and tests moved to Cargo:** PowerShell 7 is no longer required. The legacy
  `bin/*.ps1` scripts and POSIX wrapper twins are removed, and the test suites run through Cargo
  and cold-clone tests instead of PowerShell scripts.
- **Release binaries with checksums:** Tagged releases build `mdr-kernel-windows-x86_64.exe` and
  `mdr-kernel-linux-x86_64` on `windows-latest` and `ubuntu-latest` with `SHA256SUMS.txt` attached.
  Both prebuilt download verification and source builds (`cargo build --locked --release` plus
  `mdr-kernel install`) are documented in `README.md`, with a full `0.2.0` to `2.0.0` upgrade
  guide in `docs/upgrading.md`.

### New 2.0 capabilities

- **Compact contract and commit-ladder trailers:** `BRAIN-CONTRACT v2` trims the generated door
  block from 10.3 KB to about 3.3 KB. `.githooks/commit-msg` enforces `Least-code:` (for new `bin/`
  files or more than 60 added code lines) and `Least-file:` (for added files or folders beyond one
  dated handoff and `inbox/` notes).
- **Per-session recall receipts:** `hook-recall` stores trap fingerprints per `session_id` in the
  OS temp folder (`mdr-kernel-recall-delivery/`) so unchanged traps are not repeated across prompts
  in the same session, and re-arms automatically when a trap changes, a checkpoint changes, or a
  `SessionStart` payload arrives (including after context compaction).
- **Document lifecycle commands:** `mdr-kernel find-stale list` reports `[Ingested]`,
  `[Superseded]`, `[Rejected]`, and un-cited 30-day-old `[Record]` documents.
  `mdr-kernel archive-adopted` moves approved working documents or expired records under `archive/`
  under your `STATE.lock`, redirects `INDEX.md` rows, refuses active main-file citations, and
  checks for links or concurrent edits.
- **Built-in read-only MCP server:** `mdr-kernel mcp` speaks newline-delimited JSON-RPC 2.0 on
  stdio and exposes `recall_gates`, `get_gate_traps`, and `find_history` in-process with no Node or
  Bun runtime required.
- **Local history search hardening:** `mdr-kernel find` self-heals when its SQLite cache is corrupt
  or has a foreign schema, skips non-UTF-8 handoffs with a warning instead of failing all search,
  names the cache directory (`%LOCALAPPDATA%\mdr-kernel\history` / `~/.cache/mdr-kernel/history`),
  and rejects `--`-prefixed values passed to value flags without `-Opt:--val` / `--opt:--val`.
- **CI on pull requests and branches:** `.github/workflows/doctor.yml` checks out the tested commit
  as local `main` so PR and branch CI runs end to end.

### Behavior fixes

- **Write-approval post-approval swap gate closed:** `write-approval` validation now compares git's
  staged blob ID for the target path against `git hash-object --path=<target>` of the approved
  content (applying the same line-ending filters as staging) and adds `--no-renames` defense in
  depth, blocking content swaps after approval.
- **Four node-map bugs fixed:** Kept tracked-set and issue-key `HashSet` collections intact instead
  of unrolling them in PowerShell (fixing `update-node-map -Check` crashes on clean memories,
  generation crashes on repositories with untracked Markdown, and false substring matches when only
  one link issue existed) and enabled `core.quotepath=false` with UTF-8 decoding on
  `git ls-files` so non-ASCII filenames track and resolve reliably.
- **`sync-doors` case and exit-code fixes:** Door drift checks now compare case-sensitively (so
  case-only contract edits are caught as drift), and usage errors exit with code 2 rather than 1.
- **`doctor` pending-sync and lock-age fixes:** Fixed pending-sync counting (which previously read
  the empty stdout of `git show-ref --quiet` and never ran) and fixed UTC `Z` lock-timestamp
  parsing (which previously shifted calculated lock age by the local timezone offset).
- **Closeout, `secret-audit`, and CLI forwarding fixes:** `close-session` now checks unmatched
  `-Also` patterns on their own instead of treating regenerated `maps/*` as a match;
  `secret-audit -CheckScannerCoverage` requires the native scanners; and wrapper hand-offs pass
  values using `-Name:value` so `--`-prefixed payloads forward intact.

## 2.0.0-rc.1

- Added `mdr-kernel find` for local history search. It indexes STATE, decision logs, and handoffs
  in a per-memory SQLite cache and returns file and line references for matching passages.
- The native search builds from source. Private setup, recall, locking, and close still use the
  existing PowerShell entry points.

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
- The upgrade guide covers 0.2.0 to 2.0.0-rc.1, including review of the contract merge, regeneration
  of the doors and maps, and preservation of existing decision authority and private memory.
- `tests/decision-authority.ps1` clears an inherited session identity and lock token before it
  runs, so it passes inside a memory where a session holds the lock.
- `bin/start-session.ps1` closing lines now name `reference/INDEX.md` for the reading slice and the
  recall hook or `bin/prepare-work.ps1` for recall, matching the 0.2 contract.

## Post-0.2.0 documentation

- `docs/upgrading.md` is now an exact, rehearsed 0.1-to-0.2 walkthrough for template copies and
  retained-history clones: read-only release fetch, path-by-path copy with three-way merges of
  your edits, decision-log preflight before the new hooks arrive, explicit authority migration,
  expected refusals and rollback by revert. The README no longer suggests merging the public
  repository as a second remote.

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
