#requires -Version 7.0
<#
.SYNOPSIS
  Validate the structural integrity of DECISIONS.md.
.DESCRIPTION
  Fails on duplicate decision ids, non-descending order, missing ids, empty entries, and
  anonymous decision bodies (more than one top-level **Decision:** body under one heading).
  Use -FromIndex from pre-commit so the staged blob, rather than the working tree, is checked.

  Append-only: with -FromIndex the staged blob is also
  compared with HEAD's log (or with -BaselinePath when given). Every id in the baseline must be
  byte-identical in the checked log, from its heading to the next heading. Permitted exceptions:
  the header above the first entry, the template/footer tail after the oldest entry, and added
  machine stamp lines of the exact form `_Status: <kind> by D### (YYYY-MM-DD)_`, where kind is
  `superseded`, `superseded in part` or `retired`. A correction is a new entry, never an edit.

  Authority: every entry from -AuthorityFrom on carries exactly one
  `**Authority:** operator-ruled` or `**Authority:** proposed` line. operator-ruled also needs an
  `**Operator said (<surface>, YYYY-MM-DD):** "<quote>"` line. `operator-ratified` is never typed
  (ratification is recorded separately) and `record` is refused (a work report belongs in a handoff).

  Supersession as data: `**Supersedes:**`, `**Supersedes in part:**` and `**Retires:**`
  list older ids. Each listed target must carry the matching stamp dated with the superseding entry,
  every stamp must be backed by such a field, and a `proposed` entry may only supersede another
  `proposed` entry and may not retire anything - a ruling yields only to a ruling.

  Ratification: `protocol/decision-ratifications.jsonl` holds one JSON object per line,
  {id, entry_sha256, ratified_at, quote, surface}, appended only by the operator through
  `bin/ratify-decision.ps1`. Each record must name an existing entry and match the SHA-256 of that
  entry's bytes (stamp lines removed, line endings normalised), so an entry edited after ratification
  loses it loudly. The file is append-only against the baseline. -HashOf D### prints the hash the
  ratification tool binds to, so there is exactly one hashing rule.
#>
[CmdletBinding()]
param(
  [string]$Path,
  [switch]$FromIndex,
  [string]$BaselinePath,
  [ValidateRange(1,2147483647)][int]$AuthorityFrom = 1,
  [string]$RatificationsPath,
  [string]$RatificationsBaselinePath,
  [string]$HashOf
)

$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same command when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key):$($p.Value)" }
  })
  & $engine check-decision-history @forward
  exit $LASTEXITCODE
}
if (-not $Path) { $Path = Join-Path $root 'DECISIONS.md' }
$errors = [System.Collections.Generic.List[string]]::new()
$strictUtf8 = [Text.UTF8Encoding]::new($false, $true)
$stampPattern = '(?m)^_Status: (?:superseded|superseded in part|retired) by D\d+ \(\d{4}-\d{2}-\d{2}\)_\r?(?:\n|\z)'
$stampParse = '(?m)^_Status: (?<kind>superseded|superseded in part|retired) by D(?<by>\d+) \((?<date>\d{4}-\d{2}-\d{2})\)_\r?$'
$fieldKinds = [ordered]@{ 'Supersedes' = 'superseded'; 'Supersedes in part' = 'superseded in part'; 'Retires' = 'retired' }
$quotePattern = '(?m)^\*\*Operator said \((?:chat|terminal|operator terminal|in person|email|phone)\b[^)\r\n]*?\d{4}-\d{2}-\d{2}[^)\r\n]*\):\*\* ["“][^\r\n]{12,}?["”]'

