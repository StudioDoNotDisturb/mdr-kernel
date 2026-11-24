#requires -Version 7.0
<#
.SYNOPSIS
  INTERNAL setup/repair primitive. Adopters do not run this directly: the one first-run entry
  point is bin/setup-private.ps1, which verifies/pins the private destination and then calls this
  script. Run bootstrap directly only to repair a clone that was already set up (brain-doctor
  reports hooks unarmed or scaffold files missing; existing files are never overwritten). It:
    - arms git hooks (core.hooksPath=.githooks) - git does not version-control this, so a fresh
      clone has the hook files but they are inactive until armed
    - scaffolds the "grow your own memory" files so brain-doctor passes on a cold clone
    - writes the .brain-root marker
    - requires explicit session identity and sealed lock ownership before any writes
    - optionally commits the scaffold under STATE.lock so start-session begins from a clean tree
    - runs brain-doctor

  Idempotent: existing files are never overwritten. Safe to re-run.

.EXAMPLE
  pwsh -NoProfile -File bin/setup-private.ps1 -Model 'claude-opus-5'   # the adopter entry point
  pwsh -NoProfile -File bin/bootstrap-clone.ps1 -Model 'claude-opus-5' -CommitScaffold  # repair
  pwsh -NoProfile -File bin/bootstrap-clone.ps1 -Model 'claude-opus-5' # inspect scaffold before committing
  pwsh -NoProfile -File bin/bootstrap-clone.ps1 -Model '<actual-model-id>' # repair
#>
[CmdletBinding()]
param(
  [string]$Model,
  [switch]$CommitScaffold
)

$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same bootstrap when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine bootstrap-clone @forward
  exit $LASTEXITCODE
}
Set-Location $root
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Ok  ([string]$m) { Write-Host "[ ok ] $m" }
function Warn([string]$m) { Write-Host "[warn] $m" }
function Step([string]$m) { Write-Host "`n== $m ==" }

Write-Host "mdr-kernel bootstrap - $root"
if (-not (git rev-parse --is-inside-work-tree 2>$null)) {
  throw 'This folder is not a git working tree. ZIP/no-git copies are unsupported for bootstrap; use git clone (preferred) or git init first.'
}

# Bootstrap writes shared files even without -CommitScaffold. Explicit identity and sealed
# ownership are required before the first mutation; shared config is never promoted to identity.
$current = if ($Model -and $Model.Trim()) { $Model.Trim() } elseif ($env:BRAIN_MODEL -and $env:BRAIN_MODEL.Trim()) { $env:BRAIN_MODEL.Trim() } else { '' }
if (-not $current) { throw 'Bootstrap requires explicit -Model or process BRAIN_MODEL before writing; git config brain.model is display-only.' }
$env:BRAIN_MODEL = $current
$scaffoldPaths = @('STATE.md','DECISIONS.md','handoffs/INDEX.md','reference/INDEX.md','specs/INDEX.md',
  'inbox/README.md','protocol/recall-gates.json','CLAUDE.md','AGENTS.md','GEMINI.md','protocol/advisory.md',
  'protocol/decision-authority.json','protocol/rulings.json','maps/node-map.json','maps/NODE-MAP.md','maps/OBSIDIAN-NODE-LINKS.md')
