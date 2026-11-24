#requires -Version 7.0
<#
.SYNOPSIS
  Safely prepend a newest-first entry to DECISIONS.md.
.DESCRIPTION
  Every new entry's body must carry `**Authority:** operator-ruled` (plus an
  `**Operator said (<surface>, YYYY-MM-DD):** "<quote>"` line) or `**Authority:** proposed`.
  Optional `**Supersedes:**`, `**Supersedes in part:**` and `**Retires:**` lines list older ids; each
  target is stamped `_Status: <kind> by D### (YYYY-MM-DD)_` in the same atomic write.
  The checker validates the whole result and the write is rolled back if it fails. The file's
  line-ending style (CRLF or LF) and UTF-8 BOM are preserved. The canonical DECISIONS.md needs your
  own STATE.lock (-Model/-LockToken or BRAIN_MODEL/BRAIN_LOCK_TOKEN) and then rebuilds
  protocol/rulings.json; a -Path fixture needs no lock and never touches rulings.
.EXAMPLE
  pwsh -NoProfile -File bin/add-decision.ps1 -Title 'Adopt the bounded control' -BodyPath .\decision-body.md
#>
[CmdletBinding(DefaultParameterSetName = 'Body')]
param(
  [Parameter(Mandatory = $true)]
  [string]$Title,
  [Parameter(Mandatory = $true, ParameterSetName = 'Body')]
  [string]$Body,
  [Parameter(Mandatory = $true, ParameterSetName = 'BodyPath')]
  [string]$BodyPath,
  [string]$Path,
  [string]$Model = $env:BRAIN_MODEL,
  [string]$LockToken = $env:BRAIN_LOCK_TOKEN,
  [int]$AuthorityFrom = 1
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same command when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine add-decision @forward
  exit $LASTEXITCODE
}
$canonicalPath = Join-Path $root 'DECISIONS.md'
if (-not $Path) { $Path = $canonicalPath }
$target = [IO.Path]::GetFullPath($Path)
$isCanonical = $target.Equals([IO.Path]::GetFullPath($canonicalPath), [StringComparison]::OrdinalIgnoreCase)

if ($Title -match '[\r\n]' -or [string]::IsNullOrWhiteSpace($Title)) {
  throw 'Title must be one non-empty line.'
}
$bodyText = if ($PSCmdlet.ParameterSetName -eq 'BodyPath') {
  Get-Content -Raw -LiteralPath $BodyPath
} else { $Body }
if ([string]::IsNullOrWhiteSpace($bodyText)) { throw 'Decision body must not be empty.' }
if ($bodyText -notmatch '(?m)^\*\*Decision(?: \([^\r\n]+\))?:\*\*') {
  throw 'Decision body must contain a top-level **Decision:** marker.'
}

if ($isCanonical) {
  if ([string]::IsNullOrWhiteSpace($Model)) { throw 'BRAIN_MODEL or -Model is required for a canonical decision append.' }
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'state-lock.ps1') -CheckOwner -Model $Model -Token $LockToken
  if ($LASTEXITCODE -ne 0) { throw 'Your sealed STATE.lock is required before editing DECISIONS.md.' }
}

$checker = Join-Path $PSScriptRoot 'check-decision-history.ps1'
& pwsh -NoProfile -File $checker -Path $target -AuthorityFrom $AuthorityFrom
if ($LASTEXITCODE -ne 0) { throw 'Existing decision history is invalid; append refused.' }

