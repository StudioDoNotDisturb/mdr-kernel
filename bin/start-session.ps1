#requires -Version 7.0
<#
.SYNOPSIS
  Canonical, model-neutral mdr-kernel startup sequence.
.DESCRIPTION
  Runs the full-participant startup contract in one place:
    1. brain-doctor
    2. git pull --rebase (skipped on a dirty tree)
    3. STATE.md readback

  Read-only with respect to tracked files. Refuses to pull when the working tree is dirty,
  reports a machine-readable STARTUP_STATUS line, and prints the full live STATE so every model
  starts from repository truth rather than private memory.
#>
[CmdletBinding()]
param(
  [string]$Model = $env:BRAIN_MODEL
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$modelId = if ([string]::IsNullOrWhiteSpace($Model)) { 'UNSET' } else { $Model.Trim() }
$status = 'READY'
$notes = [System.Collections.Generic.List[string]]::new()

Write-Host "mdr-kernel canonical session start"
Write-Host "ROOT=$root"
Write-Host "MODEL_ID=$modelId"
# Identity divergence is how commits silently misattribute: a session that starts as X but never
# sets $BRAIN_MODEL will lock and commit as the clone's config fallback Y.
$fallbackModel = (git config brain.model 2>$null)
if ($modelId -ne 'UNSET' -and $fallbackModel -and -not $env:BRAIN_MODEL -and ($modelId -ne $fallbackModel)) {
  Write-Host "NOTE: session started as '$modelId' but this clone's fallback identity is '$fallbackModel'."
  Write-Host "      Locks/commits without an explicit -Model will attribute to '$fallbackModel'. Seal this session:"
  Write-Host "        `$env:BRAIN_MODEL = '$modelId'"
}
if (-not (git rev-parse --is-inside-work-tree 2>$null)) {
  Write-Host "`n== startup result =="
  Write-Host 'STARTUP_STATUS=BLOCKED'
  Write-Host 'NOTE=This folder is not a git working tree. Use git clone (preferred) or git init before bootstrap/start-session.'
  Write-Host 'RESPONSE_CONTRACT=Report Right now, blockers, and top next action from STATE.md; if work was assigned, use STATE.md plus maps/NODE-MAP.md (or maps/node-map.json) to pick the smallest relevant file set before reading broadly.'
  Write-Host 'SHARED_MEMORY_CONTRACT=Anything another model needs must be written into the repo; never rely on private model memory.'
  Write-Host "RECALL_CONTRACT=Before acting on assigned work, run: pwsh -NoProfile -File bin/brain-recall.ps1 -Objective '<operator objective>'; if gates match, read the listed sources and state how the traps change the next action."
  exit 1
}
Write-Host '---' -ForegroundColor DarkCyan
Write-Host 'MDR KERNEL: Induction record opened.' -ForegroundColor Cyan
Write-Host 'External context quarantined. STATE.md will be loaded.' -ForegroundColor Gray
Write-Host '---' -ForegroundColor DarkCyan
if ($modelId -eq 'UNSET') {
  $notes.Add('Model identity is unset. Use -Model <id> now and set BRAIN_MODEL or an explicit Brain-Model trailer before lock/commit work.')
}

Write-Host "`n== 1. brain-doctor =="
$doctorOutput = & pwsh -NoProfile -File (Join-Path $root 'bin/brain-doctor.ps1') 2>&1
$doctorExit = $LASTEXITCODE
$doctorOutput | ForEach-Object { Write-Host $_ }
if ($doctorExit -ne 0) {
  $status = 'BLOCKED'
  $notes.Add("brain-doctor exited $doctorExit; resolve the reported hazard before working.")
}

Write-Host "`n== 2. git pull --rebase =="
# Tracked changes only: close-session deliberately leaves stray untracked files in the tree, and
# they cannot make a pull --rebase unsafe (same gate sync-private uses). Counting them here made
# every startup after the taught stray-file behavior report BLOCKED.
$dirty = @(git status --porcelain --untracked-files=no 2>$null)
if ($dirty.Count -gt 0) {
  $status = 'BLOCKED'
  $notes.Add('Tracked changes are uncommitted; pull was not attempted. Reconcile or commit existing work first.')
  Write-Host 'SKIPPED: tracked changes are uncommitted.'
} elseif ($doctorExit -ne 0) {
  Write-Host 'SKIPPED: brain-doctor reported a hard failure.'
} else {
  # A brand-new repo may have no upstream yet; that's fine, not a block.
  # for-each-ref returns success with an empty value when no upstream exists. Unlike rev-parse,
  # it does not write a native stderr record that ErrorActionPreference=Stop would escalate.
  $branch = (git branch --show-current 2>$null)
  $hasUpstream = (git for-each-ref --format='%(upstream:short)' "refs/heads/$branch" 2>$null)
  if (-not $hasUpstream) {
    Write-Host 'SKIPPED: no upstream configured (new repo).'
  } else {
    $pullOutput = & git pull --rebase 2>&1
    $pullExit = $LASTEXITCODE
    $pullOutput | ForEach-Object { Write-Host $_ }
    if ($pullExit -ne 0) {
      $status = 'BLOCKED'
      $notes.Add("git pull --rebase exited $pullExit; do not write from stale or conflicted state.")
    }
  }
}

Write-Host "`n== 3. STATE.md =="
$statePath = Join-Path $root 'STATE.md'
if (Test-Path -LiteralPath $statePath) {
  Get-Content -Raw -LiteralPath $statePath | Write-Host
} else {
  $status = 'BLOCKED'
  $notes.Add("STATE.md is missing - first run: pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>' (example: -Model 'claude-opus-4-8'); repairing a clone that was already set up: bin/bootstrap-clone.ps1.")
  Write-Host 'MISSING: STATE.md'
}

Write-Host "`n== startup result =="
Write-Host "STARTUP_STATUS=$status"
foreach ($note in $notes) { Write-Host "NOTE=$note" }
Write-Host 'RESPONSE_CONTRACT=Report Right now, blockers, and top next action from STATE.md; if work was assigned, use STATE.md plus maps/NODE-MAP.md (or maps/node-map.json) to pick the smallest relevant file set before reading broadly.'
Write-Host 'SHARED_MEMORY_CONTRACT=Anything another model needs must be written into the repo; never rely on private model memory.'
Write-Host "RECALL_CONTRACT=Before acting on assigned work, run: pwsh -NoProfile -File bin/brain-recall.ps1 -Objective '<operator objective>'; if gates match, read the listed sources and state how the traps change the next action."

if ($status -ne 'READY') { exit 1 }
exit 0