$preStaged = @(git diff --cached --name-only)
# Existing staged work has an owner, even when its path resembles a scaffold output.
if ($preStaged.Count -gt 0) { throw ('Bootstrap refuses a nonempty index; commit or unstage your own work first: ' + ($preStaged -join ', ')) }
$lockScript = Join-Path $root 'bin/state-lock.ps1'
$originalLockToken = $env:BRAIN_LOCK_TOKEN
$ownedBefore = $false
if ($originalLockToken) {
  & pwsh -NoProfile -File $lockScript -CheckOwner -Model $current *> $null
  $ownedBefore = $LASTEXITCODE -eq 0
}
$acquireOutput = @(& pwsh -NoProfile -File $lockScript -Acquire -Model $current -Objective 'initialize or repair mdr-kernel memory scaffold' 2>&1)
$acquireCode = $LASTEXITCODE
$acquireOutput | ForEach-Object { Write-Host $_ }
if ($acquireCode -ne 0) { throw 'could not acquire STATE.lock before bootstrap writes' }
$acquiredToken = @($acquireOutput | ForEach-Object { if ([string]$_ -match '^session token: ([a-f0-9]+)$') { $Matches[1] } })
# Acquire prints no token when it sealed the caller's BRAIN_LOCK_TOKEN into a free lock; the
# -CheckOwner proof below then verifies that this invocation's acquire holds it.
if ($acquiredToken.Count -eq 1) { $env:BRAIN_LOCK_TOKEN = $acquiredToken[0] }
elseif (-not $ownedBefore -and -not $env:BRAIN_LOCK_TOKEN) { throw 'Acquire did not return a session token; no bootstrap writes performed.' }
& pwsh -NoProfile -File $lockScript -CheckOwner -Model $current
if ($LASTEXITCODE -ne 0) { throw 'Bootstrap ownership proof failed; no bootstrap writes performed.' }
try {

# 1) Arm hooks for this clone -----------------------------------------------------
Step 'arm git hooks'
$hooksPath = (git config --get core.hooksPath 2>$null)
if ($hooksPath -eq '.githooks') {
  Ok "hooks already armed (core.hooksPath=.githooks)"
} else {
  git config core.hooksPath .githooks
  if ($LASTEXITCODE -eq 0) { Ok "armed hooks (set core.hooksPath=.githooks)" }
  else { Warn "could not set core.hooksPath - run manually: git config core.hooksPath .githooks" }
}

# 1b) Remote privacy check ---------------------------------------------------------
# One source of truth: remote-privacy.ps1 verifies live visibility and the locally approved pin.
# A remote-backed clone must go through setup-private.ps1 before it grows memory.
Step 'remote privacy check'
$originUrl = (git remote get-url origin 2>$null)
if (-not $originUrl) {
  Ok 'no origin remote - local-only clone (wire a PRIVATE repo you own when ready)'
} else {
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'remote-privacy.ps1') -RequireApproved
  if ($LASTEXITCODE -ne 0) {
    throw "remote privacy is not verified and approved - run the adopter entry point: pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>' (example: -Model 'claude-opus-5')"
  }
  Ok 'private remote verified and approved'
}