$originalBytes = [IO.File]::ReadAllBytes($target)
$hasBom = $originalBytes.Length -ge 3 -and $originalBytes[0] -eq 0xEF -and $originalBytes[1] -eq 0xBB -and $originalBytes[2] -eq 0xBF
$offset = if ($hasBom) { 3 } else { 0 }
$utf8 = [Text.UTF8Encoding]::new($false, $true)
$raw = $utf8.GetString($originalBytes, $offset, $originalBytes.Length - $offset)
$first = [regex]::Match($raw, '(?m)^## D(?<id>\d+)\b[^\r\n]*')
$nextId = if ($first.Success) { [int]$first.Groups['id'].Value + 1 } else { 1 }
$eol = if ($raw.Contains("`r`n")) { "`r`n" } else { "`n" }
$normalizedBody = ($bodyText -replace "`r`n|`r|`n", $eol).TrimEnd("`r", "`n")
$today = [DateTime]::UtcNow.ToString('yyyy-MM-dd')
$entry = "## D$nextId - $today - $($Title.Trim())$eol$eol$normalizedBody$eol$eol"
$updated = if ($first.Success) { $raw.Insert($first.Index, $entry) } else {
  $marker = $raw.IndexOf('_No decisions yet._')
  if ($marker -lt 0) { throw 'Empty log marker missing.' }
  $raw.Remove($marker, '_No decisions yet._'.Length).Insert($marker, $entry.TrimEnd())
}

# Stamp every target named in a supersession field, directly under its heading line.
$fieldKinds = [ordered]@{ 'Supersedes' = 'superseded'; 'Supersedes in part' = 'superseded in part'; 'Retires' = 'retired' }
foreach ($field in $fieldKinds.Keys) {
  $fieldLine = [regex]::Match($normalizedBody, "(?m)^\*\*$([regex]::Escape($field)):\*\*[ \t]*(?<v>[^\r\n]*?)[ \t]*\r?$")
  if (-not $fieldLine.Success) { continue }
  foreach ($targetId in @([regex]::Matches($fieldLine.Groups['v'].Value, 'D(\d+)') | ForEach-Object { [int]$_.Groups[1].Value } | Sort-Object -Unique)) {
    $heading = [regex]::Match($updated, "(?m)^## D$targetId\b[^\r\n]*\r?\n")
    if (-not $heading.Success) { throw "**${field}:** names D$targetId, which does not exist." }
    $updated = $updated.Insert($heading.Index + $heading.Length, "_Status: $($fieldKinds[$field]) by D$nextId ($today)_$eol")
  }
}
$payload = $utf8.GetBytes($updated)
if ($hasBom) {
  $withBom = [byte[]]::new($payload.Length + 3)
  [Array]::Copy([byte[]](0xEF, 0xBB, 0xBF), $withBom, 3)
  [Array]::Copy($payload, 0, $withBom, 3, $payload.Length)
  $payload = $withBom
}

$temp = Join-Path (Split-Path -Parent $target) ".DECISIONS.$([Guid]::NewGuid().ToString('N')).tmp"
$backup = Join-Path (Split-Path -Parent $target) ".DECISIONS.$([Guid]::NewGuid().ToString('N')).bak"
try {
  [IO.File]::WriteAllBytes($temp, $payload)
  [IO.File]::Replace($temp, $target, $backup)
  & pwsh -NoProfile -File $checker -Path $target -AuthorityFrom $AuthorityFrom
  if ($LASTEXITCODE -ne 0) { throw 'Appended decision failed verification (structure, authority or supersession); rolled back.' }
} catch {
  if (Test-Path -LiteralPath $backup) { [IO.File]::Copy($backup, $target, $true) }
  else { [IO.File]::WriteAllBytes($target, $originalBytes) }
  throw
} finally {
  if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Force }
  if (Test-Path -LiteralPath $backup) { Remove-Item -LiteralPath $backup -Force }
}

Write-Host "DECISION_APPEND_STATUS=OK id=D$nextId path=$target"
# Keep protocol/rulings.json current for the canonical log (fixtures are left alone).
if ($isCanonical) {
  & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'build-rulings.ps1') | Out-Host
  if ($LASTEXITCODE -ne 0) { Write-Warning 'protocol/rulings.json was not rebuilt; run bin/build-rulings.ps1 before committing.' }
}
