#requires -Version 7.0
<#
.SYNOPSIS
  Safely sends locally pending memory checkpoints to the approved private origin/main.
.DESCRIPTION
  Re-verifies privacy live, fetches and rebases when needed, then pushes explicit origin/main.
  If the remote moves during that attempt it retries the fetch/rebase/push sequence exactly once.
  It never uses the cached destination pin as proof of current privacy and aborts a conflicted
  rebase before returning.
#>
[CmdletBinding()]
param(
  [switch]$PreserveWorkingTree
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same sync when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  # Arrays (-Also) travel comma-joined; the engine splits them as this script does.
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$(@($p.Value) -join ',')" }
  })
  & $engine sync-private @forward
  exit $LASTEXITCODE
}
Set-Location $root

function Fail([string]$Message) { Write-Host "[FAIL] $Message" -ForegroundColor Red; exit 1 }
function Ok([string]$Message) { Write-Host "[ ok ] $Message" -ForegroundColor Green }
function Update-FromOrigin {
  $remoteMain = (& git ls-remote --exit-code --heads origin refs/heads/main 2>$null | Out-String).Trim()
  $remoteMainExit = $LASTEXITCODE
  if ($remoteMainExit -eq 2) {
    $ahead = [int]((& git rev-list --count HEAD 2>$null | Out-String).Trim())
    Write-Host "SYNC_STATUS=ahead:$ahead behind:0 remote-main:missing"
    return
  }
  if ($remoteMainExit -ne 0 -or -not $remoteMain) { Fail 'could not inspect private origin/main; no push was attempted' }

  git fetch origin main | Out-Host
  if ($LASTEXITCODE -ne 0) { Fail 'fetch failed; no push was attempted' }
  $ahead = [int]((& git rev-list --count origin/main..HEAD 2>$null | Out-String).Trim())
  $behind = [int]((& git rev-list --count HEAD..origin/main 2>$null | Out-String).Trim())
  Write-Host "SYNC_STATUS=ahead:$ahead behind:$behind"
  if ($behind -gt 0) {
    $rebaseArgs = @('rebase')
    if ($PreserveWorkingTree) { $rebaseArgs += '--autostash' }
    $rebaseArgs += 'origin/main'
    git @rebaseArgs | Out-Host
    if ($LASTEXITCODE -ne 0) {
      git rebase --abort 2>$null
      Fail 'rebase conflicted and was aborted; STATE.lock was not changed. Resolve with an experienced Git user before retrying'
    }
    if ($PreserveWorkingTree -and (git diff --name-only --diff-filter=U 2>$null)) {
      Fail 'rebase completed, but restoring unrelated tracked changes conflicted; no push was attempted and your changes remain in the autostash. Run git status and ask an experienced Git user before retrying'
    }
  }
}
function Push-OriginMain {
  git push origin main | Out-Host
  return $LASTEXITCODE
}

Write-Host "mdr-kernel private sync - $root`n"
if ((git config --local --get brain.mode 2>$null) -eq 'local-only') {
  Fail 'this memory is local-only; there is no private remote to sync'
}
if (-not (git remote get-url --push origin 2>$null)) {
  Fail 'origin is missing; rerun bin/setup-private.ps1 after cloning or adding your PRIVATE repository'
}
if ((git branch --show-current 2>$null) -ne 'main') { Fail 'private memory sync runs only from main' }
if (-not $PreserveWorkingTree -and (git status --porcelain --untracked-files=no)) {
  Fail 'tracked changes are not committed; close the session before syncing'
}

$privacyOut = @(& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'remote-privacy.ps1') -RequireApproved)
$privacyOut | Write-Host
if ($LASTEXITCODE -ne 0) { Fail 'origin is not live-verified as the approved private destination; run bin/setup-private.ps1' }

Update-FromOrigin
$pushExit = Push-OriginMain
if ($pushExit -ne 0) {
  Write-Host '[warn] push failed; rechecking privacy and origin/main, then retrying once'
  Update-FromOrigin
  $pushExit = Push-OriginMain
}
if ($pushExit -ne 0) { Fail 'push retry failed; pending-sync state remains and no further attempt was made' }
git config --local --unset-all brain.pendingSync 2>$null
Ok 'private origin/main is synchronized'
Write-Host 'PENDING_SYNC=false'
