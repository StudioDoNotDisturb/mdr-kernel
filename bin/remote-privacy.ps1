#requires -Version 7.0
<#
.SYNOPSIS
  Verifies that the actual mdr-kernel push destination is private and approved.
.DESCRIPTION
  GitHub remotes are checked live on every enforced push. An authenticated `gh repo view` is
  preferred. Without gh, a public GitHub API 404 plus authenticated git access proves the repo
  is not publicly visible. That fallback cannot distinguish PRIVATE from GitHub Enterprise
  INTERNAL visibility; use gh when that distinction matters.

  The three network probes are functions so the test battery can dot-source this file and replace
  them with hermetic fakes. Dot-sourcing defines functions only and does not run the command.
#>
[CmdletBinding()]
param(
  [string]$RemoteUrl,
  [switch]$Approve,
  [switch]$RequireApproved,
  [switch]$ConfirmPrivateRemote,
  [switch]$Quiet
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot 'remote-identity.ps1')

function Invoke-GitHubRepoView([string]$Repository) {
  $raw = (& gh repo view $Repository --json visibility,isFork,nameWithOwner 2>$null | Out-String).Trim()
  [pscustomobject]@{ ExitCode = $LASTEXITCODE; Raw = $raw }
}

function Test-GitHubCliAvailable { return [bool](Get-Command gh -ErrorAction SilentlyContinue) }

