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
  [string]$Model = $env:BRAIN_MODEL,
  [switch]$FullDoctor
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same startup when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key)"; "$($p.Value)" }
  })
  & $engine start-session @forward
  exit $LASTEXITCODE
}
Set-Location $root

$modelId = if ([string]::IsNullOrWhiteSpace($Model)) { 'UNSET' } else { $Model.Trim() }
$status = 'READY'
$notes = [System.Collections.Generic.List[string]]::new()

Write-Host "mdr-kernel canonical session start"
Write-Host "ROOT=$root"
Write-Host "MODEL_ID=$modelId"
# The clone's git config brain.model is display-only: lock, close and commit refuse it as identity.
# A session started with -Model but no BRAIN_MODEL must seal its identity before mutating, and a
# divergent display fallback is named so nobody mistakes it for this session.
$fallbackModel = (git config brain.model 2>$null)
if ($modelId -ne 'UNSET' -and -not $env:BRAIN_MODEL) {
  if ($fallbackModel -and ($modelId -ne $fallbackModel)) {
    Write-Host "NOTE: session started as '$modelId' but this clone's display-only fallback identity is '$fallbackModel'."
  }
  Write-Host "IDENTITY: locks/commits without an explicit -Model or BRAIN_MODEL are refused. Seal this session:"
  Write-Host "        `$env:BRAIN_MODEL = '$modelId'"
}
if (-not (git rev-parse --is-inside-work-tree 2>$null)) {
  Write-Host "`n== startup result =="
  Write-Host 'STARTUP_STATUS=BLOCKED'
  Write-Host 'NOTE=This folder is not a git working tree. Use git clone (preferred) or git init before bootstrap/start-session.'
  Write-Host 'RESPONSE_CONTRACT=Report Right now, blockers, and top next action from STATE.md; if work was assigned, use STATE.md plus reference/INDEX.md to pick the smallest relevant file set before reading broadly; the generated node map checks link health.'
  Write-Host 'SHARED_MEMORY_CONTRACT=Anything another model needs must be written into the repo; never rely on private model memory.'
  Write-Host "RECALL_CONTRACT=Before acting on assigned work, recall must run: a verified prompt hook (bin/hook-recall.ps1) or pwsh -NoProfile -File bin/prepare-work.ps1 -Objective '<operator objective>'; if gates match, read the listed sources and state how the traps change the next action."
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
# Compact by default: this report enters every model's context. [ ok ] lines are counted, not
# printed; warnings, failures, the lock line and the verdict always print.
if ($FullDoctor -or $env:BRAIN_STARTUP_FULL_DOCTOR -eq '1') {
  $doctorOutput | ForEach-Object { Write-Host $_ }
} else {
  $okHidden = 0
  foreach ($line in $doctorOutput) {
    $s = "$line"
    if ($s -match '^\[ ok \]' -and $s -notmatch 'STATE\.lock') { $okHidden++; continue }
    if ($s.Trim()) { Write-Host $s }
  }
  Write-Host "($okHidden ok checks passed, not shown; full output: pwsh -NoProfile -File bin/brain-doctor.ps1)"
}
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
  $notes.Add("STATE.md is missing - first run: pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>' (example: -Model 'claude-opus-5'); repairing a clone that was already set up: bin/bootstrap-clone.ps1.")
  Write-Host 'MISSING: STATE.md'
}

Write-Host "`n== startup result =="
Write-Host "STARTUP_STATUS=$status"
foreach ($note in $notes) { Write-Host "NOTE=$note" }
Write-Host 'RESPONSE_CONTRACT=Report Right now, blockers, and top next action from STATE.md; if work was assigned, use STATE.md plus reference/INDEX.md to pick the smallest relevant file set before reading broadly; the generated node map checks link health.'
Write-Host 'SHARED_MEMORY_CONTRACT=Anything another model needs must be written into the repo; never rely on private model memory.'
Write-Host "RECALL_CONTRACT=Before acting on assigned work, recall must run: a verified prompt hook (bin/hook-recall.ps1) or pwsh -NoProfile -File bin/prepare-work.ps1 -Objective '<operator objective>'; if gates match, read the listed sources and state how the traps change the next action."

if ($status -ne 'READY') { exit 1 }
exit 0