# Exact blob bytes: a PowerShell pipeline would re-encode and split lines, which can hide a change.
function Read-GitBlob([string]$Spec) {
  $psi = [Diagnostics.ProcessStartInfo]::new('git')
  foreach ($arg in @('-C', $root, 'cat-file', 'blob', $Spec)) { $psi.ArgumentList.Add($arg) }
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.UseShellExecute = $false
  $process = [Diagnostics.Process]::Start($psi)
  $buffer = [IO.MemoryStream]::new()
  $stderrTask = $process.StandardError.ReadToEndAsync()
  $process.StandardOutput.BaseStream.CopyTo($buffer)
  $process.WaitForExit()
  [void]$stderrTask.Result
  if ($process.ExitCode -ne 0) { return $null }
  # Same text the working tree gives: [IO.File]::ReadAllText drops a UTF-8 BOM, so drop it here too.
  return $strictUtf8.GetString($buffer.ToArray()).TrimStart([char]0xFEFF)
}

# A migration boundary preserves existing entries without declaring them rulings.
# It is immutable once committed, and may exempt only entries already in HEAD.
$canonical = [IO.Path]::GetFullPath($Path).Equals([IO.Path]::GetFullPath((Join-Path $root 'DECISIONS.md')), [StringComparison]::OrdinalIgnoreCase)
if ($FromIndex -or $canonical) {
  $policyRel = 'protocol/decision-authority.json'
  $policyPath = Join-Path $root $policyRel
  $policyRaw = if ($FromIndex) { Read-GitBlob ":$policyRel" } elseif (Test-Path -LiteralPath $policyPath) { [IO.File]::ReadAllText($policyPath) }
  if ($policyRaw) {
    try {
      $policy = $policyRaw | ConvertFrom-Json
      $from = $policy.authority_from
      if ($policy.schema_version -ne 1 -or ($from -isnot [long] -and $from -isnot [int]) -or $from -lt 1 -or $from -gt [int]::MaxValue) { throw 'invalid policy' }
      $AuthorityFrom = [int]$policy.authority_from
      if ($FromIndex) {
        $oldPolicy = Read-GitBlob "HEAD:$policyRel"
        if ($oldPolicy -and ($oldPolicy -replace "`r`n", "`n").TrimEnd() -cne ($policyRaw -replace "`r`n", "`n").TrimEnd()) { $errors.Add('The committed decision authority boundary is immutable.') }
        if (-not $oldPolicy) {
          $oldLog = Read-GitBlob 'HEAD:DECISIONS.md'
          $maxOld = 0
          foreach ($m in [regex]::Matches([string]$oldLog, '(?m)^## D(\d+)\b')) { $maxOld = [Math]::Max($maxOld,[int]$m.Groups[1].Value) }
          if ($AuthorityFrom -gt $maxOld + 1) { $errors.Add('Migration can exempt only decision entries already committed in HEAD.') }
        }
      }
    } catch { $errors.Add('Invalid protocol/decision-authority.json; expected schema_version 1 and positive integer authority_from.') }
  } elseif ($FromIndex -and (Read-GitBlob "HEAD:$policyRel")) { $errors.Add('The committed decision authority boundary cannot be deleted.') }
}

# id -> bytes from its heading up to the next heading (or the template/footer tail after the oldest).
function Get-EntrySegments([string]$Text) {
  $segments = @{}
  $found = [regex]::Matches($Text, '(?m)^## D(?<id>\d+)\b[^\r\n]*')
  if ($found.Count -eq 0) { return $segments }
  $tail = $Text.IndexOf('<!-- Template for a new entry', $found[$found.Count - 1].Index)
  $footer = [regex]::new('(?m)^---[ \t]*\r?\n(?:[ \t]*\r?\n)*## SYSTEM NODE LINKS').Match($Text, $found[$found.Count - 1].Index)
  if ($footer.Success -and ($tail -lt 0 -or $footer.Index -lt $tail)) { $tail = $footer.Index }
  if ($tail -lt 0) { $tail = $Text.Length }
  for ($i = 0; $i -lt $found.Count; $i++) {
    $id = [int]$found[$i].Groups['id'].Value
    $end = if ($i + 1 -lt $found.Count) { $found[$i + 1].Index } else { $tail }
    if (-not $segments.ContainsKey($id)) { $segments[$id] = $Text.Substring($found[$i].Index, $end - $found[$i].Index) }
  }
  return $segments
}

