Title: Upgrade the kernel while preserving your memory
Type: Operator Guide / Upgrade
Context: Exact, rehearsed commands for moving an existing private 0.1 memory to 0.2 without merging histories or touching your STATE, decisions, handoffs, gates or remote.

# Upgrade an existing memory

The public repository distributes tools and examples. Your private repository holds your work.
An upgrade copies the release's tools into your memory and merges the few files you customized,
in one reviewed commit. It is not another first-time setup, not a history merge, and never a
replacement of STATE, decisions, handoffs, references, local gates or your remote.

## Hearing about a release is separate from applying it

- **Notification.** Each release is a tagged commit with a GitHub Release. On the public
  repository choose **Watch > Custom > Releases** to be notified; a newsletter, if you read one,
  is only an announcement. Neither one changes your files.
- **There is no updater.** No script fetches releases. Re-running `bin/setup-private.ps1` or
  `bin/bootstrap-clone.ps1` does not upgrade: they only create files that are missing.
- **Your private origin does not carry releases.** `git pull` from your private repository only
  synchronizes your own memory. The release must be fetched from the public repository, read-only,
  as shown below.

## Which shape is your memory?

- **(A) Template copy - the common case.** GitHub's "Use this template" creates a new repository
  whose first commit has no shared ancestry with the public repository. Never run
  `git merge --allow-unrelated-histories` against the release, and never replace your tree with a
  fresh export: either would mix release history into your memory or overwrite it.
- **(B) Retained-history clone - rare.** A private mirror that kept the public commits. It shares
  ancestry, but its doors, maps and decision log still diverge from the release, so it uses the
  same path-by-path procedure. Step U2 prints which shape you have; nothing else differs.

| Your `VERSION` | Public commit | Tag |
|---|---|---|
| `0.1.0` | `8397591e8cd42ec25a09a311df160b74c0af2afb` | none (first export) |
| `0.2.0` | `82b6b6a8efbf89242c53ee516d65a4b6fe9c36a3` | `v0.2.0` |

## What the upgrade changes and what it keeps

**Distribution-owned (taken from the release):** every path the release changed between your
version and the new one. In 0.2 that is `bin/`, `.githooks/`, `tests/`, `docs/`, `.github/`,
`_templates/`, the shipped `specs/` and `reference/` notes, `skills/mdr-kernel/`, `README.md`,
`CHANGELOG.md`, `VERSION` and the other top-level release files. A file you never edited is
replaced by the release copy; a file you edited gets a three-way merge instead (step U5).

**Merged by hand:** `protocol/CONTRACT.md` and `protocol/coordination-files.txt` when you edited
them, and any other distribution file you adapted. Your lines stay; the release's lines arrive.

**Regenerated, never copied:** the front doors `CLAUDE.md`, `AGENTS.md`, `GEMINI.md` and
`protocol/advisory.md` (from your merged contract by `bin/sync-doors.ps1`), the `maps/` output
(by `bin/update-node-map.ps1`), and `protocol/rulings.json` (by `bin/build-rulings.ps1`). The
release's own doors are phase-zero doors for fresh adopters; do not copy them over yours.

**Adopter-owned (never touched):** `STATE.md`, `DECISIONS.md`, `handoffs/`, your `reference/`
notes and `reference/INDEX.md`, `inbox/`, `protocol/recall-gates.json`, host settings such as
`.claude/` and `.codex/`, local Git configuration (`core.hooksPath`, `brain.mode`,
`brain.approvedRemote`), and your private remote with its approval pin. The release does not
change these paths, so the procedure below cannot overwrite them.

## The procedure

Run every block in **one** PowerShell 7 window, from the root of your private memory, in order.
The commands are the same on Windows and Linux; `tar` ships with both (on Windows 10 1803 and
later it is `C:\Windows\System32\tar.exe`; if `Get-Command tar` names a different one, such as
a POSIX `tar` from another toolset, which can misread `C:` as a host name, replace `tar` in U3
with that full path). Each block stops with `STOP at ...` when a step fails; fix that cause, then continue from
the same block. Allow about 30 minutes; the suites and the final commit take most of it.

