#requires -Version 7.0
<#
.SYNOPSIS
  Deeper on-demand secret audit. The pre-commit scan and brain-doctor check stay fast with a small
  high-confidence pattern set; this audit casts a wider net - more providers, generic credential
  assignments, URLs with embedded passwords, JWTs, and secret-shaped tracked FILENAMES. Run it
  periodically and before publishing a repo. It is deliberately NOT wired into the fast pre-commit
  path.

  False positives are expected at this width. Suppress a reviewed line by appending the marker
  comment `secret-audit:allow` to that line.
.EXAMPLE
  pwsh -NoProfile -File bin/secret-audit.ps1
.EXAMPLE
  pwsh -NoProfile -File bin/secret-audit.ps1 -CheckScannerCoverage
#>
[CmdletBinding()]
param([switch]$CheckScannerCoverage)
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same audit when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key)"; "$($p.Value)" }
  })
  & $engine secret-audit @forward
  exit $LASTEXITCODE
}
Set-Location $root

function Test-ScannerCoverage {
  $targets = @(
    [pscustomobject]@{
      Name = 'pre-commit fast staged scan'
      Path = '.githooks/pre-commit'
      RequiredPatterns = @(
        '(?m)^\s*patterns=.*sk-\(proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}',
        '(?m)^\s*patterns=.*api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token'
      )
    },
    [pscustomobject]@{
      Name = 'brain-doctor tracked-file scan'
      Path = 'bin/brain-doctor.ps1'
      RequiredPatterns = @(
        '(?s)\$patterns\s*=\s*@\(.*sk-\(\?:proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}',
        '(?s)\$patterns\s*=\s*@\(.*api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token'
      )
    },
    [pscustomobject]@{
      Name = 'secret-audit deep scan'
      Path = 'bin/secret-audit.ps1'
      RequiredPatterns = @(
        "(?m)^\s*'fast-scan set'\s*=\s*'.*sk-\(\?:proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}",
        "(?m)^\s*'credential assignment'\s*=\s*'.*api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token"
      )
    },
    # The native engine's scanners guard commits and audits once bin/mdr-kernel is installed.
    [pscustomobject]@{
      Name = 'native pre-commit staged scan'
      Path = 'native/src/hook.rs'
      RequiredPatterns = @(
        'sk-\(proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}',
        '\(api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token\)'
      )
    },
    [pscustomobject]@{
      Name = 'native secret-audit deep scan'
      Path = 'native/src/health.rs'
      RequiredPatterns = @(
        '"fast-scan set",\s*r"[^"]*sk-\(\?:proj\|svcacct\)-\[A-Za-z0-9_-\]\{20,\}',
        '"credential assignment",\s*r#?"[^\n]*\(api\[_-\]\?key\|secret\|passwd\|password\|access\[_-\]\?token\)'
      )
    }
  )

  $missing = @()
  foreach ($target in $targets) {
    $fp = Join-Path $root $target.Path
    if (-not (Test-Path -LiteralPath $fp)) {
      $missing += "$($target.Name) ($($target.Path)): file missing"
      continue
    }

    $text = Get-Content -Raw -LiteralPath $fp
    foreach ($requiredPattern in $target.RequiredPatterns) {
      if ($text -notmatch $requiredPattern) {
        $missing += "$($target.Name) ($($target.Path)): missing required secret pattern coverage"
      }
    }
  }

  if ($missing.Count -gt 0) {
    foreach ($m in $missing) { Write-Host "[missing] $m" }
    Write-Host ''
    Write-Host 'scanner-coverage: failed. Add the modern OpenAI project/service-account key regex to every scanner before relying on a clean audit.'
    return 1
  }

  Write-Host '[ ok ] pre-commit, brain-doctor, and secret-audit all cover both modern key shapes and generic credential assignments.'
  return 0
}

if ($CheckScannerCoverage) {
  exit (Test-ScannerCoverage)
}

# Files whose job is to describe secret shapes.
$skip = @('.githooks/pre-commit', 'bin/brain-doctor.ps1', 'bin/secret-audit.ps1', 'native/src/health.rs')

# Wider net than the fast scan (which covers Anthropic/OpenAI/GitHub/AWS/Google/Slack/PEM).
$patterns = [ordered]@{
  'fast-scan set'        = 'sk-ant-[A-Za-z0-9_-]{20,}|sk-(?:proj|svcacct)-[A-Za-z0-9_-]{20,}|sk-[A-Za-z0-9]{20,}|ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|xox[baprs]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY-----'
  'provider tokens'      = 'npm_[A-Za-z0-9]{30,}|pypi-[A-Za-z0-9_-]{30,}|hf_[A-Za-z0-9]{30,}|dop_v1_[a-f0-9]{20,}|sk_live_[A-Za-z0-9]{20,}|rk_live_[A-Za-z0-9]{20,}|SG\.[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{20,}|glpat-[A-Za-z0-9_-]{20,}'
  'slack webhook'        = 'hooks\.slack\.com/services/T[A-Za-z0-9_/]{10,}'
  'JWT'                  = 'eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}'
  'bearer header'        = 'Authorization:\s*Bearer\s+[A-Za-z0-9_\-.=]{20,}'
  'URL with password'    = '[a-z][a-z0-9+.-]*://[^/\s:@"]+:[^@\s/"]{3,}@'
  'credential assignment' = '(?i)[\w-]*(api[_-]?key|secret|passwd|password|access[_-]?token)\b\s*[:=]\s*["''][^"'']{8,}["'']'
}

# Tracked filenames that usually hold credentials.
$namePatterns = '(?i)(^|/)(\.env(\..+)?|id_rsa.*|.*\.(pem|pfx|p12|key)|.*credentials.*|.*secret.*)$'
$nameAllow    = '(?i)(secret-audit|secrets?\.md$)'  # docs ABOUT secrets, and this tool

$hits = 0
foreach ($f in (git ls-files)) {
  if ($skip -contains $f) { continue }
  $fp = Join-Path $root $f
  if (-not (Test-Path $fp)) { continue }

  if ($f -match $namePatterns -and $f -notmatch $nameAllow) {
    Write-Host "[name] $f - secret-shaped filename is tracked; keep secret VALUES out of the repo. Store them in an untracked local path and reference them by filename."
    $hits++
  }

  $lineNo = 0
  foreach ($line in (Get-Content -LiteralPath $fp -ErrorAction SilentlyContinue)) {
    $lineNo++
    if ($line -match 'secret-audit:allow') { continue }
    foreach ($k in $patterns.Keys) {
      if ($line -match $patterns[$k]) {
        $shown = if ($line.Length -gt 120) { $line.Substring(0, 120) + '…' } else { $line }
        Write-Host "[$k] ${f}:${lineNo}: $($shown.Trim())"
        $hits++
        break
      }
    }
  }
}

Write-Host ''
if ($hits -gt 0) {
  Write-Host "secret-audit: $hits finding(s). Review each: real secret -> move it to an untracked location and purge it from git history; false positive -> append ``secret-audit:allow`` to the line."
  exit 1
}
Write-Host 'secret-audit: clean.'
exit 0
