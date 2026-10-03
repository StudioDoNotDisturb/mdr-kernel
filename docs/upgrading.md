Title: Upgrade the kernel while preserving your memory
Type: Operator Guide / Upgrade
Context: Provides a deliberate merge path for existing private memories without replacing their history, gates, or operator authority.

# Upgrade an existing memory

The public repository distributes tools and examples. Your private repository holds your work.
An upgrade is a reviewed software merge, not another first-time export and not a replacement
of STATE, decisions, handoffs, references, or local gate data.

1. Finish or checkpoint current work. Coordinate with other sessions and acquire your own lock
   before shared changes. Preserve a reachable Git commit for rollback. Review your current
   remote with the existing privacy verifier; an upgrade does not authorize a new destination.
2. Compare the distribution change against your installed version. Bring over the relevant
   `bin/`, `.githooks/`, tests and documentation. Merge your customized contract and coordination
   registry deliberately. Keep your real model IDs, local host configuration, reference index,
   memory, and `protocol/recall-gates.json`. The example gates are examples, not replacements.
3. For an older decision log, use the explicit migration in `docs/decision-authority.md`.
   An unchanged committed log is required before the boundary is created. Resolve genuine
   structural errors deliberately; never invent operator quotations to make a check pass.
4. Validate your real gates with `bin/brain-recall.ps1 -CheckGates`, compile rulings with
   `bin/build-rulings.ps1`, and regenerate the doors with `bin/sync-doors.ps1`. Keep the private
   full contract doors; the distribution's phase-zero doors are for fresh adopters only.
5. Review the staged diff. Run the tests listed in `.github/workflows/doctor.yml` locally and
   `bin/brain-doctor.ps1`. Pre-commit also runs affected suites against staged scripts. Commit
   only the reviewed upgrade and generated outputs. Sync to your approved private destination.
6. Start a new session through `bin/start-session.ps1` and require READY. Configure prompt
   recall only for supported host events and verify an actual firing receipt as described in
   `docs/recall-hooks.md`; use `prepare-work.ps1` until then.

The 0.2 tools intentionally refuse config-only identity, tokenless locks, unrelated staged
closeouts, malformed recall data, and stale ruling output. Fix the underlying evidence instead
of disabling hooks. A stale lock is not proof that another session abandoned it.

## Upgrade notes: 0.1 to 0.2

These 0.2 behaviours break habits that worked in 0.1. Each one is deliberate.

- **Every commit needs an explicit identity.** The commit-message hook refuses a commit that has
  neither `BRAIN_MODEL` in the committing shell nor a `Brain-Model:` trailer, and a trailer that
  disagrees with `BRAIN_MODEL`; `unknown` is refused as a name. It no longer falls back to
  `git config brain.model`, and setup no longer writes that setting. This includes a human's hand
  commits.
- **Tokenless locks no longer prove ownership.** Heartbeat, release, `-CheckOwner` and
  coordination commits need explicit identity plus the token printed by your own
  `bin/state-lock.ps1 -Acquire` (`BRAIN_LOCK_TOKEN` or `-Token`). A lock written by 0.1 without a
  token must be recovered explicitly, not adopted.
- **Forced release needs a recorded reason.** `-Release -Force` now requires
  `-AbandonmentReason '<independent evidence or operator direction>'`; a fresh heartbeat also
  needs `MDR_FORCE_LIVE_LOCK=1`. A stale heartbeat alone is not proof that the holder is gone.
- **Closeout keeps to its close set.** `bin/close-session.ps1` refuses when unrelated paths are
  staged and leaves them staged. It no longer adopts Markdown notes under `inbox/`; name the notes
  your session owns with `-Also`.
- **An existing decision log blocks commits until migrated.** Pre-commit checks decision history
  on every commit once `DECISIONS.md` exists. Commit the old log unchanged, then run
  `bin/initialize-decision-authority.ps1 -PreserveLegacy` under your own lock (see
  `docs/decision-authority.md`). Old entries are preserved, not ratified.
- **Pre-commit needs `pwsh` and every gate helper.** The hook refuses when `pwsh` or a required
  utility is missing, or when any of `bin/state-lock.ps1`, `bin/write-approval.ps1`,
  `bin/sync-doors.ps1`, `bin/envelope-check.ps1`, `bin/check-decision-history.ps1`,
  `bin/build-rulings.ps1`, `bin/test-staged.ps1` or `protocol/coordination-files.txt` is absent.
  Bring the helpers over in the same commit as the hooks.
- **The staged test runner must match its staged copy.** Pre-commit refuses when
  `bin/test-staged.ps1` in the working tree differs from the staged version; stage or restore it.
- **Some commits take minutes.** Pre-commit runs the affected suites against the staged scripts.
  A commit that changes a lifecycle script (`state-lock`, `close-session`, `bootstrap-clone`,
  `setup-private` or the commit hooks) runs `tests/session-safety.ps1`, about 2-3 minutes. Each
  run keeps its snapshot in a `mdr-staged-*` folder under the system temp directory for
  diagnosis; those folders can be deleted.
- **`protocol/coordination-files.txt` gained entries.** The decision tools, `bin/hook-recall.ps1`,
  `bin/prepare-work.ps1`, `bin/test-staged.ps1`, `protocol/decision-authority.json`,
  `protocol/decision-ratifications.jsonl` and `protocol/rulings.json` now need `STATE.lock`.
  Merge the new lines into your own list; keep the entries you added yourself.

Rollback means reverting the reviewed software commit and checking compatibility with any
memory written afterward. Do not reset away subsequent private history. The authority boundary
and ratification records are durable history, not disposable configuration.

## Optional project patterns

Long projects can keep their own packet with current scope, asset references, local traps and
review status. Promote a trap into global recall only when it is broadly useful; the generic
kernel does not ship a video production system. Likewise, a project's explicit phase can guard
unapproved work, but its policy belongs to that project. These are patterns to adopt deliberately,
not extra background services or inherited studio policies.

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: decision-authority.md
Related Workflows: CHANGELOG.md, docs/decision-authority.md, docs/recall-hooks.md, protocol/RUNBOOK.md
AI Execution Prompts: protocol/CONTRACT.md