### U1 - Seal identity and checkpoint

Close every other session first (their normal close releases `STATE.lock`). Then:

```powershell
# [U1] identity, clean 0.1 checkpoint, rollback point
$model = '<your model id>'
$env:BRAIN_MODEL = $model
function Must([string]$What) { if ($LASTEXITCODE -ne 0) { throw "STOP at '$What' (exit $LASTEXITCODE)" } }
if (git status --porcelain) { throw 'STOP: commit or close current work first; the tree must be clean' }
if (Test-Path -LiteralPath STATE.lock) { throw 'STOP: STATE.lock is held; let its session close, or recover it by your current rules' }
$installed = (Get-Content -Raw VERSION).Trim()
if ($installed -ne '0.1.0') { throw "STOP: this walkthrough upgrades 0.1.0; VERSION says $installed" }
git diff --quiet HEAD -- DECISIONS.md; Must 'DECISIONS.md must be committed unchanged'
git branch mdr-before-0.2.0; Must 'rollback branch'
```

### U2 - Fetch the release read-only

No remote is added; the release lands in two private refs under `refs/mdr/`, which your sync
never pushes.

```powershell
# [U2] fetch the tagged release and name the installed version
$mdr = 'https://github.com/StudioDoNotDisturb/mdr-kernel.git'
git fetch --no-tags $mdr refs/tags/v0.2.0:refs/mdr/v0.2.0; Must 'fetch v0.2.0'
if ((git rev-parse 'refs/mdr/v0.2.0^{commit}') -ne '82b6b6a8efbf89242c53ee516d65a4b6fe9c36a3') { throw 'STOP: v0.2.0 does not resolve to the published commit' }
git update-ref refs/mdr/v0.1.0 8397591e8cd42ec25a09a311df160b74c0af2afb; Must 'name 0.1.0'
$shared = @(git rev-list HEAD) -contains '8397591e8cd42ec25a09a311df160b74c0af2afb'
"Shape: $(if ($shared) { 'B - retained history' } else { 'A - template copy, no shared history' })"
git diff --stat refs/mdr/v0.1.0 HEAD -- bin .githooks tests docs protocol/CONTRACT.md protocol/coordination-files.txt
```

The last command lists the release files you customized. An empty list means you never edited
them.

### U3 - Unpack both releases for comparison

```powershell
# [U3] read-only copies of 0.1.0 and 0.2.0 in a temporary folder
$work = Join-Path ([IO.Path]::GetTempPath()) ('mdr-upgrade-' + [guid]::NewGuid().ToString('N'))
foreach ($v in 'v0.1.0', 'v0.2.0') {
  New-Item -ItemType Directory -Path (Join-Path $work $v) | Out-Null
  git archive --format=tar -o (Join-Path $work "$v.tar") "refs/mdr/$v"; Must "archive $v"
  tar -xf (Join-Path $work "$v.tar") -C (Join-Path $work $v); Must "unpack $v"
}
$work
```

### U4 - Check the decision log before any new hook arrives

0.2's pre-commit validates `DECISIONS.md` on every commit. Run its checker from the unpacked
copy first, while your 0.1 hooks are still in charge:

```powershell
# [U4] preflight the decision log with the 0.2 checker (reads only)
$ids = @([regex]::Matches((Get-Content -Raw DECISIONS.md), '(?m)^## D(\d+)\b') | ForEach-Object { [int]$_.Groups[1].Value })
$next = if ($ids.Count) { ($ids | Measure-Object -Maximum).Maximum + 1 } else { 1 }
pwsh -NoProfile -File (Join-Path $work 'v0.2.0/bin/check-decision-history.ps1') -Path (Join-Path $PWD 'DECISIONS.md') -AuthorityFrom $next
$preflight = $LASTEXITCODE
Select-String -Path DECISIONS.md -Pattern '^\*\*(Authority|Operator said)'
```

- `DECISION_HISTORY_STATUS=OK` - go to U5.
- `DECISION_HISTORY_STATUS=FAIL` - 0.1 never prescribed an entry format, so free-form logs fail
  here. Do U4b, then run U4 again.
