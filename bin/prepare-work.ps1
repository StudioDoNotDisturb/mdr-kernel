#requires -Version 7.0
<#
.SYNOPSIS
  Portable recall fallback for a surface without a proven working prompt hook.
#>
[CmdletBinding(DefaultParameterSetName = 'Objective')]
param(
  [Parameter(Mandatory, ParameterSetName = 'Objective')][ValidateNotNullOrEmpty()][string]$Objective,
  [Parameter(Mandatory, ParameterSetName = 'Stdin')][switch]$FromStdin
)
$ErrorActionPreference = 'Stop'
$matcher = Join-Path $PSScriptRoot 'brain-recall.ps1'
if (-not (Test-Path -LiteralPath $matcher -PathType Leaf)) {
  [Console]::Error.WriteLine('PREPARE_WORK_STATUS=BLOCKED reason=missing-matcher')
  exit 2
}
Write-Output 'RECALL_SURFACE=fallback-wrapper status=RUNNING'
# A child scope invokes the same authority without putting Objective on another OS command line.
$global:LASTEXITCODE = 0
if ($FromStdin) { & $matcher -FromStdin } else { & $matcher -Objective $Objective }
if ($LASTEXITCODE -ne 0) {
  [Console]::Error.WriteLine('PREPARE_WORK_STATUS=BLOCKED reason=recall-failed')
  exit $LASTEXITCODE
}
Write-Output 'PREPARE_WORK_STATUS=READY (read any matched gates and required sources before acting)'
exit 0
