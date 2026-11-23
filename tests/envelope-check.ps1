#requires -Version 7.0
<#
.SYNOPSIS
  Tests for bin/envelope-check.ps1: the limits at their edges, sentence counting past
  abbreviations, YAML front matter, and the staged pre-commit mode in a throwaway repo.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$checker = Join-Path $root 'bin/envelope-check.ps1'
$failures = 0
# MDR_TEST_ENGINE names a built binary: run every case through `mdr-kernel envelope` instead, so
# the native port is held to the same fixtures as the PowerShell checker.
$engine = $env:MDR_TEST_ENGINE
function Invoke-Checker([string]$File, [switch]$Staged, [switch]$Count) {
  $mode = if ($Staged) { 'staged' } elseif ($Count) { 'count' } else { 'check' }
  if ($engine) { & $engine envelope $mode @(if ($File) { $File }) }
  else { & pwsh -NoProfile -File $checker @(if ($Staged) { '-Staged' } elseif ($Count) { '-Count' } else { $File }) }
}

function Expect([string]$Name, [bool]$Condition, [string]$Detail = '') {
  if ($Condition) { Write-Host "[PASS] $Name" }
  else {
    Write-Host "[FAIL] $Name$(if ($Detail) { " - $Detail" })"
    $script:failures++
  }
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) "envelope-check-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $tmp | Out-Null
function Doc([string]$Title, [string]$Context) { "Title: $Title`nType: Test`nContext: $Context`n`n# T`n`nbody`n" }
function Check([string]$Text) {
  $f = Join-Path $tmp 'case.md'
  [IO.File]::WriteAllText($f, $Text)
  $null = Invoke-Checker $f
  $LASTEXITCODE
}

try {
  $good = 'Use when checking the envelope checker against its own fixtures.'
  Expect 'compliant header passes' ((Check (Doc 'A short name' $good)) -eq 0)
  Expect 'title at 100 chars passes' ((Check (Doc ('x' * 100) $good)) -eq 0)
  Expect 'title at 101 chars fails' ((Check (Doc ('x' * 101) $good)) -eq 1)
  Expect 'context at 300 chars passes' ((Check (Doc 'T' ('Use ' + 'x' * 295 + '.'))) -eq 0)
  Expect 'context at 301 chars fails' ((Check (Doc 'T' ('Use ' + 'x' * 296 + '.'))) -eq 1)
  Expect 'two short sentences fail' ((Check (Doc 'T' 'Use when testing. Written 2026-09-25.')) -eq 1)
  Expect 'abbreviations and initials do not split a sentence' ((Check (Doc 'T' "Use when comparing e.g. No. 0412 vs. the U.S. plan; see p. 4 of Dr. Smith's note.")) -eq 0)
  Expect 'decimals do not split a sentence' ((Check (Doc 'T' 'Version 2.5 of the file, at 3.14 scale.')) -eq 0)
  Expect 'missing envelope is not this check''s signal' ((Check "# No envelope`n`nbody`n") -eq 0)
  Expect 'a wrapped Context is joined and checked' ((Check "Title: T`nType: Test`nContext: Use when testing a Context that`nwraps onto a second line. And a third sentence.`n`n# T`n") -eq 1)
  Expect 'text after the blank line is body, not Context' ((Check "Title: T`nType: Test`nContext: Use when testing.`n`nBody text. More text.`n") -eq 0)
  Expect 'YAML front matter is checked' ((Check "---`nTitle: `"$('y' * 120)`"`nType: Skill`nContext: One. Two.`nname: x`n---`n`n# S`n") -eq 1)
  Expect 'compliant YAML front matter passes' ((Check "---`nTitle: 'Short'`nType: Skill`nContext: 'Use when testing.'`n---`n") -eq 0)

  $repo = Join-Path $tmp 'repo'
  New-Item -ItemType Directory -Path $repo | Out-Null
  Push-Location $repo
  try {
    git init -q
    [IO.File]::WriteAllText((Join-Path $repo 'ok.md'), (Doc 'Short' $good))
    git add ok.md
    $null = Invoke-Checker -Staged
    Expect 'staged: compliant file passes' ($LASTEXITCODE -eq 0)
    [IO.File]::WriteAllText((Join-Path $repo 'bad.md'), (Doc ('z' * 150) 'Written today. Nothing here is doctrine.'))
    git add bad.md
    $null = Invoke-Checker -Staged
    Expect 'staged: overlong header blocks the commit' ($LASTEXITCODE -eq 1)
    $out = Invoke-Checker -Count
    Expect 'count reports tracked files over the limits' ("$out" -match 'ENVELOPE_LENGTH over=1 tracked=2') "$out"
    [IO.File]::WriteAllText((Join-Path $repo 'bad.md'), (Doc 'Short' $good))
    $null = Invoke-Checker -Staged
    Expect 'staged: an unstaged fix cannot mask the staged blob' ($LASTEXITCODE -eq 1)
  } finally { Pop-Location }
} finally {
  Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

if ($failures) { Write-Host "envelope-check: $failures failure(s)"; exit 1 }
Write-Host 'envelope-check: all checks passed'
