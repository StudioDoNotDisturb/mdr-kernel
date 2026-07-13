#requires -Version 7.0
<#
.SYNOPSIS
  Structural checks for every shipped skill: YAML frontmatter must open at byte one (the
  convention documented in reference/markdown-system-node-standard.md and required by
  skill-loading harnesses), carry the envelope fields plus the skill metadata keys, and pair
  with a trigger-eval file shaped for should_trigger/should_not_trigger checks.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$failures = 0

function Expect([string]$Name, [bool]$Condition, [string]$Detail = '') {
  if ($Condition) { Write-Host "[PASS] $Name" }
  else {
    Write-Host "[FAIL] $Name$(if ($Detail) { " - $Detail" })"
    $script:failures++
  }
}

function Test-SkillFile([string]$Path, [string]$Label) {
  $bytes = [IO.File]::ReadAllBytes($Path)
  $hasBom = $bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF
  Expect "$Label has no BOM" (-not $hasBom) 'a leading BOM would shift byte one off the ---'

  $text = [IO.File]::ReadAllText($Path)
  Expect "$Label frontmatter opens at byte one" ($text.StartsWith('---')) "first bytes: '$($text.Substring(0, [Math]::Min(10, $text.Length)))'"

  $fm = [regex]::Match($text, '(?s)^---\r?\n(.*?)\r?\n---\r?\n')
  Expect "$Label frontmatter block closes" $fm.Success
  if (-not $fm.Success) { return }
  $body = $fm.Groups[1].Value

  foreach ($field in 'Title', 'Type', 'Context', 'name', 'description', 'compatibility') {
    $found = [regex]::IsMatch($body, "(?m)^${field}:")
    Expect "$Label frontmatter has '$field'" $found
  }
}

Test-SkillFile (Join-Path $root 'skills/mdr-kernel/SKILL.md') 'skills/mdr-kernel/SKILL.md'
Test-SkillFile (Join-Path $root '_templates/skill-template.md') '_templates/skill-template.md'

$evalPath = Join-Path $root 'skills/mdr-kernel/evals/trigger-queries.json'
Expect 'trigger-queries.json exists' (Test-Path -LiteralPath $evalPath)
if (Test-Path -LiteralPath $evalPath) {
  $evalJson = Get-Content -Raw -LiteralPath $evalPath | ConvertFrom-Json
  foreach ($section in 'should_trigger', 'should_not_trigger', 'output_quality_assertions') {
    $has = $null -ne $evalJson.$section
    Expect "trigger-queries.json has '$section'" $has
    if ($has -and $section -ne 'output_quality_assertions') {
      Expect "trigger-queries.json '$section' is non-empty" (@($evalJson.$section).Count -gt 0)
      foreach ($entry in @($evalJson.$section)) {
        Expect "trigger-queries.json '$section' entry has query+why" ($entry.query -and $entry.why) ($entry | ConvertTo-Json -Compress)
      }
    }
  }
}

if ($failures) { Write-Host "skill-structure: $failures failure(s)"; exit 1 }
Write-Host 'skill-structure: PASS'
