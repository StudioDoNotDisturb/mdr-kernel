#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$helper = Join-Path $root 'bin/remote-privacy.ps1'

Write-Host 'remote-privacy live GitHub smoke test'
$probeOut = @(& pwsh -NoProfile -File $helper -RemoteUrl 'https://github.com/octocat/Hello-World.git' 2>&1)
$probeExit = $LASTEXITCODE
$probeOut | Out-Host
$probeText = ($probeOut | Out-String)
if ($probeExit -ne 1) {
  Write-Host "[FAIL] expected a known public GitHub repository to be refused (exit 1, got $probeExit)"
  exit 1
}
if ($probeText -notmatch 'REMOTE_PRIVACY_STATUS=NOT_PRIVATE' -or
    $probeText -notmatch 'GitHub reports visibility PUBLIC') {
  Write-Host '[FAIL] refusal lacked live PUBLIC-visibility proof; an outage must not count as a passing smoke test'
  exit 1
}
Write-Host '[PASS] live GitHub visibility check refused a known public repository'
Write-Host 'remote-privacy-live: PASS'
