#requires -Version 7.0
<#
.SYNOPSIS
  Compile live operator rulings for recall; proposals and unclassified legacy entries are excluded.
.DESCRIPTION
  The decision checker validates structure, authority and ratification hashes before compilation.
  A live ruling is an entry that is not superseded or retired and is either hash-ratified
  (operator-ratified) or carries `**Authority:** operator-ruled` with an `**Operator said (...)**`
  line (operator-ruled). Proposals and unclassified legacy entries are excluded; a superseded-in-part
  ruling stays, annotated. A recall gate citing any other id fails the build. The output is
  deterministic (no timestamp) and records SHA-256 hashes of its sources, which recall verifies.
  -Check rebuilds in memory and exits 1 (RULINGS_STATUS=STALE) when the file differs;
  -Check -FromIndex compares staged sources against staged output. No runtime firing is implied.
  With a non-canonical -DecisionsPath and no -RatificationsPath, no ratification ledger is read,
  matching the checker (the live ledger binds the canonical log only).
#>
[CmdletBinding()]
param(
  [switch]$Check,
  [switch]$FromIndex,
  [string]$DecisionsPath,
  [string]$RatificationsPath,
  [string]$GatesPath,
  [string]$OutPath
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
function Assert-DecisionValidation([int]$Code) {
  if ($Code -ne 0) { throw 'Decision validation failed; no rulings compiled.' }
}
# The native engine runs this same command when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine build-rulings -ScriptHandoff @forward
  # Exit 3 is internal to the handoff. Render the same terminating error as the legacy path.
  if ($LASTEXITCODE -eq 3) { Assert-DecisionValidation $LASTEXITCODE }
  exit $LASTEXITCODE
}
$rel = [ordered]@{
  decisions = 'DECISIONS.md'
  ratifications = 'protocol/decision-ratifications.jsonl'; gates = 'protocol/recall-gates.json'
  out = 'protocol/rulings.json'
}
$paths = @{ decisions = $DecisionsPath; ratifications = $RatificationsPath; gates = $GatesPath; out = $OutPath }
$utf8 = [Text.UTF8Encoding]::new($false)

# Exact staged bytes, decoded as UTF-8: a PowerShell pipeline would re-encode non-ASCII text.
function Read-Blob([string]$spec) {
  $psi = [Diagnostics.ProcessStartInfo]::new('git')
  foreach ($a in @('-C', $root, 'cat-file', 'blob', $spec)) { $psi.ArgumentList.Add($a) }
  $psi.RedirectStandardOutput = $true; $psi.RedirectStandardError = $true; $psi.UseShellExecute = $false
  $proc = [Diagnostics.Process]::Start($psi)
  $buf = [IO.MemoryStream]::new()
  $err = $proc.StandardError.ReadToEndAsync()
  $proc.StandardOutput.BaseStream.CopyTo($buf)
  $proc.WaitForExit(); [void]$err.Result
  if ($proc.ExitCode -ne 0) { return '' }
  # Match [IO.File]::ReadAllText, which drops a UTF-8 BOM, so staged and working-tree hashes agree.
  return [Text.UTF8Encoding]::new($false).GetString($buf.ToArray()).TrimStart([char]0xFEFF)
}
function Read-Source([string]$key) {
  if ($FromIndex -and -not $paths[$key]) { return (Read-Blob ":$($rel[$key])") }
  $p = if ($paths[$key]) { $paths[$key] } else { Join-Path $root $rel[$key] }
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { return '' }
  return [IO.File]::ReadAllText((Resolve-Path -LiteralPath $p))
}
function Sha([string]$text) {
  $bytes = $utf8.GetBytes(($text -replace "`r`n", "`n").TrimEnd())
  $h = [Security.Cryptography.SHA256]::Create()
  try { return (($h.ComputeHash($bytes) | ForEach-Object { $_.ToString('x2') }) -join '') } finally { $h.Dispose() }
}
function Clip([string]$s, [int]$n) {
  $s = ($s -replace '\s+', ' ').Trim()
  if ($s.Length -gt $n) { return $s.Substring(0, $n - 3).TrimEnd() + '...' }
  return $s
}

$checkArgs = @('-NoProfile', '-File', (Join-Path $PSScriptRoot 'check-decision-history.ps1'))
if ($FromIndex) { $checkArgs += '-FromIndex' }
if ($DecisionsPath) { $checkArgs += @('-Path', $DecisionsPath) }
if ($RatificationsPath) { $checkArgs += @('-RatificationsPath', $RatificationsPath) }
& pwsh @checkArgs | Out-Host
Assert-DecisionValidation $LASTEXITCODE

$decisions = Read-Source 'decisions'
$fixtureLog = $DecisionsPath -and -not [IO.Path]::GetFullPath($DecisionsPath).Equals([IO.Path]::GetFullPath((Join-Path $root $rel.decisions)), [StringComparison]::OrdinalIgnoreCase)
$ratText = if ($fixtureLog -and -not $RatificationsPath) { '' } else { Read-Source 'ratifications' }
$gatesText = Read-Source 'gates'
if (-not $decisions) { throw 'DECISIONS.md not found.' }