$baseline = $null
$baselineLabel = $null
if ($FromIndex) {
  $raw = Read-GitBlob ':DECISIONS.md'
  if ($null -eq $raw) {
    Write-Host 'DECISION_HISTORY_STATUS=FAIL'
    Write-Host 'ERROR=DECISIONS.md is absent from the Git index.'
    exit 1
  }
  if (-not $BaselinePath) {
    $baseline = Read-GitBlob 'HEAD:DECISIONS.md'   # null on a first commit: nothing to preserve yet
    $baselineLabel = 'HEAD'
  }
} else {
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    Write-Host 'DECISION_HISTORY_STATUS=FAIL'
    Write-Host "ERROR=Decision log not found: $Path"
    exit 1
  }
  $raw = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $Path))
}
if ($BaselinePath) {
  if (-not (Test-Path -LiteralPath $BaselinePath -PathType Leaf)) {
    Write-Host 'DECISION_HISTORY_STATUS=FAIL'
    Write-Host "ERROR=Baseline decision log not found: $BaselinePath"
    exit 1
  }
  $baseline = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $BaselinePath))
  $baselineLabel = 'baseline'
}

$headings = [regex]::Matches($raw, '(?m)^## D(?<id>\d+)\b[^\r\n]*')
if ($headings.Count -eq 0) {
  if ($raw -notmatch '_No decisions yet\._' -or $raw -match '(?m)^\*\*Decision') { $errors.Add('Empty decision logs must explicitly say _No decisions yet._ and contain no decision bodies.') }
} else {
  $seen = @{}
  $ids = [System.Collections.Generic.List[int]]::new()
  foreach ($heading in $headings) {
    $id = [int]$heading.Groups['id'].Value
    $ids.Add($id)
    if ($seen.ContainsKey($id)) {
      $errors.Add("Duplicate decision id D$id.")
    } else {
      $seen[$id] = $true
    }
  }

  for ($i = 1; $i -lt $ids.Count; $i++) {
    if ($ids[$i - 1] -le $ids[$i]) {
      $errors.Add("Decision order is not strictly descending: D$($ids[$i - 1]) precedes D$($ids[$i]).")
    }
  }

  $maximum = ($ids | Measure-Object -Maximum).Maximum
  $minimum = ($ids | Measure-Object -Minimum).Minimum
  foreach ($expected in $maximum..$minimum) {
    if (-not $seen.ContainsKey($expected)) { $errors.Add("Missing decision id D$expected.") }
  }
  if ($minimum -ne 1) { $errors.Add("Decision history ends at D$minimum instead of D1.") }

  $beforeFirst = $raw.Substring(0, $headings[0].Index)
  if ($beforeFirst -match '(?m)^\*\*Decision(?: \([^\r\n]+\))?:\*\*') {
    $errors.Add('Anonymous decision body appears before the first decision heading.')
  }

  for ($i = 0; $i -lt $headings.Count; $i++) {
    $start = $headings[$i].Index + $headings[$i].Length
    $end = if ($i + 1 -lt $headings.Count) { $headings[$i + 1].Index } else { $raw.Length }
    $body = $raw.Substring($start, $end - $start)
    if ($i -eq ($headings.Count - 1)) {
      $body = [regex]::Replace($body, '(?s)<!--\s*Template for a new entry.*?-->', '')
    }
    if ([string]::IsNullOrWhiteSpace(($body -replace '(?m)^---\s*$', ''))) {
      $errors.Add("D$($ids[$i]) has no body.")
      continue
    }
    $bodyStarts = [regex]::Matches($body, '(?m)^\*\*Decision(?: \([^\r\n]+\))?:\*\*')
    if ($bodyStarts.Count -eq 0) { $errors.Add("D$($ids[$i]) needs one **Decision:** body.") }
    if ($bodyStarts.Count -gt 1) {
      $errors.Add("D$($ids[$i]) contains $($bodyStarts.Count) decision bodies; at least one body is anonymous.")
    }
  }
}

