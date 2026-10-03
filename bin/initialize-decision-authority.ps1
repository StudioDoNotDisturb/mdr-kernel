#requires -Version 7.0
<#
.SYNOPSIS
  Initialize an immutable authority boundary without rewriting or ratifying old entries.
.DESCRIPTION
  Fresh empty logs start at D1. -PreserveLegacy must be explicit for existing histories;
  only already committed entries may remain unclassified. The caller must own STATE.lock.
#>
[CmdletBinding()]
param([switch]$PreserveLegacy, [string]$Model = $env:BRAIN_MODEL, [string]$Token = $env:BRAIN_LOCK_TOKEN)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$target = Join-Path $root 'protocol/decision-authority.json'
if (Test-Path -LiteralPath $target) { throw 'Authority boundary already exists; it is immutable.' }
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'state-lock.ps1') -CheckOwner -Model $Model -Token $Token
if ($LASTEXITCODE -ne 0) { throw 'Own STATE.lock before initializing authority.' }
$log = Join-Path $root 'DECISIONS.md'
if (-not (Test-Path -LiteralPath $log)) { throw 'DECISIONS.md is missing; bootstrap first.' }
$raw = [IO.File]::ReadAllText($log)
$ids = @([regex]::Matches($raw, '(?m)^## D(\d+)\b') | ForEach-Object { [int]$_.Groups[1].Value })
$next = 1
if ($ids.Count) {
  if (-not $PreserveLegacy) { throw 'Existing history: review it, then explicitly use -PreserveLegacy; no old entry is ratified.' }
  # Only committed, unchanged history may be preserved: tracked in HEAD and identical in index and tree.
  & git -C $root cat-file -e HEAD:DECISIONS.md 2>$null
  $committed = $LASTEXITCODE -eq 0
  if ($committed) { & git -C $root diff --quiet HEAD -- DECISIONS.md; $committed = $LASTEXITCODE -eq 0 }
  if (-not $committed) { throw 'Commit existing history unchanged before migration.' }
  $next = [int](($ids | Measure-Object -Maximum).Maximum) + 1
}
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-decision-history.ps1') -Path $log -AuthorityFrom $next
if ($LASTEXITCODE -ne 0) { throw 'Existing history is structurally invalid; migration refused.' }
# Same compact form bootstrap-clone.ps1 writes for a new empty log.
$policy = [ordered]@{schema_version=1; authority_from=$next} | ConvertTo-Json -Compress
[IO.File]::WriteAllText($target, "$policy`n", [Text.UTF8Encoding]::new($false))
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'build-rulings.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Boundary written but rulings failed; inspect before committing.' }
Write-Host "AUTHORITY_INITIALIZED from=D$next legacy=unclassified"
