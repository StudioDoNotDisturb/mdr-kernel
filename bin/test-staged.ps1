#requires -Version 7.0
<#
.SYNOPSIS
  Run relevant regression tests against the exact staged candidate in an isolated directory.
  No network, GPU, source-index mutation, or working-tree substitutes. Fixtures remain for diagnosis.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$changed = @(& git -C $root diff --cached --name-only)
if ($LASTEXITCODE -ne 0) { throw 'Cannot read the staged candidate.' }
$suites = [ordered]@{
  'decision-authority.ps1' = '^(bin/(add-decision|build-rulings|check-decision-history|initialize-decision-authority|ratify-decision)\.ps1|tests/decision-authority\.ps1)$'
  'recall.ps1' = '^(bin/(brain-recall|hook-recall|prepare-work|build-rulings)\.ps1|tests/recall\.ps1|protocol/recall-gates\.example\.json)$'
  'session-safety.ps1' = '^(bin/(state-lock|close-session|bootstrap-clone|setup-private)\.ps1|\.githooks/(pre-commit|prepare-commit-msg)|tests/session-safety\.ps1)$'
}
$selected = @($suites.Keys | Where-Object { @($changed -match $suites[$_]).Count -gt 0 })
$checkGates = @($changed -match '^protocol/recall-gates(\.example)?\.json$').Count -gt 0
if (-not $selected.Count -and -not $checkGates) { Write-Host 'staged regressions: no affected suite'; exit 0 }
$snapshot = Join-Path ([IO.Path]::GetTempPath()) ('mdr-staged-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($snapshot)
$prefix = $snapshot.Replace('\','/') + '/'
& git -C $root checkout-index --all "--prefix=$prefix"
if ($LASTEXITCODE -ne 0) { throw 'Cannot materialize the staged candidate.' }

function Run-Candidate([string]$relative, [string[]]$arguments = @()) {
  $path = Join-Path $snapshot $relative
  if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { throw "Required staged helper missing: $relative" }
  $psi = [Diagnostics.ProcessStartInfo]::new('pwsh')
  $psi.UseShellExecute = $false
  $psi.WorkingDirectory = $snapshot
  foreach ($a in @('-NoProfile','-File',$path) + $arguments) { [void]$psi.ArgumentList.Add($a) }
  # Git hook environment overrides -C and can silently point nested fixtures at the real index.
  foreach ($key in @($psi.Environment.Keys)) {
    if ($key -match '^GIT_' -or $key -match '^(BRAIN_|MDR_FORCE_)') { [void]$psi.Environment.Remove($key) }
  }
  $log = Join-Path $snapshot (($relative -replace '[/\\]','_') + '.log')
  $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true
  $process = [Diagnostics.Process]::Start($psi)
  $stdout = $process.StandardOutput.ReadToEndAsync(); $stderr = $process.StandardError.ReadToEndAsync()
  if (-not $process.WaitForExit(240000)) { $process.Kill($true); throw "Staged check timed out: $relative ($snapshot)" }
  $body = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
  [IO.File]::WriteAllText($log,$body,[Text.UTF8Encoding]::new($false))
  if ($process.ExitCode -ne 0) { throw "Staged check failed: $relative. Evidence: $log`n$body" }
  Write-Host "PASS staged $relative (evidence: $log)"
}
if ($checkGates) {
  foreach ($file in 'protocol/recall-gates.json','protocol/recall-gates.example.json') {
    if (Test-Path -LiteralPath (Join-Path $snapshot $file)) { Run-Candidate 'bin/brain-recall.ps1' @('-CheckGates','-GatesPath',(Join-Path $snapshot $file)) }
  }
}
foreach ($suite in $selected) { Run-Candidate "tests/$suite" }
Write-Host "staged regressions: $($selected.Count) suite(s) passed against index bytes. Snapshot: $snapshot"