# One hashing rule for ratification - the entry's bytes with stamp lines removed, CRLF folded
# to LF and trailing whitespace dropped, so a stamp or a checkout's line endings never break a binding.
function Get-EntryHash([string]$Segment) {
  $bare = ([regex]::Replace($Segment, $stampPattern, '') -replace "`r`n", "`n").TrimEnd()
  $sha = [Security.Cryptography.SHA256]::Create()
  try { return (($sha.ComputeHash([Text.UTF8Encoding]::new($false).GetBytes($bare)) | ForEach-Object { $_.ToString('x2') }) -join '') }
  finally { $sha.Dispose() }
}
$allSegments = if ($headings.Count -gt 0) { Get-EntrySegments $raw } else { @{} }
if ($HashOf) {
  $wanted = if ($HashOf -match '^D?(\d+)$') { [int]$Matches[1] } else { -1 }
  if (-not $allSegments.ContainsKey($wanted)) { Write-Host "ENTRY_HASH_STATUS=FAIL"; Write-Host "ERROR=No entry $HashOf."; exit 1 }
  Write-Host "ENTRY_HASH_STATUS=OK"
  Write-Host "ENTRY_ID=D$wanted"
  Write-Host "ENTRY_HEADING=$(($allSegments[$wanted] -split "`r?`n")[0])"
  Write-Host "ENTRY_SHA256=$(Get-EntryHash $allSegments[$wanted])"
  exit 0
}

$ratified = @{}
$ratificationCount = 0
$ratSurfaces = @('terminal', 'chat', 'operator terminal', 'in person', 'email', 'phone')
$ratText = $null
$ratBaseline = $null
$ratRel = 'protocol/decision-ratifications.jsonl'
if ($FromIndex -and -not $RatificationsPath) {
  $ratText = Read-GitBlob ":$ratRel"
  if (-not $RatificationsBaselinePath) { $ratBaseline = Read-GitBlob "HEAD:$ratRel" }
} else {
  # The live ledger binds the canonical log only; a fixture or copy is checked against the
  # ledger it is given, or none (found when the first real ratification broke every fixture).
  $isCanonicalLog = [IO.Path]::GetFullPath($Path).Equals([IO.Path]::GetFullPath((Join-Path $root 'DECISIONS.md')), [StringComparison]::OrdinalIgnoreCase)
  if (-not $RatificationsPath -and $isCanonicalLog) { $RatificationsPath = Join-Path $root $ratRel }
  if ($RatificationsPath -and (Test-Path -LiteralPath $RatificationsPath -PathType Leaf)) { $ratText = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $RatificationsPath)) }
}
if ($RatificationsBaselinePath -and (Test-Path -LiteralPath $RatificationsBaselinePath -PathType Leaf)) {
  $ratBaseline = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $RatificationsBaselinePath))
}
$ratLines = @(if ($ratText) { ($ratText -replace "`r`n", "`n") -split "`n" | Where-Object { $_.Trim() } })
if ($null -ne $ratBaseline) {
  $baseLines = @(($ratBaseline -replace "`r`n", "`n") -split "`n" | Where-Object { $_.Trim() })
  $prefixOk = $baseLines.Count -le $ratLines.Count
  for ($k = 0; $prefixOk -and $k -lt $baseLines.Count; $k++) { if ($baseLines[$k] -cne $ratLines[$k]) { $prefixOk = $false } }
  if (-not $prefixOk) { $errors.Add("$ratRel is append-only: an existing ratification record was changed or removed.") }
}
$lineNo = 0
foreach ($line in $ratLines) {
  $lineNo++
  try { $rec = $line | ConvertFrom-Json -ErrorAction Stop } catch { $errors.Add("$ratRel line ${lineNo}: not valid JSON."); continue }
  $rid = [string]$rec.id
  $problems = @()
  if ($rid -notmatch '^D(\d+)$') { $problems += 'id must look like D123' }
  if ([string]$rec.entry_sha256 -notmatch '^[0-9a-f]{64}$') { $problems += 'entry_sha256 must be 64 lowercase hex characters' }
  $parsedAt = [DateTimeOffset]::MinValue
  if (-not [DateTimeOffset]::TryParse([string]$rec.ratified_at, [ref]$parsedAt)) { $problems += 'ratified_at must be an ISO timestamp' }
  if ($ratSurfaces -notcontains [string]$rec.surface) { $problems += "surface must be one of: $($ratSurfaces -join ', ')" }
  if ($null -eq $rec.quote -or $rec.quote -isnot [string]) { $problems += 'quote must be a string (empty is allowed)' }
  if ($problems.Count) { $errors.Add("$ratRel line ${lineNo}: $($problems -join '; ')."); continue }
  $target = [int]$rid.Substring(1)
  if (-not $allSegments.ContainsKey($target)) { $errors.Add("$ratRel line ${lineNo}: ratifies $rid, which does not exist."); continue }
  if ((Get-EntryHash $allSegments[$target]) -ne $rec.entry_sha256) {
    $errors.Add("$ratRel line ${lineNo}: $rid no longer matches the hash it was ratified at - the entry changed after ratification.")
    continue
  }
  $ratified[$target] = $true
  $ratificationCount++
}

