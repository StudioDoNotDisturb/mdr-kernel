#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $root 'bin/remote-privacy.ps1')
$failures = 0

function Expect([string]$Name, [string]$Expected, [scriptblock]$Action) {
  try { $actual = [string](& $Action) } catch { $actual = "THREW:$($_.Exception.Message)" }
  if ($actual -eq $Expected) { Write-Host "[PASS] $Name" }
  else { Write-Host "[FAIL] $Name (expected '$Expected', got '$actual')"; $script:failures++ }
}
function Set-GhResult([string]$Visibility, [bool]$Fork = $false) {
  function script:Test-GitHubCliAvailable { $true }
  $script:fakeGhJson = @{ visibility = $Visibility; isFork = $Fork; nameWithOwner = 'owner/repo' } | ConvertTo-Json -Compress
  function script:Invoke-GitHubRepoView([string]$Repository) { [pscustomobject]@{ ExitCode = 0; Raw = $script:fakeGhJson } }
}
function Set-NoGhApiResult([bool]$Private, [bool]$Fork = $false) {
  function script:Test-GitHubCliAvailable { $false }
  $script:fakePrivate = $Private; $script:fakeFork = $Fork
  function script:Invoke-GitHubPublicLookup([string]$Repository) { [pscustomobject]@{ private = $script:fakePrivate; fork = $script:fakeFork } }
}
function Set-NoGhApiFailure([Nullable[int]]$Status, [int]$ProbeExit = 1) {
  function script:Test-GitHubCliAvailable { $false }
  $script:fakeStatus = $Status; $script:fakeProbeExit = $ProbeExit
  function script:Invoke-GitHubPublicLookup([string]$Repository) {
    $e = [Exception]::new('hermetic lookup failure')
    if ($null -ne $script:fakeStatus) { $e.Data['StatusCode'] = [int]$script:fakeStatus }
    throw $e
  }
  function script:Invoke-GitRemoteProbe([string]$Url) { $script:fakeProbeExit }
}
function Status([string]$Url) { (Invoke-RemotePrivacy -RemoteUrl $Url).Status }
function Identity([string]$Url) { (Invoke-RemotePrivacy -RemoteUrl $Url).Identity }

Set-GhResult 'PRIVATE';  Expect 'gh: PRIVATE accepted' 'PRIVATE' { Status 'https://github.com/Owner/Repo.git' }
Set-GhResult 'PUBLIC';   Expect 'gh: PUBLIC refused' 'NOT_PRIVATE' { Status 'https://github.com/owner/repo' }
Set-GhResult 'PRIVATE';  [void](Status 'https://github.com/owner/flipped.git')
Set-GhResult 'PUBLIC';   Expect 'live recheck refuses a remote flipped from private to public' 'NOT_PRIVATE' { Status 'https://github.com/owner/flipped.git' }
Set-GhResult 'INTERNAL'; Expect 'gh: INTERNAL refused' 'NOT_PRIVATE' { Status 'git@github.com:owner/repo.git' }
Set-GhResult 'PRIVATE' $true; Expect 'gh: fork refused' 'FORK' { Status 'ssh://git@github.com/owner/repo.git' }

Set-NoGhApiResult $false; Expect 'no-gh API 200 public refused' 'NOT_PRIVATE' { Status 'https://github.com/owner/repo.git' }
Set-NoGhApiResult $true;  Expect 'no-gh API 200 private accepted' 'PRIVATE' { Status 'https://github.com/owner/repo.git' }
Set-NoGhApiResult $true $true; Expect 'no-gh API fork refused' 'FORK' { Status 'https://github.com/owner/repo.git' }
Set-NoGhApiFailure 404 0; Expect 'no-gh 404 plus authenticated git accepted as nonpublic' 'PRIVATE' { Status 'https://github.com/owner/repo.git' }
Set-NoGhApiFailure 404 1; Expect 'no-gh 404 plus failed git refused' 'NOT_PRIVATE' { Status 'https://github.com/owner/repo.git' }
Set-NoGhApiFailure 403 0; Expect 'no-gh 403 never falls through to git proof' 'NOT_PRIVATE' { Status 'https://github.com/owner/repo.git' }
Set-NoGhApiFailure $null 0; Expect 'no-gh timeout/unknown failure refused' 'NOT_PRIVATE' { Status 'https://github.com/owner/repo.git' }

Set-GhResult 'PRIVATE'
Expect 'HTTPS parsing normalizes identity' 'github:owner/repo' { Identity 'https://github.com/Owner/Repo.git' }
Expect 'SSH scp parsing normalizes identity' 'github:owner/repo' { Identity 'git@github.com:Owner/Repo.git' }
Expect 'SSH URL parsing normalizes identity' 'github:owner/repo' { Identity 'ssh://git@github.com/Owner/Repo.git' }
Expect 'embedded credentials refused' 'UNSAFE_URL' { Status 'https://user:token@github.com/owner/repo.git' } # secret-audit:allow synthetic test URL
Expect 'upstream template refused before probes' 'UPSTREAM_TEMPLATE' { Status 'https://github.com/StudioDoNotDisturb/mdr-kernel.git' }
Expect 'PowerShell compatibility floor is 7.0' 'True' { ($PSVersionTable.PSVersion -ge [version]'7.0').ToString() }

if ($failures) { Write-Host "remote-privacy-unit: $failures failure(s)"; exit 1 }
Write-Host 'remote-privacy-unit: PASS'