# 2) Scaffold the grow-your-own-memory files -------------------------------------
# These are the files the contract tells you to create. brain-doctor checks for them, so a cold
# clone can't pass until they exist. We create minimal, valid starters; you fill them in as you go.
Step 'scaffold memory files'
function New-IfMissing([string]$rel, [string]$content) {
  $path = Join-Path $root $rel
  if (Test-Path -LiteralPath $path) { Ok "exists: $rel"; return }
  $dir = Split-Path -Parent $path
  if ($dir -and -not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
  [System.IO.File]::WriteAllText($path, $content, $Utf8NoBom)
  Ok "created: $rel"
}

function Invoke-NodeMap([switch]$Required) {
  $nodeMap = Join-Path $root 'bin/update-node-map.ps1'
  if (-not (Test-Path -LiteralPath $nodeMap)) {
    if ($Required) { throw 'bin/update-node-map.ps1 missing - cannot generate node map' }
    Warn 'bin/update-node-map.ps1 missing - node map not generated'
    return
  }

  & pwsh -NoProfile -File $nodeMap | ForEach-Object { Write-Host "  $_" }
  if ($LASTEXITCODE -ne 0) {
    if ($Required) { throw "node map generation failed with exit code $LASTEXITCODE" }
    Warn "node map generation failed with exit code $LASTEXITCODE"
  } else {
    Ok 'node map generated (maps/NODE-MAP.md + maps/node-map.json)'
  }
}

$top = (git rev-parse --show-toplevel 2>$null)
if ($top) { New-IfMissing '.brain-root' ($top.Trim() + "`n") }

New-IfMissing 'STATE.md' @'
Title: STATE - the current state of play
Type: Live State / Coordination File
Context: Live coordination snapshot; when status conflicts, this file wins over older docs.

# STATE - the current state of play

> Read this first, keep it current. If anything else disagrees with this file, this file wins -
> then go fix the other doc. Keep it scannable in ~30s and within the doctor budget (90 lines/10 KB).

## Right now

Fresh mdr-kernel clone. Replace this with your real current state.

## Active work

_Nothing yet._

## Blockers

_None._

## Next actions

- Replace this scaffold with your real state. Adopting existing work? Follow
  docs/adoption-guide.md: inventory first, then file each item deliberately.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: DECISIONS.md
Related Workflows: DECISIONS.md, handoffs/INDEX.md
AI Execution Prompts: protocol/CONTRACT.md
'@

New-IfMissing 'DECISIONS.md' @'
Title: Decisions
Type: Decision Log / Coordination File
Context: Append-only decision log; newer entries supersede older ones and explain why.

# Decisions

Append-only. Newest at the top. Don't edit old entries - if a decision changes, add a new one
with `bin/add-decision.ps1` and an explicit supersession field. Each entry answers: what, why,
what it rules out. A proposal does not bind; an operator ruling includes the operator's words.

---

_No decisions yet._

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: STATE.md
Related Workflows: STATE.md, handoffs/INDEX.md
AI Execution Prompts: protocol/CONTRACT.md
'@

New-IfMissing 'handoffs/INDEX.md' @'
Title: Handoffs index
Type: Handoffs Index / Session Registry
Context: Registry of session handoffs; use the newest relevant handoff for detail after STATE.md.

# Handoffs index

One line per session handoff, newest first.

| Date | File | What it covers | Status |
|---|---|---|---|

---

## SYSTEM NODE LINKS

Directory Path: handoffs/
Next Sequential File: STATE.md
Related Workflows: STATE.md, DECISIONS.md
AI Execution Prompts: protocol/CONTRACT.md
'@

New-IfMissing 'reference/INDEX.md' @'
Title: Reference index
Type: Reference Index / Knowledge Registry
Context: Registry of durable reference notes; the shared facts other sessions rely on.

# Reference index

One row per reference note. [[wiki-links]] elsewhere resolve to reference/<slug>.md.

| Note | File | What it covers |
|---|---|---|

---

## SYSTEM NODE LINKS

Directory Path: reference/
Next Sequential File: STATE.md
Related Workflows: STATE.md
AI Execution Prompts: protocol/CONTRACT.md
'@

New-IfMissing 'specs/INDEX.md' @'
Title: Specs index
Type: Specs Index / Authority Registry
Context: Registry of specs with current status and update dates.

# Specs index

One file per topic. Superseded versions go in archive/.

| Spec | File | Status | Updated |
|---|---|---|---|
| Node map | node-map.md | active | 2026-07-06 |

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: STATE.md
Related Workflows: STATE.md
AI Execution Prompts: protocol/CONTRACT.md
'@

New-IfMissing 'inbox/README.md' @'
Title: inbox
Type: Inbox / Staging Area
Context: Background/advisory output lands here as atomic notes until one session folds it into STATE.

# inbox

Drop atomic notes here instead of editing STATE.md directly. One session folds them in later.
Delete a note once it is folded. This README is ignored by the fold check.

---

## SYSTEM NODE LINKS

Directory Path: inbox/
Next Sequential File: STATE.md
Related Workflows: STATE.md
AI Execution Prompts: protocol/CONTRACT.md
'@

# Active recall data: copy the shipped example if the real file doesn't exist yet.
$recallReal = Join-Path $root 'protocol/recall-gates.json'
$recallExample = Join-Path $root 'protocol/recall-gates.example.json'
if (Test-Path $recallReal) { Ok 'exists: protocol/recall-gates.json' }
elseif (Test-Path $recallExample) {
  Copy-Item -LiteralPath $recallExample -Destination $recallReal
  Ok 'created: protocol/recall-gates.json (from example)'
} else { Warn 'protocol/recall-gates.example.json missing - cannot seed recall gates' }

# An empty new log has no legacy to classify. Never infer authority for an existing log.
$authorityPath = Join-Path $root 'protocol/decision-authority.json'
if (-not (Test-Path -LiteralPath $authorityPath)) {
  $logText = [IO.File]::ReadAllText((Join-Path $root 'DECISIONS.md'))
  if ($logText -match '(?m)^## D\d+\b') {
    Warn 'Existing decision history needs explicit migration: see docs/decision-authority.md.'
  } else {
    New-IfMissing 'protocol/decision-authority.json' '{"schema_version":1,"authority_from":1}'
  }
}
if (Test-Path -LiteralPath $authorityPath) {
  & pwsh -NoProfile -File (Join-Path $root 'bin/build-rulings.ps1')
  if ($LASTEXITCODE -ne 0) { throw 'Could not compile decision rulings.' }
}

# 2b) Generate the model front doors from the contract -----------------------------
Step 'generate front doors'
$syncDoors = Join-Path $root 'bin/sync-doors.ps1'
if (Test-Path $syncDoors) {
  & pwsh -NoProfile -File $syncDoors | ForEach-Object { Write-Host "  $_" }
  Ok 'front doors generated (CLAUDE.md / AGENTS.md / GEMINI.md / protocol/advisory.md)'
} else { Warn 'bin/sync-doors.ps1 missing - cannot generate front doors' }

if (-not $CommitScaffold) {
  Step 'generate node map'
  Invoke-NodeMap
}

# 3) Identity is already explicit and lock-proven above.
Step 'model identity'
Ok "using process BRAIN_MODEL = $current"

# 4) Optional scaffold checkpoint -------------------------------------------------
# Bootstrap deliberately creates local coordination files. start-session refuses a dirty tree so
# it cannot pull over uncommitted work. For a one-command public quick start, this flag turns the
# scaffold into the clone's first local memory checkpoint under STATE.lock, using the same lock
# discipline a normal session must follow.
if ($CommitScaffold) {
  Step 'commit scaffold'
  $dirty = @(git status --porcelain 2>$null)
  if ($dirty.Count -eq 0) {
    Ok 'nothing to commit; scaffold already checkpointed'
  } else {
    $userName = (git config user.name 2>$null)
    $userEmail = (git config user.email 2>$null)
    if (-not $userName) {
      git config user.name 'mdr-kernel bootstrap'
      Ok 'set local git user.name for bootstrap commit'
    }
    if (-not $userEmail) {
      git config user.email 'bootstrap@example.invalid'
      Ok 'set local git user.email for bootstrap commit'
    }

      # Stage only the scaffold outputs bootstrap owns. Clone metadata like .brain-root stays
      # local-only and is intentionally left out of the checkpoint.
      git add -- STATE.md DECISIONS.md handoffs/INDEX.md reference/INDEX.md specs/INDEX.md inbox/README.md protocol/recall-gates.json CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md
      if ($LASTEXITCODE -ne 0) { throw 'git add failed for scaffold commit' }
      git add -- protocol/decision-authority.json protocol/rulings.json
      if ($LASTEXITCODE -ne 0) { throw 'git add failed for authority scaffold' }
      Step 'generate node map'
      Invoke-NodeMap -Required
      git add -- maps/node-map.json maps/NODE-MAP.md maps/OBSIDIAN-NODE-LINKS.md
      if ($LASTEXITCODE -ne 0) { throw 'git add failed for generated node maps' }
      $foreign = @(git diff --cached --name-only | Where-Object { $_ -notin $scaffoldPaths })
      if ($foreign.Count) { throw ('Foreign staged paths appeared during bootstrap; leaving index intact: ' + ($foreign -join ', ')) }
      # Untracked work belonging to someone else makes the tree dirty without giving bootstrap
      # anything to commit; an empty scaffold stage is "already checkpointed", not a failure.
      if (-not @(git diff --cached --name-only).Count) {
        Ok 'nothing to commit; scaffold already checkpointed (unrelated paths left untouched)'
      } else {
        git commit -m "Initialize mdr-kernel memory scaffold`n`nLeast-file: the kernel requires these scaffold files to start a memory"
        if ($LASTEXITCODE -ne 0) { throw 'git commit failed for scaffold commit' }
        Ok 'committed scaffold checkpoint'
      }

  }
} else {
  Warn 'bootstrap created local scaffold files; commit them before start-session, or rerun with -CommitScaffold'
}

# 5) Health check -----------------------------------------------------------------
Step 'brain-doctor'
& pwsh -NoProfile -File (Join-Path $root 'bin/brain-doctor.ps1')
$doctorExit = $LASTEXITCODE

Write-Host "`nbootstrap complete. Next:"
if (-not $CommitScaffold) {
  Write-Host "  # checkpoint scaffold first, or run bootstrap again with -CommitScaffold"
}
Write-Host "  pwsh -NoProfile -File bin/start-session.ps1 -Model '$Model'"
} finally {
  # A caller that already owned this lock keeps it. Release only the lock this invocation acquired.
  if (-not $ownedBefore) {
    & pwsh -NoProfile -File $lockScript -Release -Model $current | ForEach-Object { Write-Host "  $_" }
    if ($LASTEXITCODE -ne 0) { Write-Warning 'Bootstrap lock release failed; inspect ownership before recovery.' }
  }
  if ($null -eq $originalLockToken) { Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue }
  else { $env:BRAIN_LOCK_TOKEN = $originalLockToken }
}
exit $doctorExit
