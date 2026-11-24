#requires -Version 7.0
[CmdletBinding()]
param([string]$Adopt, [string]$Expired, [string]$Model, [string]$Token)
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine archive-adopted @forward
  exit $LASTEXITCODE
}
[Console]::Error.WriteLine('Install the native mdr-kernel binary to run archive-adopted.')
exit 2