# Entry segments: id -> text from its heading to the next heading.
$heads = [regex]::Matches($decisions, '(?m)^## D(?<id>\d+) [-—] (?<date>\d{4}-\d{2}-\d{2}) [-—] (?<title>[^\r\n]+)')
$entries = @{}
for ($i = 0; $i -lt $heads.Count; $i++) {
  $end = if ($i + 1 -lt $heads.Count) { $heads[$i + 1].Index } else { $decisions.Length }
  $entries[[int]$heads[$i].Groups['id'].Value] = @{ title = $heads[$i].Groups['title'].Value.Trim(); text = $decisions.Substring($heads[$i].Index, $end - $heads[$i].Index) }
}

$ratifiedIds = @{}
foreach ($line in (($ratText -replace "`r`n", "`n") -split "`n" | Where-Object { $_.Trim() })) {
  try { $r = $line | ConvertFrom-Json; if ([string]$r.id -match '^D(\d+)$') { $ratifiedIds[[int]$Matches[1]] = $true } } catch { }
}

$gateCites = @{}
$gateData = if ($gatesText) { $gatesText | ConvertFrom-Json } else { $null }
foreach ($g in @($gateData.gates)) {
  foreach ($rid in @($g.rulings)) {
    if (-not $rid) { continue }
    $n = if ([string]$rid -match '^D(\d+)$') { [int]$Matches[1] } else { 0 }
    if (-not $gateCites.ContainsKey($n)) { $gateCites[$n] = [Collections.Generic.List[string]]::new() }
    $gateCites[$n].Add([string]$g.id)
  }
}

$rulings = [Collections.Generic.List[object]]::new()
foreach ($id in ($entries.Keys | Sort-Object -Descending)) {
  $text = $entries[$id].text
  if ($text -match '(?m)^_Status: (?:superseded|retired) by D\d+ ') { continue }
  $authority = [regex]::Match($text, '(?m)^\*\*Authority:\*\*[ \t]*(?<v>\S+)')
  $class = $null
  $decision = [regex]::Match($text, '(?ms)^\*\*Decision(?: \([^\r\n]+\))?:\*\*[ \t]*(?<v>.*?)(?=^\*\*[A-Z][^\r\n]*?:\*\*|^---|^## SYSTEM NODE LINKS|\z)')
  $rule = if ($decision.Success) { $decision.Groups['v'].Value.Trim() } else { $entries[$id].title }
  if ($ratifiedIds.ContainsKey($id)) { $class = 'operator-ratified' }
  elseif ($authority.Success -and $authority.Groups['v'].Value -eq 'operator-ruled' -and $text -match '(?m)^\*\*Operator said \(') { $class = 'operator-ruled' }
  if ($text -match '(?m)^_Status: superseded in part by D\d+ ' -and $class -and $class -notmatch 'partial') { $rule = "$rule (superseded in part; see the stamp)" }
  if (-not $class) { continue }
  $rulesOut = ''
  $ro = [regex]::Match($text, '(?ms)^\*\*(?:What it )?[Rr]ules out:\*\*\s*(?<v>.+?)(?:\r?\n\r?\n|\z)')
  if ($ro.Success) { $rulesOut = Clip (($ro.Groups['v'].Value -split '(?<=[.;])\s')[0]) 200 }
  $gates = if ($gateCites.ContainsKey($id)) { @($gateCites[$id] | Sort-Object -Unique) } else { @() }
  $rulings.Add([ordered]@{ id = "D$id"; class = $class; title = $entries[$id].title; rule = $rule; rules_out = $rulesOut; gates = $gates })
}

$live = @{}
foreach ($r in $rulings) { $live[[int]$r.id.Substring(1)] = $true }
$badCites = @($gateCites.Keys | Where-Object { -not $live.ContainsKey($_) } | Sort-Object | ForEach-Object { "D$_ (cited by $((@($gateCites[$_]) | Sort-Object -Unique) -join ', '))" })
if ($badCites.Count) {
  Write-Host 'RULINGS_STATUS=FAIL'
  Write-Host "ERROR=recall gates cite ids that are not live rulings (retired, superseded, proposed or unknown): $($badCites -join '; ')"
  exit 1
}

$counts = [ordered]@{}
foreach ($c in @('operator-ratified', 'operator-ruled')) {
  $counts[$c] = @($rulings | Where-Object { $_.class -eq $c }).Count
}
$doc = [ordered]@{
  description = 'Generated by bin/build-rulings.ps1. Do not edit by hand. Unclassified legacy entries are not rulings.'
  sources = [ordered]@{ decisions = (Sha $decisions); ratifications = (Sha $ratText); gates = (Sha $gatesText) }
  counts = $counts
  total = $rulings.Count
  rulings = $rulings
}
$json = ($doc | ConvertTo-Json -Depth 6) -replace "`r`n", "`n"
$json += "`n"

$outFile = if ($paths.out) { $paths.out } else { Join-Path $root $rel.out }
if ($Check) {
  $current = if ($FromIndex -and -not $paths.out) {
    (Read-Blob ":$($rel.out)") -replace "`r`n", "`n"
  } elseif (Test-Path -LiteralPath $outFile) { [IO.File]::ReadAllText($outFile) -replace "`r`n", "`n" } else { '' }
  if ($current -cne $json) {
    Write-Host 'RULINGS_STATUS=STALE'
    Write-Host 'ERROR=protocol/rulings.json does not match its sources; run: pwsh -NoProfile -File bin/build-rulings.ps1'
    exit 1
  }
  Write-Host "RULINGS_STATUS=OK total=$($rulings.Count) $((($counts.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '))"
  exit 0
}
[IO.File]::WriteAllText($outFile, $json, $utf8)
Write-Host "RULINGS_STATUS=WRITTEN total=$($rulings.Count) $((($counts.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '))"
