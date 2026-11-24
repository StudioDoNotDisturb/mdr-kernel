#requires -Version 7.0
<#
.SYNOPSIS
  Proposal/approval helper for new agent-authored reference notes and skills.

  Propose into local ignored staging, approve into the repo, and append an audit JSONL entry.
  The pre-commit hook requires an approve audit entry when a new reference/*.md or
  skills/*/SKILL.md file is staged.
.EXAMPLE
  pwsh -NoProfile -File bin/write-approval.ps1 -Propose -Target reference/example.md -Source .\draft.md -Reason 'operator requested'
.EXAMPLE
  pwsh -NoProfile -File bin/write-approval.ps1 -Approve -ProposalId 20260704T010000Z-example -Reason 'reviewed with operator'
.EXAMPLE
  pwsh -NoProfile -File bin/write-approval.ps1 -List
#>
[CmdletBinding()]
param(
  [switch]$Propose,
  [switch]$Approve,
  [switch]$List,
  [switch]$Show,
  [switch]$ValidateStagedApprovals,
  [string]$Target,
  [string]$Source,
  [string]$ProposalId,
  [string]$Reason,
  [switch]$Force,
  [switch]$DryRun
)

$ErrorActionPreference = 'Stop'
# Adopter-facing tool: render any unhandled terminating error as a one-line [FAIL] verdict
# instead of a raw PowerShell exception dump. Handled errors (the -List metadata loop) still
# use their local try/catch.
trap { Write-Host "[FAIL] $($_.Exception.Message)"; exit 1 }
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same workflow when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine write-approval @forward
  exit $LASTEXITCODE
}
$proposalRoot = Join-Path $root '.write-proposals'
$auditPath = Join-Path $root 'inbox/write-approval-audit.jsonl'
$workflow = 'write-approval-v1'
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Test-PathUnderRoot([string]$TargetPath, [string]$RootPath) {
  $target = ([IO.Path]::GetFullPath($TargetPath)).TrimEnd([char[]]@('\', '/'))
  $rootFull = ([IO.Path]::GetFullPath($RootPath)).TrimEnd([char[]]@('\', '/'))
  return (
    $target.Equals($rootFull, [StringComparison]::OrdinalIgnoreCase) -or
    $target.StartsWith("$rootFull\", [StringComparison]::OrdinalIgnoreCase) -or
    $target.StartsWith("$rootFull/", [StringComparison]::OrdinalIgnoreCase)
  )
}

function To-RepoPath([string]$Path) {
  if (-not $Path) { return $null }
  $base = if ([IO.Path]::IsPathRooted($Path)) { $Path } else { Join-Path $root $Path }
  $full = [IO.Path]::GetFullPath($base)
  $rootFull = ([IO.Path]::GetFullPath($root)).TrimEnd([char[]]@('\', '/'))
  if (-not (Test-PathUnderRoot -TargetPath $full -RootPath $rootFull)) {
    throw "Path is outside repo: $Path"
  }
  $rel = $full.Substring($rootFull.Length).TrimStart('\', '/')
  return ($rel -replace '\\', '/')
}

function Test-ApprovedTarget([string]$RepoPath) {
  if ($RepoPath -match '^reference/[^/]+\.md$' -and $RepoPath -ne 'reference/INDEX.md') { return $true }
  if ($RepoPath -match '^skills/[^/]+/SKILL\.md$' -and $RepoPath -ne 'skills/mdr-kernel/SKILL.md') { return $true }
  return $false
}

function Get-Model {
  # A proposal or approval is attributed to an explicit session identity only. git config
  # brain.model is display-only and 'unknown' is refused, the same rule locks and commits apply.
  $explicit = if ($env:BRAIN_MODEL) { ([string]$env:BRAIN_MODEL).Trim() } else { '' }
  if (-not $explicit -or $explicit -eq 'unknown') {
    throw 'write-approval needs an explicit session identity: set $env:BRAIN_MODEL before -Propose or -Approve (git config brain.model is display-only and is not used).'
  }
  return $explicit
}

function New-Slug([string]$Text) {
  $slug = ($Text.ToLowerInvariant() -replace '[^a-z0-9]+', '-' -replace '^-|-$', '')
  if ($slug.Length -gt 48) { $slug = $slug.Substring(0, 48).TrimEnd('-') }
  if (-not $slug) { $slug = 'proposal' }
  return $slug
}

function Get-ProposalDir([string]$Id) {
  if ($Id -notmatch '^[0-9TZ-]+-[a-z0-9-]+$') { throw "Invalid proposal id: $Id" }
  $dir = Join-Path $proposalRoot $Id
  if (-not (Test-Path -LiteralPath $dir)) { throw "Proposal not found: $Id" }
  return $dir
}

function Read-Metadata([string]$Dir) {
  $metaPath = Join-Path $Dir 'metadata.json'
  if (-not (Test-Path -LiteralPath $metaPath)) { throw "Missing metadata.json in $Dir" }
  return (Get-Content -Raw -LiteralPath $metaPath | ConvertFrom-Json)
}

function Write-Audit([object]$Entry) {
  $json = $Entry | ConvertTo-Json -Compress -Depth 8
  if ($DryRun) {
    Write-Host "DRY_RUN audit: $json"
    return
  }
  $dir = Split-Path -Parent $auditPath
  if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
  [IO.File]::AppendAllText($auditPath, $json + "`n", $Utf8NoBom)
}

function Get-StagedContent([string]$RepoPath) {
  $output = & git -C $root show --no-textconv ":$RepoPath" 2>$null
  if ($LASTEXITCODE -ne 0) { return $null }
  return ($output -join "`n")
}

function Get-StagedAddedApprovalTargets() {
  $targets = [System.Collections.Generic.List[string]]::new()
  # --no-renames: a file moved into a gated path from anywhere else is a new gated file, not a
  # rename that slips past the added-file filter.
  foreach ($path in (& git -C $root diff --cached --name-only --no-renames --diff-filter=A -- 'reference/*.md' 'skills/*/SKILL.md' 2>$null)) {
    $repoPath = ($path -replace '\\', '/').Trim()
    if (-not $repoPath) { continue }
    if (Test-ApprovedTarget $repoPath) { $targets.Add($repoPath) }
  }
  return $targets
}

function Test-ValidateStagedApprovals {
  $targets = @(Get-StagedAddedApprovalTargets | Sort-Object -Unique)
  if (-not $targets.Count) { return @{ Ok = $true; Message = $null } }

  $auditText = Get-StagedContent 'inbox/write-approval-audit.jsonl'
  $errors = [System.Collections.Generic.List[string]]::new()
  $approvals = @{}

  if (-not $auditText) {
    return @{
      Ok = $false
      Message = "new reference/skill files need write approval. Missing staged inbox/write-approval-audit.jsonl entry for: $($targets -join ', ')"
    }
  }

  foreach ($line in ($auditText -split "`r?`n")) {
    if ([string]::IsNullOrWhiteSpace($line)) { continue }
    try { $entry = $line | ConvertFrom-Json } catch { continue }
    if ($entry.action -ne 'approve') { continue }
    if ($entry.workflow -ne $workflow) { continue }
    if (-not $entry.target) { continue }
    $approvals[[string]$entry.target] = $entry
  }

  foreach ($targetPath in $targets) {
    $entry = $approvals[$targetPath]
    if (-not $entry) {
      $errors.Add("${targetPath}: missing staged approve entry")
      continue
    }
    if (-not $entry.proposal_id) {
      $errors.Add("${targetPath}: approve entry missing proposal_id")
      continue
    }

    $proposalDir = Join-Path $proposalRoot ([string]$entry.proposal_id)
    $metadataPath = Join-Path $proposalDir 'metadata.json'
    $contentPath = Join-Path $proposalDir 'content.md'
    if (-not (Test-Path -LiteralPath $metadataPath) -or -not (Test-Path -LiteralPath $contentPath)) {
      $errors.Add("${targetPath}: local proposal payload missing for $($entry.proposal_id)")
      continue
    }

    try { $metadata = Get-Content -Raw -LiteralPath $metadataPath | ConvertFrom-Json }
    catch {
      $errors.Add("${targetPath}: unreadable metadata for $($entry.proposal_id)")
      continue
    }

    $proposalHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $contentPath).Hash.ToLowerInvariant()
    $expectedHash = ([string]$metadata.content_sha256).ToLowerInvariant()
    $auditHash = ([string]$entry.content_sha256).ToLowerInvariant()
    if ([string]$metadata.proposal_id -ne [string]$entry.proposal_id) {
      $errors.Add("${targetPath}: metadata proposal_id mismatch for $($entry.proposal_id)")
    }
    if ([string]$metadata.target -ne $targetPath) {
      $errors.Add("${targetPath}: metadata target mismatch for $($entry.proposal_id)")
    }
    if (-not $expectedHash -or $proposalHash -ne $expectedHash) {
      $errors.Add("${targetPath}: proposal content hash mismatch for $($entry.proposal_id)")
    }
    if (-not $auditHash -or $proposalHash -ne $auditHash) {
      $errors.Add("${targetPath}: audit content hash mismatch for $($entry.proposal_id)")
    }
    # The commit snapshots the INDEX, so the staged file itself must be the approved content.
    # hash-object --path applies the same filters staging does (autocrlf, attributes), so line
    # endings cannot cause a false refusal; any other byte difference is a swap after approval.
    $stagedId = (& git -C $root rev-parse --verify --quiet ":$targetPath" 2>$null | Out-String).Trim()
    $approvedId = (& git -C $root hash-object "--path=$targetPath" -- $contentPath 2>$null | Out-String).Trim()
    if (-not $stagedId -or -not $approvedId -or $stagedId -ne $approvedId) {
      $errors.Add("${targetPath}: staged content is not the approved proposal $($entry.proposal_id)")
    }
  }

  if ($errors.Count) {
    return @{
      Ok = $false
      Message = "new reference/skill files need valid write approval.`n  " + ($errors -join "`n  ")
    }
  }

  return @{ Ok = $true; Message = $null }
}

$selectedModes = @($Propose, $Approve, $List, $Show, $ValidateStagedApprovals) | Where-Object { $_ }
$modeCount = @($selectedModes).Count
if ($modeCount -eq 0) { $List = $true; $modeCount = 1 }
if ($modeCount -ne 1) { throw 'Choose exactly one of -Propose, -Approve, -List, -Show, or -ValidateStagedApprovals.' }

if ($ValidateStagedApprovals) {
  $result = Test-ValidateStagedApprovals
  if ($result.Ok) { exit 0 }
  Write-Host $result.Message
  exit 1
}

if ($Propose) {
  if (-not $Target -or -not $Source -or -not $Reason) {
    throw '-Propose requires -Target, -Source, and -Reason.'
  }
  $actor = Get-Model   # refuse before any proposal directory is written
  $targetRel = To-RepoPath $Target
  if (-not (Test-ApprovedTarget $targetRel)) {
    throw "Target is not gated by this workflow: $targetRel"
  }
  $sourceBase = if ([IO.Path]::IsPathRooted($Source)) { $Source } else { Join-Path (Get-Location) $Source }
  $sourceFull = [IO.Path]::GetFullPath($sourceBase)
  if (-not (Test-Path -LiteralPath $sourceFull)) { throw "Source not found: $Source" }

  $id = ((Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')) + '-' + (New-Slug ([IO.Path]::GetFileNameWithoutExtension($targetRel)))
  $dir = Join-Path $proposalRoot $id
  $contentPath = Join-Path $dir 'content.md'
  $metadataPath = Join-Path $dir 'metadata.json'
  $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $sourceFull).Hash.ToLowerInvariant()
  $metadata = [ordered]@{
    workflow = $workflow
    proposal_id = $id
    target = $targetRel
    source = $sourceFull
    content_sha256 = $hash
    reason = $Reason
    proposed_by = $actor
    proposed_at = [DateTimeOffset]::UtcNow.ToString('o')
  }

  if ($DryRun) {
    Write-Host "DRY_RUN propose: $id -> $targetRel"
    exit 0
  }
  New-Item -ItemType Directory -Path $dir -Force | Out-Null
  Copy-Item -LiteralPath $sourceFull -Destination $contentPath -Force
  [IO.File]::WriteAllText($metadataPath, (($metadata | ConvertTo-Json -Depth 8) + "`n"), $Utf8NoBom)
  Write-Host "PROPOSAL_ID=$id"
  Write-Host "TARGET=$targetRel"
  Write-Host "NEXT=pwsh -NoProfile -File bin/write-approval.ps1 -Approve -ProposalId $id -Reason '<review outcome>'"
  exit 0
}

if ($Approve) {
  if (-not $ProposalId -or -not $Reason) { throw '-Approve requires -ProposalId and -Reason.' }
  $actor = Get-Model   # refuse before the target or the audit log is touched
  $dir = Get-ProposalDir $ProposalId
  $meta = Read-Metadata $dir
  $targetRel = [string]$meta.target
  if (-not (Test-ApprovedTarget $targetRel)) { throw "Target is not gated by this workflow: $targetRel" }
  $contentPath = Join-Path $dir 'content.md'
  if (-not (Test-Path -LiteralPath $contentPath)) { throw "Missing proposal content: $contentPath" }
  $targetFull = Join-Path $root ($targetRel -replace '/', '\')
  if ((Test-Path -LiteralPath $targetFull) -and -not $Force) {
    throw "Target exists; use -Force only after review: $targetRel"
  }
  $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $contentPath).Hash.ToLowerInvariant()
  if ($meta.content_sha256 -and $hash -ne ([string]$meta.content_sha256).ToLowerInvariant()) {
    throw "Proposal content hash changed for $ProposalId"
  }

  $entry = [ordered]@{
    workflow = $workflow
    action = 'approve'
    proposal_id = $ProposalId
    target = $targetRel
    approved_by = $actor
    approved_at = [DateTimeOffset]::UtcNow.ToString('o')
    reason = $Reason
    content_sha256 = $hash
  }

  if ($DryRun) {
    Write-Host "DRY_RUN approve: $ProposalId -> $targetRel"
    Write-Audit $entry
    exit 0
  }
  $targetDir = Split-Path -Parent $targetFull
  if (-not (Test-Path -LiteralPath $targetDir)) { New-Item -ItemType Directory -Path $targetDir -Force | Out-Null }
  Copy-Item -LiteralPath $contentPath -Destination $targetFull -Force
  Write-Audit $entry
  Write-Host "APPROVED=$targetRel"
  Write-Host "AUDIT=inbox/write-approval-audit.jsonl"
  exit 0
}

if ($Show) {
  if (-not $ProposalId) { throw '-Show requires -ProposalId.' }
  $dir = Get-ProposalDir $ProposalId
  Read-Metadata $dir | ConvertTo-Json -Depth 8
  Write-Host ''
  Write-Host '--- content.md ---'
  Get-Content -LiteralPath (Join-Path $dir 'content.md')
  exit 0
}

if (-not (Test-Path -LiteralPath $proposalRoot)) {
  Write-Host 'No pending proposals.'
  exit 0
}

$items = Get-ChildItem -LiteralPath $proposalRoot -Directory | Sort-Object Name
if (-not $items) {
  Write-Host 'No pending proposals.'
  exit 0
}
foreach ($item in $items) {
  try {
    $meta = Read-Metadata $item.FullName
    Write-Host "$($meta.proposal_id) -> $($meta.target) :: $($meta.reason)"
  } catch {
    Write-Host "$($item.Name) -> unreadable metadata"
  }
}
