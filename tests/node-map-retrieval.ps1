#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$failures = 0

function Assert-True([string]$Name, [bool]$Condition, [string]$Detail = '') {
  if ($Condition) { Write-Host "[PASS] $Name"; return }
  Write-Host "[FAIL] $Name$(if ($Detail) { ": $Detail" })"
  $script:failures++
}

function Write-Utf8([string]$Path, [string]$Content) {
  $parent = Split-Path -Parent $Path
  if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Path $parent | Out-Null }
  [IO.File]::WriteAllText($Path, $Content, [Text.UTF8Encoding]::new($false))
}

$sandbox = Join-Path ([IO.Path]::GetTempPath()) "mdr-node-map-retrieval-$([guid]::NewGuid().ToString('N'))"
$clone = Join-Path $sandbox 'clone'
try {
  New-Item -ItemType Directory -Path $sandbox | Out-Null
  & git clone -q --no-hardlinks $root $clone
  if ($LASTEXITCODE) { throw 'could not create retrieval test clone' }
  & git -C $clone remote remove origin
  # MDR_TEST_ENGINE names a built binary: the clone gets it in bin/, so setup, close, the doctor and
  # the node map run the native engine, and every assertion must hold for it as for PowerShell.
  if ($env:MDR_TEST_ENGINE) { Copy-Item -LiteralPath $env:MDR_TEST_ENGINE -Destination (Join-Path $clone "bin/mdr-kernel$(if ($IsWindows) { '.exe' })") }
  & git -C $clone config user.name 'node-map retrieval test'
  & git -C $clone config user.email 'node-map@example.invalid'
  & pwsh -NoProfile -File (Join-Path $clone 'bin/setup-private.ps1') -Model 'test-model' -LocalOnly | Out-Host
  if ($LASTEXITCODE) { throw 'local-only setup failed' }

  $targetRel = 'specs/cp3-target.md'
  $sourceRel = 'docs/cp3-source.md'
  Write-Utf8 (Join-Path $clone $targetRel) @'
Title: CP3 Target
Type: Test Fixture
Context: Target for source-relative retrieval coordinates.

# CP3 Target

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: none
Related Workflows: none
AI Execution Prompts: none
'@
  Write-Utf8 (Join-Path $clone $sourceRel) @'
Title: CP3 Source
Type: Test Fixture
Context: Source for source-relative retrieval coordinates.

# CP3 Source

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: ../specs/cp3-target.md
Related Workflows: ../specs/cp3-target.md
AI Execution Prompts: none
'@

  $oldModel = $env:BRAIN_MODEL
  $env:BRAIN_MODEL = 'test-model'
  try {
    $closeOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also "$sourceRel,$targetRel" 2>&1)
    $closeExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ($closeExit -ne 0) { $closeOut | Out-Host }
  Assert-True 'close stages new Markdown before node-map generation' ($closeExit -eq 0) "exit $closeExit"

  $mapPath = Join-Path $clone 'maps/node-map.json'
  $map = Get-Content -Raw -LiteralPath $mapPath | ConvertFrom-Json
  $sourceNode = @($map.files | Where-Object { $_.relPath -eq $sourceRel })[0]
  Assert-True 'new Markdown is recorded as tracked retrieval truth' ($null -ne $sourceNode -and $sourceNode.tracked -eq $true)
  Assert-True 'source-relative link stores its canonical root coordinate' ($sourceNode.resolvedLinks.nextSequentialFile -eq $targetRel) ([string]$sourceNode.resolvedLinks.nextSequentialFile)

  $bridge = Get-Content -Raw -LiteralPath (Join-Path $clone 'maps/OBSIDIAN-NODE-LINKS.md')
  $rightCoordinate = $bridge -match [regex]::Escape("[$targetRel](../$targetRel)")
  $escapedRoot = $bridge -match [regex]::Escape("[$targetRel](../../$targetRel)")
  Assert-True 'exported bridge links stay inside the repository' ($rightCoordinate -and -not $escapedRoot)

  $skillNode = @($map.files | Where-Object { $_.relPath -eq '_templates/skill-template.md' })[0]
  $waiver = 'broken-link:skills/<skill-name>/'
  Assert-True 'intentional skill-template placeholder is explicitly waived' (
    $null -ne $skillNode -and $skillNode.issues.Count -eq 0 -and $skillNode.waivedIssues -contains $waiver
  )
  $markdownTemplate = @($map.files | Where-Object { $_.relPath -eq '_templates/markdown-file-template.md' })[0]
  Assert-True 'instructional node-field placeholders are explicitly waived' (
    $null -ne $markdownTemplate -and $markdownTemplate.issues.Count -eq 0 -and $markdownTemplate.waivedIssues.Count -eq 4
  )

  $checkOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/update-node-map.ps1') -Check 2>&1) | Out-String
  Assert-True 'generated maps pass -Check right after close' ($LASTEXITCODE -eq 0 -and $checkOut -match 'node-map check passed') $checkOut

  $sourcePath = Join-Path $clone $sourceRel
  $changed = (Get-Content -Raw -LiteralPath $sourcePath).Replace(
    'Context: Source for source-relative retrieval coordinates.',
    'Context: Changed after generation; doctor must detect exact drift.'
  )
  Write-Utf8 $sourcePath $changed
  $doctorOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/brain-doctor.ps1') 2>&1)
  $doctorExit = $LASTEXITCODE
  $doctorText = $doctorOut | Out-String
  Assert-True 'doctor detects exact node-map parity drift' (
    $doctorExit -eq 0 -and $doctorText -match 'node map differs from tracked Markdown' -and $doctorText -match 'bin/update-node-map.ps1'
  ) "exit $doctorExit"

  $auditPath = Join-Path $clone 'inbox/write-approval-audit.jsonl'
  Write-Utf8 $auditPath "{}\n"
  $auditDoctor = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/brain-doctor.ps1') 2>&1) | Out-String
  Assert-True 'durable write-approval audit log is not an inbox note' ($auditDoctor -notmatch 'inbox/:\s+\d+ note')
} finally {
  if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force }
}

if ($failures) { Write-Host "node-map-retrieval: $failures failure(s)"; exit 1 }
Write-Host 'node-map-retrieval: PASS'