- The `Select-String` lines are old entries that already claim operator authority. An old entry
  with `**Authority:** operator-ruled` and an `**Operator said (...)**` line will compile as a
  ruling; check each one is genuine before migrating. Never add such lines to make anything pass.

### U4b - Only if U4 failed: repair structure, not meaning

Edit `DECISIONS.md` by hand. Give every entry a heading `## D<n> - YYYY-MM-DD - <title>`,
newest first and numbered down to `D1` without gaps, and mark exactly one `**Decision:**` line
in each entry. Change no wording, add no `**Authority:**` line or quotation, delete nothing.
Commit this repair on its own with the 0.1 tools that are still installed:

```powershell
# [U4b] commit the structural repair under your 0.1 lock
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective 'normalize decision log before 0.2'
$acq; Must 'acquire 0.1 lock'
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
git add -- DECISIONS.md
git commit -m 'Normalize decision log structure before the 0.2 upgrade (no wording change)'; Must 'commit repair'
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model $model; Must 'release 0.1 lock'
Remove-Item Env:\BRAIN_LOCK_TOKEN
```

This commit is the "unchanged old decision log" checkpoint: from here on the log is only
appended to. If U4 passed, your existing commit already is that checkpoint.

### U5 - Bring the release files, hooks and helpers together

New hooks are active as soon as they are on disk, and the 0.2 pre-commit refuses when any helper
it calls is missing. This block therefore brings every changed release file in one step.

```powershell
# [U5] take unedited release files, three-way merge edited ones
if ($preflight -ne 0) { throw 'STOP: U4 must report DECISION_HISTORY_STATUS=OK first' }
$regenerated = 'CLAUDE.md', 'AGENTS.md', 'GEMINI.md'
$changed = @(git diff --name-only refs/mdr/v0.1.0 refs/mdr/v0.2.0 | Where-Object { $_ -notin $regenerated -and $_ -notlike 'maps/*' })
$empty = Join-Path $work 'empty'; New-Item -ItemType File -Path $empty -Force | Out-Null
$merged = @(); $conflicts = @()
foreach ($p in $changed) {
  $mine = git rev-parse -q --verify "HEAD:$p" 2>$null
  $base = git rev-parse -q --verify "refs/mdr/v0.1.0:$p" 2>$null
  if ($mine -eq $base) { git checkout refs/mdr/v0.2.0 -- $p; Must "take $p"; continue }
  $baseFile = if ($base) { Join-Path $work "v0.1.0/$p" } else { $empty }
  git merge-file --diff3 -L yours -L v0.1.0 -L v0.2.0 $p $baseFile (Join-Path $work "v0.2.0/$p")
  if ($LASTEXITCODE -lt 0 -or $LASTEXITCODE -gt 127) { throw "STOP: merge-file failed on $p" }
  $merged += $p; if ($LASTEXITCODE -gt 0) { $conflicts += $p }
}
"Taken from the release: $($changed.Count - $merged.Count); merged: $($merged -join ', ')"
"Conflicts to resolve by hand: $(if ($conflicts) { $conflicts -join ', ' } else { 'none' })"
```

`git diff --name-only --diff-filter=D refs/mdr/v0.1.0 refs/mdr/v0.2.0` lists files a release
deleted; 0.2 deletes none.

### U6 - Resolve and review your merges

Open every file named under "Conflicts". Each conflict shows your text after `<<<<<<< yours`,
the 0.1 original after `||||||| v0.1.0`, and the release after `=======`. Keep the release text,
re-add your own lines, and delete the three markers. Then confirm and review:

```powershell
# [U6] no markers remain; each merged file differs from the release only by your adaptations
if ($merged -and (Select-String -Path $merged -Pattern '^(<<<<<<< yours|\|\|\|\|\|\|\| v0\.1\.0|>>>>>>> v0\.2\.0)')) { throw 'STOP: conflict markers remain' }
foreach ($p in $merged) { git diff --no-index -- (Join-Path $work "v0.2.0/$p") $p }
```