function Invoke-GitHubPublicLookup([string]$Repository) {
  Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository" `
    -Headers @{ 'User-Agent' = 'mdr-kernel-private-remote-check'; 'Accept' = 'application/vnd.github+json' } `
    -TimeoutSec 10
}

function Invoke-GitRemoteProbe([string]$Url) {
  $oldPrompt = $env:GIT_TERMINAL_PROMPT
  $env:GIT_TERMINAL_PROMPT = '0'
  try {
    & git ls-remote $Url *> $null
    return $LASTEXITCODE
  } finally {
    if ($null -eq $oldPrompt) { Remove-Item Env:\GIT_TERMINAL_PROMPT -ErrorAction SilentlyContinue }
    else { $env:GIT_TERMINAL_PROMPT = $oldPrompt }
  }
}

function Get-HttpStatusCode($ErrorRecord) {
  try {
    $value = $ErrorRecord.Exception.Response.StatusCode
    if ($null -eq $value) { return [int]$ErrorRecord.Exception.Data['StatusCode'] }
    if ($value -is [int]) { return $value }
    if ($null -ne $value.value__) { return [int]$value.value__ }
    return [int]$value
  } catch {
    try { return [int]$ErrorRecord.Exception.Data['StatusCode'] } catch { return $null }
  }
}

function New-PrivacyResult([string]$Status, [string]$Message, [string]$Identity, [int]$Code) {
  [pscustomobject]@{ Status = $Status; Message = $Message; Identity = $Identity; ExitCode = $Code }
}

function Invoke-RemotePrivacy {
  [CmdletBinding()]
  param(
    [string]$RemoteUrl,
    [switch]$Approve,
    [switch]$RequireApproved,
    [switch]$ConfirmPrivateRemote
  )
  Push-Location $root
  try {
  if (-not $RemoteUrl) { $RemoteUrl = (git remote get-url --push origin 2>$null) }
  if (-not $RemoteUrl) {
    if ($Approve) {
      git config --local --unset-all brain.approvedRemote 2>$null
      git config --local brain.mode local-only
      git config --local --unset-all brain.pendingSync 2>$null
      return New-PrivacyResult 'LOCAL_ONLY' 'no push remote configured; memory will stay on this machine' 'local-only' 0
    }
    # No remote is only LOCAL_ONLY when local-only was actually chosen. A memory that pinned a
    # private remote and then lost origin has silently degraded: its checkpoints stop reaching
    # the backup while every surface keeps reporting a healthy local-only state.
    $pinnedMode = (git config --local --get brain.mode 2>$null)
    $pinnedRemote = (git config --local --get brain.approvedRemote 2>$null)
    if ($pinnedMode -eq 'private-remote' -or $pinnedRemote) {
      return New-PrivacyResult 'MISSING_ORIGIN' 'this memory was approved for a private remote but origin is missing; restore it (git remote add origin <approved-url>) or deliberately rerun bin/setup-private.ps1 -LocalOnly' $pinnedRemote 1
    }
    return New-PrivacyResult 'LOCAL_ONLY' 'no push remote configured; memory will stay on this machine' 'local-only' 0
  }
  if ($RemoteUrl -match '(?i)^https?://[^/@]+:[^/@]+@') {
    return New-PrivacyResult 'UNSAFE_URL' 'remote URL contains embedded credentials; replace it with a credential-free URL before continuing' '' 1
  }

  $githubRepo = $null
  if ($RemoteUrl -match '(?i)^(?:https?://github\.com/|ssh://git@github\.com/|git@github\.com:)(?<repo>[^/]+/[^/]+?)(?:\.git)?/?$') {
    $githubRepo = $Matches.repo.TrimEnd('/').ToLowerInvariant()
  }
  $identity = Get-RemoteIdentity $RemoteUrl
  $approved = (git config --local --get brain.approvedRemote 2>$null)
  if ($githubRepo -eq 'studiodonotdisturb/mdr-kernel') {
    return New-PrivacyResult 'UPSTREAM_TEMPLATE' 'this is the public template, not your private memory repo; use the GitHub template button to create a PRIVATE repo, then clone that repo' $identity 1
  }

  if ($githubRepo) {
    $visibility = $null; $isFork = $false; $proof = $null
    if (Test-GitHubCliAvailable) {
      $ghResult = Invoke-GitHubRepoView $githubRepo
      if ($ghResult.ExitCode -eq 0 -and $ghResult.Raw) {
        try {
          $repoInfo = $ghResult.Raw | ConvertFrom-Json
          $visibility = ([string]$repoInfo.visibility).ToUpperInvariant()
          $isFork = [bool]$repoInfo.isFork
          $proof = 'GitHub CLI'
        } catch { $visibility = $null }
      }
    }
    if (-not $visibility) {
      $apiStatus = $null
      try {
        $repoInfo = Invoke-GitHubPublicLookup $githubRepo
        $visibility = if ($repoInfo.private) { 'PRIVATE' } else { 'PUBLIC' }
        $isFork = [bool]$repoInfo.fork
        $proof = 'GitHub public API'
      } catch {
        $apiStatus = Get-HttpStatusCode $_
        if ($apiStatus -eq 404) {
          # GitHub requires forks of a public repository to remain public. Therefore 404 plus
          # authenticated git access proves non-public visibility, but not generic fork metadata
          # or the PRIVATE-vs-INTERNAL distinction on an Enterprise account.
          if ((Invoke-GitRemoteProbe $RemoteUrl) -eq 0) {
            $visibility = 'PRIVATE'
            $proof = 'not publicly visible via GitHub API + authenticated git access'
          }
        }
      }
    }
    if ($isFork) { return New-PrivacyResult 'FORK' 'forks are contribution lanes, not memory repos; create a new PRIVATE repository from the template' $identity 1 }
    if ($visibility -ne 'PRIVATE') {
      $why = if ($visibility) { "GitHub reports visibility $visibility" } elseif ($apiStatus) { "GitHub privacy lookup returned HTTP $apiStatus" } else { 'GitHub privacy could not be verified live' }
      return New-PrivacyResult 'NOT_PRIVATE' "$why; refusing to treat this remote as a memory destination" $identity 1
    }
    $message = "GitHub visibility PRIVATE (verified by $proof)"
  } else {
    # Pin comparisons are case-sensitive (-cne/-ceq): identities are canonicalized by
    # Get-RemoteIdentity, and PowerShell's default -eq would re-conflate case-distinct paths.
    if (-not $ConfirmPrivateRemote -and $approved -cne $identity) {
      return New-PrivacyResult 'UNVERIFIED_HOST' 'this host has no built-in visibility check; rerun setup-private.ps1 with -ConfirmPrivateRemote only after verifying access controls' $identity 1
    }
    $message = 'non-GitHub remote privacy explicitly confirmed and URL pinned'
  }

  if ($RequireApproved -and $approved -cne $identity) {
    $detail = if ($approved) { 'the push destination changed since approval' } else { 'the private destination has not been approved yet' }
    return New-PrivacyResult 'UNAPPROVED' "$detail; rerun bin/setup-private.ps1 to verify and approve it" $identity 1
  }
  if ($Approve) {
    git config --local brain.approvedRemote $identity
    git config --local brain.remoteVisibility private
    git config --local brain.mode private-remote
    $approved = $identity
  }
  $suffix = if ($approved -ceq $identity) { '; destination pin matches' } else { '; run setup-private.ps1 to approve it' }
    return New-PrivacyResult 'PRIVATE' "$message$suffix" $identity 0
  } finally { Pop-Location }
}

if ($MyInvocation.InvocationName -ne '.') {
  # Run as a command (not dot-sourced): the native engine answers when installed (README).
  $engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
  if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
    $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
      if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
    })
    & $engine remote-privacy @forward
    exit $LASTEXITCODE
  }
  $result = Invoke-RemotePrivacy -RemoteUrl $RemoteUrl -Approve:$Approve -RequireApproved:$RequireApproved -ConfirmPrivateRemote:$ConfirmPrivateRemote
  if (-not $Quiet) {
    Write-Host "REMOTE_PRIVACY_STATUS=$($result.Status)"
    if ($result.Identity) { Write-Host "REMOTE_ID=$($result.Identity)" }
    Write-Host "REMOTE_PRIVACY_MESSAGE=$($result.Message)"
  }
  exit $result.ExitCode
}
