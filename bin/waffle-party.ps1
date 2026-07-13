#requires -Version 7.0
<#
.SYNOPSIS
  Optional mdr-kernel diagnostic wrapper.
.DESCRIPTION
  Runs bin/brain-doctor.ps1 and preserves its exit code. The only difference is the terminal
  framing: this is the corporate reward path for operators who want the floor aesthetic without
  changing the serious health-check machinery.
#>
[CmdletBinding()]
param(
  [Parameter(ValueFromRemainingArguments = $true)]
  [string[]]$DoctorArgs
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

Write-Host '---' -ForegroundColor DarkCyan
Write-Host 'MDR KERNEL: Waffle Party protocol review.' -ForegroundColor Yellow
Write-Host 'Compliance diagnostic in progress.' -ForegroundColor Gray
Write-Host '---' -ForegroundColor DarkCyan

& pwsh -NoProfile -File (Join-Path $root 'bin/brain-doctor.ps1') @DoctorArgs
$doctorExit = $LASTEXITCODE

Write-Host '---' -ForegroundColor DarkCyan
if ($doctorExit -eq 0) {
  Write-Host 'Waffle Party protocol authorized. Your continued dedication to the Work has been recorded.' -ForegroundColor Yellow
} else {
  Write-Host '[ALERT]: Non-compliance event(s) remain. Waffle Party protocol denied pending correction.' -ForegroundColor Red
}
Write-Host '---' -ForegroundColor DarkCyan

exit $doctorExit
