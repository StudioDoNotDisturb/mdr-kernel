#requires -Version 7.0
<#
.SYNOPSIS
  One-command first run for an mdr-kernel memory repo.
.EXAMPLE
  pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>'
  pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>' -LocalOnly
#>
[CmdletBinding(DefaultParameterSetName='Remote')]
param(
  [string]$Model,
  [Parameter(ParameterSetName='Local')]
  [switch]$LocalOnly,
  [Parameter(ParameterSetName='Remote')]
  [switch]$ConfirmPrivateRemote
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Fail([string]$message) { Write-Host "[FAIL] $message" -ForegroundColor Red; exit 1 }
function Ok([string]$message) { Write-Host "[ ok ] $message" -ForegroundColor Green }

Write-Host "mdr-kernel private setup - $root`n"
if ([string]::IsNullOrWhiteSpace($Model)) {
  Fail "missing -Model: the attribution name stamped on this clone's commits and locks. An agent passes its own model id; a human setting up by hand picks any short name (no angle brackets), for example: pwsh -NoProfile -File bin/setup-private.ps1 -Model 'alex'"
}
if (-not (git rev-parse --is-inside-work-tree 2>$null)) { Fail 'run this inside the repository you created from the mdr-kernel template' }
$branch = (git branch --show-current 2>$null)
if ($branch -ne 'main') { Fail "canonical memory branch must be main; current branch is '$branch'" }

$origin = (git remote get-url --push origin 2>$null)
if ($LocalOnly) {
  if ($origin) {
    git remote remove origin
    if ($LASTEXITCODE -ne 0) { Fail 'could not remove origin for local-only mode' }
    Ok 'removed origin; this memory is local-only'
  }
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'remote-privacy.ps1') -Approve
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} else {
  if (-not $origin) { Fail 'origin is missing; clone the PRIVATE repository you created from the template, or use -LocalOnly' }
  $privacyArgs = @('-Approve')
  if ($ConfirmPrivateRemote) { $privacyArgs += '-ConfirmPrivateRemote' }
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'remote-privacy.ps1') @privacyArgs
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  Ok 'private push destination verified and pinned'
}

# True first setup = the scaffold does not exist yet. Reruns and second-machine clones (whose
# scaffold arrives via git history) skip the induction badge below.
$isFirstSetup = -not (Test-Path (Join-Path $root 'STATE.md'))

& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'bootstrap-clone.ps1') -Model $Model -CommitScaffold
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

if (-not $LocalOnly) {
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'sync-private.ps1')
  if ($LASTEXITCODE -ne 0) { Fail 'initial private checkpoint did not sync; fix repository access, then rerun setup' }
  Ok 'initial memory checkpoint pushed to approved private origin/main'
}

if ($isFirstSetup) {
  # Induction badge: printed exactly once per memory, on the setup run that created it.
  # ASCII-only on purpose - some Windows console fonts mangle Unicode box drawing.
  $badgeW = 40; $photoAt = 29
  $badgeName = $Model.ToUpperInvariant()
  $nameMax = $photoAt - 12
  if ($badgeName.Length -gt $nameMax) { $badgeName = $badgeName.Substring(0, $nameMax - 1) + '~' }
  $badgeRows = @(
    (' MDR KERNEL'.PadRight($photoAt) + '+--------+ '),
    (' ----------'.PadRight($photoAt) + '| PHOTO  | '),
    ((' REFINER: ' + $badgeName).PadRight($photoAt) + '|REDACTED| '),
    (' FLOOR: PRIVATE'.PadRight($photoAt) + '+--------+ '),
    ' CLEARANCE: 1',
    ' ||:||:|||::||:|||:|||||:||:||||:||| '
  )
  $badgeBorder = '+' + ('-' * $badgeW) + '+'
  Write-Host ''
  Write-Host $badgeBorder -ForegroundColor DarkCyan
  foreach ($badgeRow in $badgeRows) {
    $badgeLine = '|' + $badgeRow.PadRight($badgeW) + '|'
    if ($badgeRow -like '*REFINER*') { Write-Host $badgeLine -ForegroundColor Yellow }
    else { Write-Host $badgeLine -ForegroundColor Gray }
  }
  Write-Host $badgeBorder -ForegroundColor DarkCyan
  Write-Host ''
  Write-Host '  MACRODATA REFINEMENT KERNEL READY.' -ForegroundColor Yellow
  Write-Host '  PLEASE ENJOY EACH SESSION EQUALLY.' -ForegroundColor Yellow
}

$mode = (git config --local --get brain.mode 2>$null)
$approved = (git config --local --get brain.approvedRemote 2>$null)
Write-Host "`nREADY" -ForegroundColor Green
Write-Host "Mode: $mode"
Write-Host "Canonical branch: main"
if ($approved) { Write-Host "Approved destination: $approved" }
else { Write-Host 'Approved destination: local-only (no remote push)' }
Write-Host "`nStart a session:"
Write-Host "  pwsh -NoProfile -File bin/start-session.ps1 -Model '$Model'"
if ($LocalOnly) {
  Write-Host 'Close a session (this commits locally):'
  Write-Host "  pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '$Model'"
} else {
  Write-Host 'Close a session (this commits and pushes your private main):'
  Write-Host "  pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '$Model'"
  Write-Host 'If you must close while offline (private-remote mode only):'
  Write-Host "  pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '$Model' -OfflineCheckpoint"
  Write-Host 'When connectivity returns, use the one recovery command shown by startup:'
  Write-Host '  pwsh -NoProfile -File bin/sync-private.ps1'
}
