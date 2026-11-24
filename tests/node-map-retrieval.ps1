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
    # This close adds two files, so the native commit-msg gate needs a Least-file reason.
    $closeOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also "$sourceRel,$targetRel" -LeastFile 'fixture files for this node-map close test' 2>&1)
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

  # Set semantics. The script's functions return their HashSets whole (`return ,$set`). Unrolled,
  # an empty set became $null (a null-method crash) and a single entry a substring test.
  $mapScript = Join-Path $clone 'bin/update-node-map.ps1'
  $node = { param($Rel, $Next) Write-Utf8 (Join-Path $clone $Rel) "Title: Fixture`nType: Test Fixture`nContext: Use when testing node-map set semantics.`n`n# Fixture`n`nBody.`n`n---`n`n## SYSTEM NODE LINKS`n`nDirectory Path: docs/`nNext Sequential File: $Next`nRelated Workflows: ../STATE.md`nAI Execution Prompts: ../protocol/CONTRACT.md`n" }
  $fileOf = { param($Rel) ((Get-Content -Raw -LiteralPath (Join-Path $clone 'maps/node-map.json') | ConvertFrom-Json).files | Where-Object { $_.relPath -eq $Rel }) }

  # 1) A memory whose committed map has no link issues must report a new one, not crash.
  & $node 'docs/set-fresh.md' 'gone.md'
  $freshOut = @(& pwsh -NoProfile -File $mapScript -Check 2>&1) | Out-String
  Assert-True 'check reports a new issue when the committed map has none' ($freshOut -match 'New node-map issues:[^\r\n]*broken-link:gone\.md' -and $freshOut -notmatch 'null-valued') $freshOut
  Remove-Item -LiteralPath (Join-Path $clone 'docs/set-fresh.md')

  # 2) With exactly one known issue, a new issue that is a substring of it is still new.
  # The known key ends broken-link:gone.md.bak; the new key, ending broken-link:gone.md, is a
  # substring of it. (A bare 'gone' is recorded as a note, not a link.)
  & $node 'docs/set-one.md' 'gone.md.bak'
  & pwsh -NoProfile -File $mapScript | Out-Null
  & $node 'docs/set-one.md' 'gone.md'
  $oneOut = @(& pwsh -NoProfile -File $mapScript -Check 2>&1) | Out-String
  Assert-True 'a new issue that is a substring of the one known issue is still reported' ($oneOut -match 'New node-map issues:[^\r\n]*broken-link:gone\.md(?!\.bak)') $oneOut
  Remove-Item -LiteralPath (Join-Path $clone 'docs/set-one.md')

  # 3) A tracked file with a non-ASCII name is tracked, and a link to it resolves.
  & $node 'docs/café-note.md' '../STATE.md'
  & $node 'docs/set-link.md' 'café-note.md'
  & git -C $clone add -- 'docs/café-note.md' 'docs/set-link.md'
  & pwsh -NoProfile -File $mapScript | Out-Null
  $accented = & $fileOf 'docs/café-note.md'
  $linker = & $fileOf 'docs/set-link.md'
  Assert-True 'a tracked non-ASCII file name is tracked and links to it resolve' (
    $accented.tracked -eq $true -and $linker -and -not @($linker.issues | Where-Object { $_ -match '^broken-link' }).Count
  ) "tracked=$($accented.tracked) issues=$(@($linker.issues) -join ',')"
  & git -C $clone rm -q --cached -- 'docs/café-note.md' 'docs/set-link.md'
  Remove-Item -LiteralPath (Join-Path $clone 'docs/café-note.md'), (Join-Path $clone 'docs/set-link.md')

  # 4) A git root with Markdown but nothing tracked generates instead of crashing.
  $second = Join-Path $sandbox 'second-root'
  New-Item -ItemType Directory -Path $second | Out-Null
  & git -C $second init -q
  & $node '../second-root/README.md' '../STATE.md'
  $rootsPath = Join-Path $clone 'maps/node-roots.json'
  $rootsBefore = [IO.File]::ReadAllText($rootsPath)
  $roots = @($rootsBefore | ConvertFrom-Json)
  $roots += [pscustomobject]@{ name = 'second'; path = $second; writePolicy = 'read-only'; trackGit = $true; excludes = @(); waivedLegacy = @(); waivedIssues = @() }
  Write-Utf8 $rootsPath ((ConvertTo-Json -InputObject $roots -Depth 6) + "`n")
  $genOut = @(& pwsh -NoProfile -File $mapScript 2>&1) | Out-String
  $genExit = $LASTEXITCODE
  $secondFile = (Get-Content -Raw -LiteralPath (Join-Path $clone 'maps/node-map.json') | ConvertFrom-Json).files | Where-Object { $_.root -eq 'second' }
  Assert-True 'a git root with Markdown but nothing tracked generates without crashing' ($genExit -eq 0 -and $genOut -notmatch 'null-valued' -and $secondFile.tracked -eq $false) "exit $genExit $genOut"
  Write-Utf8 $rootsPath $rootsBefore
  & git -C $clone checkout -q -- maps
} finally {
  if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force }
}

if ($failures) { Write-Host "node-map-retrieval: $failures failure(s)"; exit 1 }
Write-Host 'node-map-retrieval: PASS'
