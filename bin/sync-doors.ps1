#requires -Version 7.0
<#
.SYNOPSIS
  Propagate the canonical contract blocks from protocol/CONTRACT.md into the front doors.
.DESCRIPTION
  Two blocks are generated:
    BRAIN-CONTRACT  -> CLAUDE.md, AGENTS.md, GEMINI.md   (the CLI-agent front doors)
    BRAIN-ADVISORY  -> protocol/advisory.md              (paste into a chat app's custom instructions)
  Default: rewrite drifted targets in place. -Check: report drift and exit 1 if any target is out
  of sync (used by brain-doctor). If a target file is missing, it is created with the canonical
  block. -Check -Staged: validate the git INDEX instead of the working tree — a commit snapshots
  the index, so a partially staged contract/door set can be in-sync on disk yet drifted in the
  commit; the pre-commit hook uses this mode.
#>
[CmdletBinding()]
param([switch]$Check, [switch]$Staged)
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same sync when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine sync-doors @forward
  exit $LASTEXITCODE
}
if ($Staged -and -not $Check) { [Console]::Error.WriteLine('-Staged validates the commit snapshot and requires -Check'); exit 2 }
# show-prefix keeps :<path> index lookups correct when this kernel lives in a subdirectory of a
# larger repository (e.g. the template's own development tree).
$indexPrefix = ''
if ($Staged) { $indexPrefix = ((& git -C $root rev-parse --show-prefix 2>$null) | Out-String).Trim() }

function Read-Norm([string]$p) { [IO.File]::ReadAllText($p) -replace "`r`n", "`n" }
# Returns the content the check should judge ($null = missing): working tree by default, the
# staged index blob with -Staged.
function Read-Source([string]$rel) {
  if (-not $Staged) {
    $p = Join-Path $root $rel
    if (-not (Test-Path $p)) { return $null }
    return Read-Norm $p
  }
  $blob = & git -C $root show ":$indexPrefix$rel" 2>$null
  if ($LASTEXITCODE -ne 0) { return $null }
  return ((@($blob) -join "`n") -replace "`r", '')
}
function Write-Lf([string]$p, [string]$s) {
  $dir = Split-Path -Parent $p
  if ($dir -and -not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
  [IO.File]::WriteAllText($p, ($s -replace "`r`n", "`n"), (New-Object System.Text.UTF8Encoding($false)))
}

# A minimal door wrapper used when a target file doesn't exist yet.
function New-DoorShell([string]$title, [string]$block, [string]$target) {
  if ($target -eq 'protocol/advisory.md') {
    return @"
Title: Advisory door
Type: Generated Advisory Instructions / Front Door
Context: Generated advisory instructions for chat surfaces that cannot read the repo or run git.

# $title

_This file is generated from protocol/CONTRACT.md by bin/sync-doors.ps1. Edit the
contract, then run sync - do not edit this copy._

$block

---

## SYSTEM NODE LINKS

Directory Path: protocol/
Next Sequential File: CONTRACT.md
Related Workflows: CONTRACT.md, ../bin/sync-doors.ps1
AI Execution Prompts: CONTRACT.md
"@
  }

  $doorTitle = "$target - mdr-kernel front door"
  return @"
Title: $doorTitle
Type: Generated Agent Front Door
Context: Generated CLI-agent instructions that mirror the canonical mdr-kernel contract.

# $doorTitle

_This file is generated from protocol/CONTRACT.md by bin/sync-doors.ps1. Edit the
contract, then run sync - do not edit this copy._

$block

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: protocol/CONTRACT.md
Related Workflows: protocol/CONTRACT.md, bin/sync-doors.ps1
AI Execution Prompts: protocol/CONTRACT.md
"@
}

$contractPath = Join-Path $root 'protocol/CONTRACT.md'
$contract = Read-Source 'protocol/CONTRACT.md'
if ($null -eq $contract) { [Console]::Error.WriteLine("CONTRACT not found: $contractPath"); exit 2 }

# Block name -> (regex, target files relative to root, shell title if the file must be created)
$blocks = @(
  @{ Name = 'BRAIN-CONTRACT'; Targets = @('CLAUDE.md', 'AGENTS.md', 'GEMINI.md'); Title = 'Front door' },
  @{ Name = 'BRAIN-ADVISORY'; Targets = @('protocol/advisory.md');               Title = 'Advisory door' }
)

$drift   = New-Object System.Collections.Generic.List[string]
$changed = New-Object System.Collections.Generic.List[string]

foreach ($b in $blocks) {
  $rx = "(?s)<!-- BEGIN $($b.Name).*?<!-- END $($b.Name) -->"
  $cm = [regex]::Match($contract, $rx)
  if (-not $cm.Success) { [Console]::Error.WriteLine("No $($b.Name) markers in protocol/CONTRACT.md"); exit 2 }
  $canonical = $cm.Value
  foreach ($t in $b.Targets) {
    $p = Join-Path $root $t
    $txt = Read-Source $t
    if ($null -eq $txt) {
      if ($Check) { $drift.Add("$t (missing)"); continue }
      Write-Lf $p (New-DoorShell $b.Title $canonical $t); $changed.Add("$t (created)"); continue
    }
    $tm  = [regex]::Match($txt, $rx)
    if (-not $tm.Success) {
      if ($Check) { $drift.Add("$t (no $($b.Name) markers)") }
      else { Write-Lf $p (New-DoorShell $b.Title $canonical $t); $changed.Add("$t (recreated)") }
      continue
    }
    if ($tm.Value -cne $canonical) {  # case-sensitive: a case-only contract edit is drift
      if ($Check) { $drift.Add($t) }
      else { Write-Lf $p ($txt.Remove($tm.Index, $tm.Length).Insert($tm.Index, $canonical)); $changed.Add($t) }
    }
  }
}

if ($Check) {
  if ($drift.Count -gt 0) {
    Write-Host "TARGETS OUT OF SYNC: $($drift -join ', ')"
    Write-Host "Fix: pwsh -NoProfile -File bin/sync-doors.ps1"
    exit 1
  }
  Write-Host "doors + advisory in sync with protocol/CONTRACT.md"
  exit 0
}

if ($changed.Count -gt 0) { Write-Host "Updated: $($changed -join ', ')" } else { Write-Host "All targets already in sync." }
exit 0