In `protocol/coordination-files.txt` keep the paths you added and the eleven new 0.2 lines. In
`protocol/CONTRACT.md` keep your own rules inside the contract blocks; the doors are built from
them in U9.

### U7 - Take the lock with the 0.2 tool

```powershell
# [U7] acquire STATE.lock and keep its token in this window
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective 'upgrade mdr-kernel 0.1.0 to 0.2.0'
$acq; Must 'acquire lock'
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
pwsh -NoProfile -File bin/state-lock.ps1 -CheckOwner -Model $model; Must 'prove lock ownership'
```

### U8 - Migrate decision authority explicitly

```powershell
# [U8] boundary above your newest legacy entry; nothing is ratified
pwsh -NoProfile -File bin/initialize-decision-authority.ps1 -PreserveLegacy; Must 'initialize authority'
pwsh -NoProfile -File bin/build-rulings.ps1 -Check; Must 'rulings current'
```

Expect `AUTHORITY_INITIALIZED from=D<newest+1> legacy=unclassified`. Old entries stay exactly as
written and are not rulings unless they already carried an operator quotation (U4). To adopt an
old idea as a rule, ask the operator to rule on it in a new entry. A memory that never recorded
a decision gets `from=D1`.

### U9 - Gates, doors and map

```powershell
# [U9] validate your own gates, regenerate doors and map from your merged sources
pwsh -NoProfile -File bin/brain-recall.ps1 -CheckGates; Must 'your recall gates'
pwsh -NoProfile -File bin/sync-doors.ps1; Must 'regenerate doors'
pwsh -NoProfile -File bin/update-node-map.ps1; Must 'regenerate node map'
```

### U10 - Test, inspect, commit

```powershell
# [U10] suites from .github/workflows/doctor.yml, then one reviewed commit, then doctor
$keepToken = $env:BRAIN_LOCK_TOKEN; Remove-Item Env:\BRAIN_LOCK_TOKEN, Env:\BRAIN_MODEL   # suites run as in CI
foreach ($suite in 'remote-privacy-unit', 'skill-structure', 'envelope-check', 'decision-authority', 'recall') {
  pwsh -NoProfile -File "tests/$suite.ps1"; Must "tests/$suite"
}
$pristine = Join-Path $work 'release-checkout'
git init -q -b unborn $pristine; Must 'pristine repository'
git -C $pristine fetch -q $PWD refs/mdr/v0.2.0; Must 'pristine fetch'
git -C $pristine switch -q -c main FETCH_HEAD; Must 'pristine main'
foreach ($suite in 'private-onboarding', 'node-map-retrieval', 'session-safety') {
  pwsh -NoProfile -File (Join-Path $pristine "tests/$suite.ps1"); Must "release tests/$suite"
}
$env:BRAIN_LOCK_TOKEN = $keepToken; $env:BRAIN_MODEL = $model
git add -- $changed CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md maps protocol/decision-authority.json protocol/rulings.json; Must 'stage'
$left = @(git status --porcelain | Where-Object { $_ -notmatch '^[AMD] ' })
if ($left) { $left; throw 'STOP: unstaged or untracked paths remain; review them before committing' }
git diff --cached --stat
git commit -m 'Upgrade mdr-kernel 0.1.0 to 0.2.0'; Must 'upgrade commit'
pwsh -NoProfile -File bin/brain-doctor.ps1; Must 'doctor'
```

Five suites build their own fixtures and run against your upgraded files. `private-onboarding`,
`node-map-retrieval` and `session-safety` copy or clone the whole repository and expect a
never-set-up template (phase-zero doors, a scaffold-only first commit), so they fail by design
inside any memory that already has STATE; they run in a throwaway checkout of the release under
`$work` that has no remote. The suites run without your identity and token, as in CI. The
commit takes several minutes: pre-commit reruns the affected suites against your staged
scripts. `BRAIN_MODEL` in this window adds your `Brain-Model:` trailer.

### U11 - Release, sync privately, start fresh