$authorityChecked = 0
$stampCount = 0
if ($headings.Count -gt 0) {
  $segments = Get-EntrySegments $raw
  $class = @{}      # id -> operator-ruled | proposed (entries from -AuthorityFrom on)
  $dates = @{}
  $claims = @{}     # "target|kind|by" -> $true, from supersession fields
  foreach ($id in $segments.Keys) {
    $heading = [regex]::Match($segments[$id], '^## D\d+ [-—] (?<date>\d{4}-\d{2}-\d{2})')
    if ($heading.Success) { $dates[$id] = $heading.Groups['date'].Value }
  }
  foreach ($id in ($segments.Keys | Where-Object { $_ -ge $AuthorityFrom } | Sort-Object -Descending)) {
    $text = $segments[$id]
    $authorityChecked++
    $lines = @([regex]::Matches($text, '(?m)^\*\*Authority:\*\*[ \t]*(?<v>[^\r\n]*?)[ \t]*\r?$'))
    if ($lines.Count -ne 1) {
      $errors.Add("D$id needs exactly one **Authority:** line (operator-ruled or proposed); found $($lines.Count).")
      continue
    }
    $value = $lines[0].Groups['v'].Value
    switch -Exact ($value) {
      'operator-ruled' {
        if ($text -notmatch $quotePattern) {
          $errors.Add("D$id is operator-ruled but has no **Operator said (<surface>, YYYY-MM-DD):** `"<quote of 12+ characters>`" line.")
        }
        $class[$id] = $value
      }
      'proposed' { $class[$id] = $value }
      'operator-ratified' { $errors.Add("D$id types operator-ratified; that class is set only by the operator's ratification record, never typed.") }
      'record' { $errors.Add("D$id is a record; a work report belongs in a handoff, not the decision log.") }
      default { $errors.Add("D$id has unknown authority '$value'; use operator-ruled or proposed.") }
    }
    foreach ($field in $fieldKinds.Keys) {
      $kind = $fieldKinds[$field]
      $fieldLines = @([regex]::Matches($text, "(?m)^\*\*$([regex]::Escape($field)):\*\*[ \t]*(?<v>[^\r\n]*?)[ \t]*\r?$"))
      if ($fieldLines.Count -gt 1) { $errors.Add("D$id has more than one **${field}:** line."); continue }
      if ($fieldLines.Count -eq 0) { continue }
      $list = $fieldLines[0].Groups['v'].Value
      if ($list -notmatch '^D\d+(?:\s*,\s*D\d+)*$') { $errors.Add("D$id **${field}:** must be a comma-separated list of ids like D12, D34."); continue }
      foreach ($target in @([regex]::Matches($list, 'D(\d+)') | ForEach-Object { [int]$_.Groups[1].Value })) {
        if ($target -ge $id -or -not $segments.ContainsKey($target)) { $errors.Add("D$id lists D$target under **${field}:**, which is not an older existing entry."); continue }
        $claims["$target|$kind|$id"] = $true
        if ($class[$id] -eq 'proposed') {
          if ($kind -eq 'retired') { $errors.Add("D$id is proposed and cannot retire D$target; only an operator ruling retires.") }
          elseif ($ratified.ContainsKey($target) -or -not ($target -ge $AuthorityFrom -and $segments[$target] -match '(?m)^\*\*Authority:\*\*[ \t]*proposed[ \t]*\r?$')) { $errors.Add("D$id is proposed and cannot supersede D$target, which is not an unratified proposal; a ruling yields only to a ruling.") }
        }
      }
    }
  }
  foreach ($id in $segments.Keys) {
    foreach ($stamp in [regex]::Matches($segments[$id], $stampParse)) {
      $stampCount++
      $by = [int]$stamp.Groups['by'].Value
      $kind = $stamp.Groups['kind'].Value
      if (-not $claims.ContainsKey("$id|$kind|$by")) {
        $errors.Add("D$id carries '_Status: $kind by D$($by)_' but D$by does not list D$id under the matching field.")
      } elseif ($dates[$by] -and $stamp.Groups['date'].Value -ne $dates[$by]) {
        $errors.Add("D$id stamp '$kind by D$by' is dated $($stamp.Groups['date'].Value), not D$by's date $($dates[$by]).")
      }
    }
  }
  foreach ($claim in $claims.Keys) {
    $target, $kind, $by = $claim -split '\|'
    $found = @([regex]::Matches($segments[[int]$target], $stampParse) | Where-Object { $_.Groups['kind'].Value -eq $kind -and $_.Groups['by'].Value -eq $by })
    if ($found.Count -eq 0) { $errors.Add("D$by lists D$target as $kind but D$target has no '_Status: $kind by D$by (date)_' stamp; append with bin/add-decision.ps1.") }
  }
}

$appendOnly = 'not-checked'
if ($null -ne $baseline) {
  $before = Get-EntrySegments $baseline
  $after = Get-EntrySegments $raw
  foreach ($id in ($before.Keys | Sort-Object -Descending)) {
    if (-not $after.ContainsKey($id)) {
      $errors.Add("D$id exists in $baselineLabel but was removed; existing entries are append-only.")
      continue
    }
    $old = $before[$id]
    $new = $after[$id]
    if ($new -ceq $old) { continue }
    # Only added stamp lines are permitted; every stamp already in the baseline must survive.
    $oldStamps = @([regex]::Matches($old, $stampPattern) | ForEach-Object { $_.Value.TrimEnd("`r", "`n") })
    $newStamps = [System.Collections.Generic.List[string]]::new()
    foreach ($m in [regex]::Matches($new, $stampPattern)) { $newStamps.Add($m.Value.TrimEnd("`r", "`n")) }
    $stampsKept = $true
    foreach ($stamp in $oldStamps) { if (-not $newStamps.Remove($stamp)) { $stampsKept = $false } }
    $oldBare = [regex]::Replace($old, $stampPattern, '')
    $newBare = [regex]::Replace($new, $stampPattern, '')
    if ($stampsKept -and $newBare -ceq $oldBare) { continue }
    $oldLines = $oldBare -split "`n"
    $newLines = $newBare -split "`n"
    $line = 0
    while ($line -lt $oldLines.Count -and $line -lt $newLines.Count -and $oldLines[$line] -ceq $newLines[$line]) { $line++ }
    $errors.Add("D$id differs from $baselineLabel (first difference at entry line $($line + 1)); existing entries are append-only - record a correction as a new entry.")
  }
  $appendOnly = "checked baseline=$baselineLabel"
}

if ($errors.Count -gt 0) {
  Write-Host 'DECISION_HISTORY_STATUS=FAIL'
  foreach ($message in $errors) { Write-Host "ERROR=$message" }
  exit 1
}

Write-Host "DECISION_HISTORY_STATUS=OK decisions=$($headings.Count) authority_from=$AuthorityFrom append_only=$appendOnly authority_checked=$authorityChecked stamps=$stampCount ratified=$ratificationCount"
exit 0
