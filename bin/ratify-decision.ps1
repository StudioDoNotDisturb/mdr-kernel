#requires -Version 7.0
<#
.SYNOPSIS
  Operator-only: ratify a DECISIONS.md entry by recording its exact bytes in
  protocol/decision-ratifications.jsonl.
.DESCRIPTION
  Run this yourself, in your own interactive terminal. It refuses when standard input is
  redirected, when the process is non-interactive, or when PowerShell was started with
  -NonInteractive - which is how model tool shells run - and it asks you to type the exact
  confirmation phrase. The record binds the entry's SHA-256 (the same hash
  `bin/check-decision-history.ps1 -HashOf` prints), so an entry edited later loses its
  ratification loudly at Doctor and at commit.

  This is a cooperative boundary on a shared Windows account, not a security guarantee: a model
  that drives your terminal, or hand-writes the JSONL line, defeats it deliberately and visibly.
  Nothing here commits; the next session's close commits the file (it is a coordination file).
.EXAMPLE
  pwsh -File bin/ratify-decision.ps1 -Id D1
.EXAMPLE
  pwsh -File bin/ratify-decision.ps1 -Id D1 -Quote 'yes, that is my ruling'
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$Id,
  [string]$Quote,
  [string]$Model = $env:BRAIN_MODEL,
  [string]$Token = $env:BRAIN_LOCK_TOKEN,
  [ValidateSet('terminal', 'chat', 'operator terminal', 'in person', 'email', 'phone')]
  [string]$Surface = 'terminal'
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$ledger = Join-Path $root 'protocol/decision-ratifications.jsonl'

$reasons = @()
if ([Console]::IsInputRedirected) { $reasons += 'standard input is redirected' }
if (-not [Environment]::UserInteractive) { $reasons += 'the process is not interactive' }
if (@([Environment]::GetCommandLineArgs() | Where-Object { $_ -match '^-noni(nteractive)?$' }).Count) { $reasons += 'PowerShell was started with -NonInteractive' }
if ($reasons.Count) {
  Write-Host 'RATIFICATION_STATUS=REFUSED'
  Write-Host "REASON=$($reasons -join '; ')"
  Write-Host 'Ratification is an operator act. Run this command yourself in your own terminal.'
  exit 3
}

if ($Id -notmatch '^D?(\d+)$') { throw "Id must look like D123, not '$Id'." }
$Id = "D$($Matches[1])"
$hashOut = @(& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-decision-history.ps1') -HashOf $Id 2>&1)
$hashLine = $hashOut | Where-Object { $_ -match '^ENTRY_SHA256=' } | Select-Object -First 1
$headLine = $hashOut | Where-Object { $_ -match '^ENTRY_HEADING=' } | Select-Object -First 1
if (-not $hashLine) { $hashOut | ForEach-Object { Write-Host $_ }; throw "Could not hash $Id." }
$sha = $hashLine.Substring('ENTRY_SHA256='.Length)
$heading = $headLine.Substring('ENTRY_HEADING='.Length)

if (Test-Path -LiteralPath $ledger) {
  foreach ($existing in (Get-Content -LiteralPath $ledger | Where-Object { $_.Trim() })) {
    $rec = $existing | ConvertFrom-Json
    if ($rec.id -eq $Id -and $rec.entry_sha256 -eq $sha) {
      Write-Host "RATIFICATION_STATUS=ALREADY id=$Id ratified_at=$($rec.ratified_at)"
      exit 0
    }
  }
}

Write-Host ''
Write-Host "  $heading"
Write-Host "  SHA-256 $sha"
Write-Host ''
if (-not $PSBoundParameters.ContainsKey('Quote')) {
  $Quote = Read-Host 'Your own words for the record (optional; press Enter to skip)'
}
$phrase = "RATIFY $Id"
$typed = Read-Host "Type '$phrase' to ratify this exact entry"
if ($typed -cne $phrase) {
  Write-Host 'RATIFICATION_STATUS=CANCELLED (confirmation phrase did not match; nothing written)'
  exit 1
}

& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'state-lock.ps1') -CheckOwner -Model $Model -Token $Token
if ($LASTEXITCODE -ne 0) { throw 'Own STATE.lock in your operator terminal before recording ratification.' }
$latestHash = @(& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-decision-history.ps1') -HashOf $Id)
if ($LASTEXITCODE -ne 0 -or $latestHash -notcontains "ENTRY_SHA256=$sha") { throw 'Entry changed while you reviewed it; rerun and review the new bytes.' }
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-decision-history.ps1')
if ($LASTEXITCODE -ne 0) { throw 'Decision history is invalid; no ratification written.' }

$record = [ordered]@{
  id           = $Id
  entry_sha256 = $sha
  ratified_at  = [DateTimeOffset]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
  quote        = [string]$Quote
  surface      = $Surface
} | ConvertTo-Json -Compress
$utf8 = [Text.UTF8Encoding]::new($false)
$prefix = ''
if (Test-Path -LiteralPath $ledger) {
  $current = [IO.File]::ReadAllText($ledger)
  if ($current.Length -and -not $current.EndsWith("`n")) { $prefix = "`n" }
}
[IO.File]::AppendAllText($ledger, "$prefix$record`n", $utf8)

& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'check-decision-history.ps1') *> $null
if ($LASTEXITCODE -ne 0) { Write-Host 'WARNING: the decision checker now fails; run bin/check-decision-history.ps1 to see why.' }
& pwsh -NoProfile -File (Join-Path $PSScriptRoot 'build-rulings.ps1') *> $null
if ($LASTEXITCODE -ne 0) { Write-Host 'WARNING: protocol/rulings.json was not rebuilt; the next session must run bin/build-rulings.ps1.' }
Write-Host "RATIFICATION_STATUS=RECORDED id=$Id sha256=$sha"
Write-Host 'The next session commits protocol/decision-ratifications.jsonl at its close.'