```powershell
# [U11] release the lock, push only to your approved private destination, require READY
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model $model; Must 'release lock'
Remove-Item Env:\BRAIN_LOCK_TOKEN
if ((git config --local --get brain.mode) -eq 'private-remote') {
  pwsh -NoProfile -File bin/remote-privacy.ps1 -RequireApproved; Must 'privacy verifier'
  pwsh -NoProfile -File bin/sync-private.ps1; Must 'private sync'
}
Remove-Item -Recurse -Force $work
pwsh -NoProfile -File bin/start-session.ps1 -Model $model; Must 'start-session'
```

The last command must end with `STARTUP_STATUS=READY`. Keep `refs/mdr/v0.2.0`: it is the base
for the next upgrade. `git config --local --unset brain.model` removes the 0.1 identity setting
that 0.2 no longer reads; doctor warns about it otherwise.

## Expected refusals and what they mean

| Message contains | Cause | Fix |
|---|---|---|
| `set BRAIN_MODEL or include an explicit Brain-Model trailer` | A commit identified only by `git config brain.model` | Set `$env:BRAIN_MODEL` in the committing window (U1). |
| `LOCK_OWNER=UNPROVEN` or `BRAIN_LOCK_TOKEN does not match` | A lock taken by another session, or a token that is not yours | Use the token from your own `-Acquire` (U7). Never copy one from the lock file. |
| `required gate 'bin/...' is missing` | Hooks arrived without their helpers | Finish U5; it brings both together. |
| `staged decision history/authority is invalid` | New hooks with an unmigrated decision log | Complete U8 before committing. |
| `Commit existing history unchanged before migration` | `DECISIONS.md` edited after its last commit | Commit or restore it (U4, U4b) first. |
| `Existing history is structurally invalid; migration refused` | A log U4 would have failed, for example free-form 0.1 headings | Go back to U4b. |
| `Authority boundary already exists; it is immutable` | U8 already ran | Keep the committed boundary; never edit or delete it. |
| `staged paths outside this close set` | `close-session.ps1` saw staged paths that belong to other work | Commit your own work separately, or name it with `-Also`. |
| `refusing to merge unrelated histories` | A merge of the release into a template copy | Do not merge; use U5. |

## Rollback

Roll back by reverting the upgrade commit. Never `git reset` past it: later memory commits
would be lost. Keep the authority boundary; it is durable history.

```powershell
# [R1] revert the upgrade, keep later memory and the authority boundary
$upgrade = git log -1 --format=%H --grep '^Upgrade mdr-kernel 0.1.0 to 0.2.0$'
if (-not $upgrade) { throw 'STOP: no upgrade commit found' }
git revert --no-commit $upgrade
$keep = @('protocol/decision-authority.json', 'protocol/rulings.json', 'protocol/decision-ratifications.jsonl') |
  Where-Object { git cat-file -e "HEAD:$_" 2>$null; $LASTEXITCODE -eq 0 }
git checkout HEAD -- $keep; Must 'keep the authority boundary and its records'
pwsh -NoProfile -File bin/sync-doors.ps1; Must 'doors from the restored contract'
pwsh -NoProfile -File bin/update-node-map.ps1; Must 'node map'
git add -- CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md maps; Must 'stage regenerated files'
$open = @(git diff --name-only --diff-filter=U)
if ($open) { $open; throw 'STOP: resolve these by hand (keep your later memory), then git add them' }
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective 'roll back mdr-kernel 0.2.0'
$acq; Must 'acquire lock'
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
git commit -m "Revert mdr-kernel 0.2.0 upgrade ($upgrade)"; Must 'revert commit'
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model $model; Must 'release lock'
```

The revert restores the 0.1 tools, contract and doors; STATE, handoffs and decisions written
after the upgrade stay. The boundary, compiled rulings and any ratification records stay too;
0.1 ignores them, and a later upgrade reuses the boundary instead of drawing a new one (skip U8's
initializer then and run `bin/build-rulings.ps1`). Decisions added under 0.2 keep their
`**Authority:**` lines.

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

After the upgrade, configure prompt recall only for supported host events and verify an actual
firing receipt as described in `docs/recall-hooks.md`; use `bin/prepare-work.ps1` until then.

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
