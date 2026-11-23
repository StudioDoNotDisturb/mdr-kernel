#requires -Version 7.0
<#
.SYNOPSIS
  One-shot STATE.md size-budget report. Prints a byte-exact verdict with headroom/overage,
  and - when over budget or with -Detail - the per-section sizes and the fattest lines, so an
  over-budget STATE costs one targeted edit instead of an edit/measure/re-read trim loop at
  session close.

  The first output line is self-contained (close-session and humans can relay it verbatim).
  Exit codes: 0 within budget, 1 over budget, 2 file missing.
.EXAMPLE
  pwsh -NoProfile -File bin/state-budget.ps1           # verdict + headroom; breakdown only when over
  pwsh -NoProfile -File bin/state-budget.ps1 -Detail   # always show the breakdown
#>
[CmdletBinding()]
param(
  [string]$Path,                # defaults to <repo>/STATE.md
  [int]$MaxLines = 90,          # keep in step with brain-doctor -StateMaxLines
  [int]$MaxKB = 10,             # keep in step with brain-doctor -StateMaxKB
  [int]$Top = 5,                # fattest lines to list in the breakdown
  [switch]$Detail               # show the breakdown even when within budget
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same report when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key)"; "$($p.Value)" }
  })
  & $engine state-budget @forward
  exit $LASTEXITCODE
}
if (-not $Path) { $Path = Join-Path $root 'STATE.md' }
if (-not (Test-Path -LiteralPath $Path)) { Write-Host "[FAIL] not found: $Path"; exit 2 }

$maxBytes = $MaxKB * 1024
$bytes    = (Get-Item -LiteralPath $Path).Length
$raw      = Get-Content -Raw -LiteralPath $Path
$lines    = @(Get-Content -LiteralPath $Path)   # same line counting as brain-doctor
$name     = Split-Path -Leaf $Path
$nlBytes  = 1; if ($raw -match "`r`n") { $nlBytes = 2 }

$overBytes = $bytes - $maxBytes
$overLines = $lines.Count - $MaxLines
$over = ($overBytes -gt 0) -or ($overLines -gt 0)

if ($over) {
  $parts = @()
  if ($overBytes -gt 0) { $parts += "$overBytes bytes" }
  if ($overLines -gt 0) { $parts += "$overLines lines" }
  Write-Host ("OVER: {0} is {1} bytes / {2} lines (budget {3} / {4}) - trim {5}" -f `
    $name, $bytes, $lines.Count, $maxBytes, $MaxLines, ($parts -join ' + '))
} else {
  Write-Host ("OK: {0} is {1} bytes / {2} lines (budget {3} / {4}) - headroom {5} bytes / {6} lines" -f `
    $name, $bytes, $lines.Count, $maxBytes, $MaxLines, (-$overBytes), (-$overLines))
}

if ($over -or $Detail) {
  # Per-line UTF-8 cost (content + newline) so the breakdown points at real file bytes.
  $utf8 = [Text.Encoding]::UTF8
  $lineBytes = New-Object 'int[]' $lines.Count
  for ($i = 0; $i -lt $lines.Count; $i++) { $lineBytes[$i] = $utf8.GetByteCount([string]$lines[$i]) + $nlBytes }

  Write-Host "`nSections (largest first):"
  $sections = @()
  $cur = New-Object psobject -Property @{ Name = '(head)'; Start = 1; Bytes = 0; Lines = 0 }
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^##\s+(.+)$') {
      if ($cur.Lines -gt 0) { $sections += $cur }
      $cur = New-Object psobject -Property @{ Name = ('## ' + $Matches[1]); Start = ($i + 1); Bytes = 0; Lines = 0 }
    }
    $cur.Bytes += $lineBytes[$i]; $cur.Lines += 1
  }
  if ($cur.Lines -gt 0) { $sections += $cur }
  foreach ($s in ($sections | Sort-Object Bytes -Descending)) {
    Write-Host ("  {0,6} B  {1,3} L  L{2}-L{3}  {4}" -f $s.Bytes, $s.Lines, $s.Start, ($s.Start + $s.Lines - 1), $s.Name)
  }

  Write-Host "`nFattest lines (top $Top):"
  $fattest = 0..($lines.Count - 1) | Sort-Object { -$lineBytes[$_] } | Select-Object -First $Top
  foreach ($i in $fattest) {
    $preview = [string]$lines[$i]
    if ($preview.Length -gt 72) { $preview = $preview.Substring(0, 72) + '...' }
    Write-Host ("  L{0,-4} {1,6} B  {2}" -f ($i + 1), $lineBytes[$i], $preview)
  }
  if ($over) {
    Write-Host "`nTrim in ONE pass: compress the fattest rows above / prune pointers a handoff already"
    Write-Host "records, then re-run this script once - the verdict line is byte-exact."
  }
}
exit ([int]$over)
