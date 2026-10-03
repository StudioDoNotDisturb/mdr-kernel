#requires -Version 7.0
<#
.SYNOPSIS
  Envelope length check: the Markdown retrieval envelope is a label, not a summary.
.DESCRIPTION
  Title is a name of at most 100 characters. Context is ONE sentence of at most 300 characters
  saying when an agent should use the file ("Use when ..."). Provenance, bylines, approval quotes,
  evidence and caveats belong in the body. A check that only tests whether the fields exist lets
  headers grow into paragraphs, so this is a ratchet: pre-commit blocks a staged .md over the
  limits, and doctor counts the untouched rest. See reference/markdown-system-node-standard.md.
.EXAMPLE
  pwsh -NoProfile -File bin/envelope-check.ps1 docs/example.md   # exit 1 if a file is over
.EXAMPLE
  pwsh -NoProfile -File bin/envelope-check.ps1 -Staged           # pre-commit (reads the index)
.EXAMPLE
  pwsh -NoProfile -File bin/envelope-check.ps1 -Count -List      # doctor, advisory
#>
[CmdletBinding(DefaultParameterSetName = 'Path')]
param(
  [Parameter(ParameterSetName = 'Path', Position = 0, ValueFromRemainingArguments = $true)]
  [string[]]$Path,
  [Parameter(ParameterSetName = 'Staged')][switch]$Staged,
  [Parameter(ParameterSetName = 'Count')][switch]$Count,
  [Parameter(ParameterSetName = 'Count')][switch]$List
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)  # git show must decode as UTF-8
$TitleMax = 100
$ContextMax = 300

function Measure-Sentences([string]$Value) {
  # Split after . ! ? before a capital or digit, but not after e.g. / No. / U.S. / initials.
  $count = 0
  $carry = ''
  foreach ($part in [regex]::Split($Value.Trim(), '(?<=[.!?])\s+(?=["''(\[`]?[A-Z0-9])')) {
    $carry = if ($carry) { "$carry $part" } else { $part }
    if (-not [regex]::IsMatch($carry, '(?:\b(?:e\.g|i\.e|etc|vs|No|Nos|St|Dr|Mr|Mrs|Ms|Jr|Inc|approx|cf|ca|p|pp)|\b[A-Z])\.$')) {
      $count++
      $carry = ''
    }
  }
  $count + [int][bool]$carry
}

function Get-EnvelopeProblems([string]$Text) {
  $Text = $Text.TrimStart([char]0xFEFF) -replace "`r`n", "`n"
  if ($Text.StartsWith("---`n")) {
    $end = $Text.IndexOf("`n---", 4)
    $lines = if ($end -gt 0) { $Text.Substring(4, $end - 4) -split "`n" } else { @() }
  } else {
    $lines = @($Text -split "`n" | Where-Object { $_.Trim() } | Select-Object -First 3)
    # A Context wrapped onto following lines is still one value: join the physical lines up to the
    # next blank line or heading, so wrapping cannot hide a paragraph from the limit.
    $phys = $Text -split "`n"
    for ($i = 0; $i -lt $phys.Count; $i++) {
      if ($phys[$i].StartsWith('Context:')) {
        $j = $i + 1
        while ($j -lt $phys.Count -and $phys[$j].Trim() -and -not $phys[$j].StartsWith('#')) { $j++ }
        $joined = ($phys[$i..($j - 1)] | ForEach-Object { $_.Trim() }) -join ' '
        $lines = @($lines | ForEach-Object { if ($_.StartsWith('Context:')) { $joined } else { $_ } })
        break
      }
    }
  }
  $found = @{}
  foreach ($line in $lines) {
    $m = [regex]::Match($line, '^(Title|Context):\s+(.+?)\s*$')
    if ($m.Success -and -not $found.ContainsKey($m.Groups[1].Value)) {
      $value = $m.Groups[2].Value
      if ($value.Length -ge 2 -and $value[0] -eq $value[-1] -and "$($value[0])" -in '"', "'") {
        $value = $value.Substring(1, $value.Length - 2)
      }
      $found[$m.Groups[1].Value] = $value
    }
  }
  $problems = @()
  if ($found.ContainsKey('Title') -and $found.Title.Length -gt $TitleMax) {
    $problems += "Title is $($found.Title.Length) chars (limit $TitleMax): make it a name; move the summary into the body"
  }
  if ($found.ContainsKey('Context')) {
    $n = Measure-Sentences $found.Context
    if ($found.Context.Length -gt $ContextMax -or $n -gt 1) {
      $problems += "Context is $($found.Context.Length) chars / $n sentences (limit $ContextMax chars, 1 sentence): " +
        'say when an agent should use this file; move provenance, approval quotes, evidence and caveats into the body'
    }
  }
  , $problems
}

if ($Staged) {
  $names = @((git diff --cached --name-only --diff-filter=ACMR -z) -split "`0" | Where-Object { $_ -like '*.md' })
  if ($LASTEXITCODE) { throw 'git diff --cached failed' }
  $bad = 0
  foreach ($name in $names) {
    $blob = (git show ":$name") -join "`n"
    if ($LASTEXITCODE) { throw "git show :$name failed" }
    foreach ($p in (Get-EnvelopeProblems $blob)) { Write-Output "X ${name}: $p"; $bad++ }
  }
  if ($bad) { Write-Output 'X envelope length: fix the staged header(s) above (reference/markdown-system-node-standard.md)' }
  exit ([int][bool]$bad)
}

if ($Count) {
  $top = (git rev-parse --show-toplevel).Trim()
  $tracked = @((git -C $top ls-files -z '*.md') -split "`0" | Where-Object { $_ })
  $over = @(foreach ($f in $tracked) {
    $full = Join-Path $top $f
    if ((Test-Path -LiteralPath $full) -and (Get-EnvelopeProblems ([IO.File]::ReadAllText($full))).Count) { $f }
  })
  Write-Output "ENVELOPE_LENGTH over=$($over.Count) tracked=$($tracked.Count)"
  if ($List) { $over | Write-Output }
  exit 0
}

if (-not $Path) { Get-Help $PSCommandPath; exit 2 }
$bad = 0
foreach ($f in $Path) {
  foreach ($p in (Get-EnvelopeProblems ([IO.File]::ReadAllText((Resolve-Path -LiteralPath $f))))) {
    Write-Output "X ${f}: $p"
    $bad++
  }
}
exit ([int][bool]$bad)
