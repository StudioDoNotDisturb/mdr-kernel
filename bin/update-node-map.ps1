param(
  [switch]$Check,
  [string]$Path,
  [switch]$Repair,
  [switch]$Apply
)

$ErrorActionPreference = 'Stop'

$ScriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$BrainRoot = [System.IO.Path]::GetFullPath((Join-Path $ScriptRoot '..'))
$RegistryPath = Join-Path $BrainRoot 'maps/node-roots.json'
$NodeMapJsonPath = Join-Path $BrainRoot 'maps/node-map.json'
$NodeMapMarkdownPath = Join-Path $BrainRoot 'maps/NODE-MAP.md'
$ObsidianNodeLinksPath = Join-Path $BrainRoot 'maps/OBSIDIAN-NODE-LINKS.md'

function Convert-ToForwardSlash([string]$Value) {
  return ($Value -replace '\\', '/')
}

function Get-FullPathSafe([string]$Value) {
  return [System.IO.Path]::GetFullPath($Value)
}

function Resolve-InputPath([string]$Value) {
  if ([System.IO.Path]::IsPathRooted($Value)) { return Get-FullPathSafe $Value }
  return Get-FullPathSafe (Join-Path $BrainRoot $Value)
}

function Test-PathUnderRoot([string]$TargetPath, [string]$RootPath) {
  $target = (Get-FullPathSafe $TargetPath).TrimEnd([char[]]@('\', '/'))
  $root = (Get-FullPathSafe $RootPath).TrimEnd([char[]]@('\', '/'))
  return (
    $target.Equals($root, [System.StringComparison]::OrdinalIgnoreCase) -or
    $target.StartsWith("$root\", [System.StringComparison]::OrdinalIgnoreCase) -or
    $target.StartsWith("$root/", [System.StringComparison]::OrdinalIgnoreCase)
  )
}

function Resolve-RootPath($RootRecord) {
  $raw = [string]$RootRecord.path
  if ([System.IO.Path]::IsPathRooted($raw)) { return Get-FullPathSafe $raw }
  return Get-FullPathSafe (Join-Path $BrainRoot $raw)
}

function Get-RegisteredRootForPath {
  param(
    [Parameter(Mandatory)][string]$FullPath,
    [Parameter(Mandatory)]$Registry
  )

  foreach ($rootRecord in $Registry) {
    if (Test-PathUnderRoot -TargetPath $FullPath -RootPath (Resolve-RootPath $rootRecord)) {
      return $rootRecord
    }
  }
  return $null
}

function Assert-UnderBrain([string]$TargetPath) {
  if (-not (Test-PathUnderRoot -TargetPath $TargetPath -RootPath $BrainRoot)) {
    throw "Refusing to write outside mdr-kernel root: $(Get-FullPathSafe $TargetPath)"
  }
}

function Write-AtomicIfChanged {
  param(
    [Parameter(Mandatory)][string]$TargetPath,
    [Parameter(Mandatory)][string]$Content,
    [string[]]$IgnoreLinePatterns = @()
  )

  Assert-UnderBrain $TargetPath
  $dir = Split-Path -Parent $TargetPath
  if (-not (Test-Path -LiteralPath $dir)) {
    New-Item -ItemType Directory -Path $dir | Out-Null
  }
  $Content = (($Content -replace "`r`n", "`n") -replace "`r", "`n")

  $write = $true
  if (Test-Path -LiteralPath $TargetPath) {
    $old = Get-Content -LiteralPath $TargetPath -Raw
    $oldComparable = $old
    $newComparable = $Content
    foreach ($pattern in $IgnoreLinePatterns) {
      $oldComparable = (($oldComparable -replace "`r`n", "`n") -split "`n" | Where-Object { $_ -notmatch $pattern }) -join "`n"
      $newComparable = (($newComparable -replace "`r`n", "`n") -split "`n" | Where-Object { $_ -notmatch $pattern }) -join "`n"
    }
    $write = -not [string]::Equals($oldComparable, $newComparable, [System.StringComparison]::Ordinal)
  }

  if (-not $write) {
    Write-Host "unchanged $TargetPath"
    return $false
  }

  $tmp = Join-Path $dir ('.' + [System.IO.Path]::GetFileName($TargetPath) + '.' + [guid]::NewGuid().ToString('N') + '.tmp')
  Assert-UnderBrain $tmp
  [System.IO.File]::WriteAllText($tmp, $Content, [System.Text.UTF8Encoding]::new($false))
  Move-Item -LiteralPath $tmp -Destination $TargetPath -Force
  Write-Host "wrote $TargetPath"
  return $true
}

function Get-NormalizedContent {
  param(
    [Parameter(Mandatory)][string]$Content,
    [string[]]$IgnoreLinePatterns = @()
  )

  $normalized = (($Content -replace "`r`n", "`n") -replace "`r", "`n")
  foreach ($pattern in $IgnoreLinePatterns) {
    $normalized = (($normalized -split "`n") | Where-Object { $_ -notmatch $pattern }) -join "`n"
  }
  return $normalized
}

function Test-GeneratedOutputFresh {
  param(
    [Parameter(Mandatory)][string]$Path,
    [Parameter(Mandatory)][string]$Content,
    [string[]]$IgnoreLinePatterns = @()
  )

  if (-not (Test-Path -LiteralPath $Path)) { return $false }
  $current = Get-Content -LiteralPath $Path -Raw
  $currentComparable = Get-NormalizedContent -Content $current -IgnoreLinePatterns $IgnoreLinePatterns
  $expectedComparable = Get-NormalizedContent -Content $Content -IgnoreLinePatterns $IgnoreLinePatterns
  return [string]::Equals($currentComparable, $expectedComparable, [System.StringComparison]::Ordinal)
}

function Test-WildcardAny([string]$RelPath, $Patterns) {
  if (-not $Patterns) { return $false }
  foreach ($pattern in $Patterns) {
    $wc = [System.Management.Automation.WildcardPattern]::new((Convert-ToForwardSlash $pattern), 'IgnoreCase')
    if ($wc.IsMatch($RelPath)) { return $true }
  }
  return $false
}

function Get-RelativePath([string]$Root, [string]$File) {
  return Convert-ToForwardSlash ([System.IO.Path]::GetRelativePath($Root, $File))
}

function Normalize-RelativePath([string]$Value) {
  $normalized = Convert-ToForwardSlash $Value
  $parts = [System.Collections.Generic.List[string]]::new()
  foreach ($part in ($normalized -split '/')) {
    if (-not $part -or $part -eq '.') { continue }
    if ($part -eq '..') {
      if ($parts.Count -gt 0 -and $parts[$parts.Count - 1] -ne '..') {
        $parts.RemoveAt($parts.Count - 1)
      } else {
        $parts.Add($part)
      }
      continue
    }
    $parts.Add($part)
  }
  return ($parts -join '/')
}

function Get-RootFiles($RootRecord) {
  $rootPath = Resolve-RootPath $RootRecord
  if (-not (Test-Path -LiteralPath $rootPath)) { return @() }
  $files = Get-ChildItem -LiteralPath $rootPath -Recurse -File -Filter '*.md' -ErrorAction SilentlyContinue
  $out = [System.Collections.Generic.List[object]]::new()
  foreach ($file in $files) {
    $rel = Get-RelativePath $rootPath $file.FullName
    if (Test-WildcardAny $rel $RootRecord.excludes) { continue }
    if ($RootRecord.includes -and -not (Test-WildcardAny $rel $RootRecord.includes)) { continue }
    $out.Add($file)
  }
  return $out
}

function Get-GitTrackedSet([string]$RootPath) {
  $set = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
  $inside = & git -C $RootPath rev-parse --is-inside-work-tree 2>$null
  if ($LASTEXITCODE -ne 0 -or $inside -notmatch 'true') { return $set }
  $tracked = & git -C $RootPath ls-files '*.md'
  foreach ($entry in $tracked) {
    if ($entry) { [void]$set.Add((Convert-ToForwardSlash $entry)) }
  }
  return $set
}

function Test-TrackGit($RootRecord) {
  if ($RootRecord.PSObject.Properties.Name -contains 'trackGit') { return [bool]$RootRecord.trackGit }
  return ($RootRecord.writePolicy -eq 'repair-allowed')
}

function Get-StrictNodeVerdict([string]$Text) {
  return [regex]::IsMatch(
    $Text.TrimEnd(),
    '---\n\n## SYSTEM NODE LINKS\n\nDirectory Path: [^\n]+\nNext Sequential File: [^\n]+\nRelated Workflows: [^\n]+\nAI Execution Prompts: [^\n]+\z'
  )
}

function Normalize-EnvelopeValue([string]$Value) {
  $v = $Value.Trim()
  if ($v.Length -ge 2 -and $v.StartsWith('"') -and $v.EndsWith('"')) {
    return ($v.Substring(1, $v.Length - 2) -replace '\\"', '"')
  }
  if ($v.Length -ge 2 -and $v.StartsWith("'") -and $v.EndsWith("'")) {
    return ($v.Substring(1, $v.Length - 2) -replace "''", "'")
  }
  return $v
}

function Get-NodeRepairPlan([string]$Text) {
  # Detects exactly two mechanical shape defects around an otherwise-correct terminal node block:
  # (1) missing the '---' blank-line separator immediately before the heading, (2) trailing
  # content after the four canonical fields that breaks the terminal anchor. Returns $null when
  # the block cannot be repaired mechanically (fields missing/reordered, no node block at all).
  $trimmed = $Text.TrimEnd()
  $fieldPattern = 'Directory Path: [^\n]+\nNext Sequential File: [^\n]+\nRelated Workflows: [^\n]+\nAI Execution Prompts: [^\n]+'
  $m = [regex]::Match($trimmed, "## SYSTEM NODE LINKS\n\n($fieldPattern)")
  if (-not $m.Success) { return $null }

  $fieldsBlock = $m.Groups[1].Value
  $headingStart = $m.Index
  $beforeHeading = $trimmed.Substring(0, $headingStart)
  $fieldsEnd = $m.Index + $m.Length
  $trailing = $trimmed.Substring($fieldsEnd)

  $needsSeparator = -not $beforeHeading.EndsWith("---`n`n")
  $needsTruncate = ($trailing.Trim().Length -gt 0)

  if (-not $needsSeparator -and -not $needsTruncate) { return $null }

  if ($needsSeparator) {
    $newBeforeHeading = $beforeHeading.TrimEnd("`n")
    if ($newBeforeHeading.Length -gt 0) { $newBeforeHeading += "`n`n---`n`n" } else { $newBeforeHeading = "---`n`n" }
  } else {
    $newBeforeHeading = $beforeHeading
  }

  $newText = $newBeforeHeading + '## SYSTEM NODE LINKS' + "`n`n" + $fieldsBlock + "`n"

  $actions = [System.Collections.Generic.List[string]]::new()
  if ($needsSeparator) { $actions.Add("insert blank line + '---' before '## SYSTEM NODE LINKS'") }
  if ($needsTruncate) { $actions.Add("remove trailing content after the node block ($($trailing.Trim().Length) chars)") }

  return [ordered]@{
    newText = $newText
    actions = @($actions)
  }
}

function Parse-Envelope {
  param([string]$Text, [string]$FallbackTitle)

  $lines = $Text -split "`n"
  $nonEmpty = @($lines | Where-Object { $_.Trim() })
  if ($nonEmpty.Count -ge 3 -and
      $nonEmpty[0] -match '^Title:\s+(.+)' -and
      $nonEmpty[1] -match '^Type:\s+(.+)' -and
      $nonEmpty[2] -match '^Context:\s+(.+)') {
    return [ordered]@{
      title = Normalize-EnvelopeValue ($nonEmpty[0] -replace '^Title:\s+', '')
      type = Normalize-EnvelopeValue ($nonEmpty[1] -replace '^Type:\s+', '')
      context = Normalize-EnvelopeValue ($nonEmpty[2] -replace '^Context:\s+', '')
      envelopeTop = 'strict'
    }
  }

  $fm = [regex]::Match($Text, '(?s)^---\n(.*?)\n---\n')
  if ($fm.Success) {
    $frontMatter = $fm.Groups[1].Value
    $exact = @{
      title = [regex]::Match($frontMatter, '(?m)^Title:\s+(.+)$')
      type = [regex]::Match($frontMatter, '(?m)^Type:\s+(.+)$')
      context = [regex]::Match($frontMatter, '(?m)^Context:\s+(.+)$')
    }
    if ($exact.title.Success -and $exact.type.Success -and $exact.context.Success) {
      return [ordered]@{
        title = Normalize-EnvelopeValue $exact.title.Groups[1].Value
        type = Normalize-EnvelopeValue $exact.type.Groups[1].Value
        context = Normalize-EnvelopeValue $exact.context.Groups[1].Value
        envelopeTop = 'strict'
      }
    }

    $ci = @{
      title = [regex]::Match($frontMatter, '(?im)^title:\s+(.+)$')
      type = [regex]::Match($frontMatter, '(?im)^type:\s+(.+)$')
      context = [regex]::Match($frontMatter, '(?im)^context:\s+(.+)$')
    }
    $found = @($ci.title.Success, $ci.type.Success, $ci.context.Success) | Where-Object { $_ }
    if ($found.Count -eq 3) {
      return [ordered]@{
        title = Normalize-EnvelopeValue $ci.title.Groups[1].Value
        type = Normalize-EnvelopeValue $ci.type.Groups[1].Value
        context = Normalize-EnvelopeValue $ci.context.Groups[1].Value
        envelopeTop = 'legacy'
      }
    } elseif ($found.Count -gt 0) {
      return [ordered]@{
        title = if ($ci.title.Success) { Normalize-EnvelopeValue $ci.title.Groups[1].Value } else { $FallbackTitle }
        type = if ($ci.type.Success) { Normalize-EnvelopeValue $ci.type.Groups[1].Value } else { '' }
        context = if ($ci.context.Success) { Normalize-EnvelopeValue $ci.context.Groups[1].Value } else { '' }
        envelopeTop = 'partial'
      }
    }
  }

  $loose = @{}
  foreach ($line in ($nonEmpty | Select-Object -First 30)) {
    $match = [regex]::Match($line, '^\s*(?:>\s*)?(?:[-*]\s*)?(?:\*\*)?(Title|Type|Context)(?:\*\*)?\s*:\s*(.+?)\s*(?:\*\*)?\s*$', 'IgnoreCase')
    if ($match.Success) {
      $key = $match.Groups[1].Value.ToLowerInvariant()
      if (-not $loose.ContainsKey($key)) { $loose[$key] = $match.Groups[2].Value.Trim() }
    }
  }
  if ($loose.Count -eq 3) {
    return [ordered]@{
      title = Normalize-EnvelopeValue $loose.title
      type = Normalize-EnvelopeValue $loose.type
      context = Normalize-EnvelopeValue $loose.context
      envelopeTop = 'legacy'
    }
  } elseif ($loose.Count -gt 0) {
    return [ordered]@{
      title = if ($loose.ContainsKey('title')) { Normalize-EnvelopeValue $loose.title } else { $FallbackTitle }
      type = if ($loose.ContainsKey('type')) { Normalize-EnvelopeValue $loose.type } else { '' }
      context = if ($loose.ContainsKey('context')) { Normalize-EnvelopeValue $loose.context } else { '' }
      envelopeTop = 'partial'
    }
  }

  $heading = [regex]::Match($Text, '(?m)^#\s+(.+?)\s*$')
  return [ordered]@{
    title = if ($heading.Success) { $heading.Groups[1].Value.Trim() } else { $FallbackTitle }
    type = ''
    context = ''
    envelopeTop = 'missing'
  }
}

function Normalize-LinkValue {
  param([string]$Value)
  $v = ($Value -replace '\*\*', '').Trim()
  $v = $v.Trim('`').Trim()
  $bracketTrimmed = $v.Trim()

  $markdown = [regex]::Match($bracketTrimmed, '^\[[^\]]+\]\(([^)]+)\)$')
  if ($markdown.Success) {
    $bracketTrimmed = $markdown.Groups[1].Value.Trim()
  } else {
    $wiki = [regex]::Match($bracketTrimmed, '^\[\[([^\]|#]+)(?:[#|][^\]]*)?\]\]$')
    if ($wiki.Success) {
      $bracketTrimmed = $wiki.Groups[1].Value.Trim()
      if ($bracketTrimmed -and
          -not $bracketTrimmed.EndsWith('/') -and
          -not ($bracketTrimmed -match '\.[A-Za-z0-9]{1,8}$')) {
        $bracketTrimmed = "$bracketTrimmed.md"
      }
    }
  }

  try { $bracketTrimmed = [System.Uri]::UnescapeDataString($bracketTrimmed) } catch {}

  if ($bracketTrimmed.StartsWith('[') -and $bracketTrimmed.EndsWith(']')) {
    $bracketTrimmed = $bracketTrimmed.Substring(1, $bracketTrimmed.Length - 2).Trim()
  }
  if ($bracketTrimmed -match '^(?i:n/?a\b|none\b|tbd\b)') { return '' }
  return $bracketTrimmed
}

function Split-LinkValues([string]$Value) {
  $parts = [System.Collections.Generic.List[string]]::new()
  foreach ($part in ($Value -split ',')) {
    $clean = Normalize-LinkValue $part
    if ($clean) { $parts.Add($clean) }
  }
  return @($parts)
}

function Select-PathLikeValues {
  param(
    [string[]]$Values,
    [System.Collections.Generic.List[string]]$Issues,
    [string]$Key
  )
  $out = [System.Collections.Generic.List[string]]::new()
  foreach ($value in $Values) {
    if (Test-PathLikeValue $value) { $out.Add($value) }
    elseif ($value) { $Issues.Add("note:${Key}:$value") }
  }
  return @($out)
}

function Test-PathLikeValue([string]$Value) {
  if (-not $Value) { return $false }
  if ($Value -match '\s' -and $Value -notmatch '^[A-Za-z]:[\\/]') { return $false }
  return (
    $Value -match '[\\/]' -or
    $Value -match '\.[A-Za-z0-9]{1,8}$' -or
    $Value -match '^\.\.?(?:/|\\)' -or
    $Value -match '^[A-Za-z0-9_.-]+\.(md|ps1|json|mjs|js|py|txt)$'
  )
}

function Resolve-CrossRootLink {
  param(
    [string]$Value,
    [System.Collections.Generic.List[object]]$OtherRoots
  )

  $normalized = Convert-ToForwardSlash $Value.Trim()
  $forms = [System.Collections.Generic.List[string]]::new()
  if ($normalized.StartsWith('/')) { $forms.Add($normalized.TrimStart('/')) }
  $forms.Add([System.IO.Path]::GetFileName($normalized))

  $hits = [System.Collections.Generic.List[object]]::new()
  foreach ($other in $OtherRoots) {
    foreach ($form in $forms) {
      $candidate = $form.TrimEnd('/')
      if ($other.Index.relative.Contains($candidate)) {
        $hits.Add(@{ root = $other.Name; target = $candidate })
        break
      }
      $basename = [System.IO.Path]::GetFileName($form).ToLowerInvariant()
      if ($basename -and $other.Index.basename.ContainsKey($basename)) {
        $matches = @($other.Index.basename[$basename])
        if ($matches.Count -eq 1) {
          $hits.Add(@{ root = $other.Name; target = $matches[0] })
          break
        }
      }
    }
  }

  if ($hits.Count -eq 1) { return @{ kind = 'resolved'; value = $Value; resolvedRoot = $hits[0].root; target = $hits[0].target } }
  if ($hits.Count -gt 1) { return @{ kind = 'ambiguous'; value = $Value } }
  return @{ kind = 'broken'; value = $Value }
}

function Build-RootIndex([string]$RootPath, $RootRecord) {
  $index = @{
    relative = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
    basename = @{}
  }
  $linkIndexExcludes = @($RootRecord.excludes | Where-Object { (Convert-ToForwardSlash $_) -ne 'maps/**' })
  $inside = & git -C $RootPath rev-parse --is-inside-work-tree 2>$null
  if ($LASTEXITCODE -eq 0 -and $inside -match 'true') {
    $gitPaths = & git -C $RootPath ls-files --cached --others --exclude-standard 2>$null
    foreach ($entry in $gitPaths) {
      if (-not $entry) { continue }
      $rel = Normalize-RelativePath $entry
      if (Test-WildcardAny $rel $linkIndexExcludes) { continue }
      if ($RootRecord.includes -and -not (Test-WildcardAny $rel $RootRecord.includes)) { continue }
      [void]$index.relative.Add($rel)
      $name = [System.IO.Path]::GetFileName($rel).ToLowerInvariant()
      if ($name) {
        if (-not $index.basename.ContainsKey($name)) { $index.basename[$name] = [System.Collections.Generic.List[string]]::new() }
        $index.basename[$name].Add($rel)
      }
      $dir = Convert-ToForwardSlash ([System.IO.Path]::GetDirectoryName($rel))
      while ($dir -and $dir -ne '.') {
        [void]$index.relative.Add($dir.TrimEnd('/'))
        $parent = Convert-ToForwardSlash ([System.IO.Path]::GetDirectoryName($dir))
        if ($parent -eq $dir) { break }
        $dir = $parent
      }
    }
    [void]$index.relative.Add('')
    if ((Get-FullPathSafe $RootPath).TrimEnd('\', '/').Equals((Get-FullPathSafe $BrainRoot).TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)) {
      foreach ($generated in @(
        'STATE.md',
        'DECISIONS.md',
        'CLAUDE.md',
        'AGENTS.md',
        'GEMINI.md',
        'handoffs',
        'handoffs/INDEX.md',
        'reference',
        'reference/INDEX.md',
        'specs',
        'specs/INDEX.md',
        'inbox',
        'inbox/README.md',
        'protocol/advisory.md',
        'protocol/recall-gates.json',
        'maps',
        'maps/node-roots.json',
        'maps/node-map.json',
        'maps/NODE-MAP.md',
        'maps/OBSIDIAN-NODE-LINKS.md'
      )) {
        [void]$index.relative.Add($generated)
        $name = [System.IO.Path]::GetFileName($generated).ToLowerInvariant()
        if (-not $index.basename.ContainsKey($name)) { $index.basename[$name] = [System.Collections.Generic.List[string]]::new() }
        if (-not $index.basename[$name].Contains($generated)) { $index.basename[$name].Add($generated) }
      }
      [void]$index.relative.Add('maps')
    }
    return $index
  }

  foreach ($item in (Get-ChildItem -LiteralPath $RootPath -Recurse -File -ErrorAction SilentlyContinue)) {
    $rel = Get-RelativePath $RootPath $item.FullName
    if (Test-WildcardAny $rel $linkIndexExcludes) { continue }
    [void]$index.relative.Add($rel)
    $name = $item.Name.ToLowerInvariant()
    if (-not $index.basename.ContainsKey($name)) { $index.basename[$name] = [System.Collections.Generic.List[string]]::new() }
    $index.basename[$name].Add($rel)
  }
  foreach ($dir in (Get-ChildItem -LiteralPath $RootPath -Recurse -Directory -ErrorAction SilentlyContinue)) {
    $rel = (Get-RelativePath $RootPath $dir.FullName).TrimEnd('/')
    if (Test-WildcardAny $rel $linkIndexExcludes) { continue }
    [void]$index.relative.Add($rel)
  }
  [void]$index.relative.Add('')
  if ((Get-FullPathSafe $RootPath).TrimEnd('\', '/').Equals((Get-FullPathSafe $BrainRoot).TrimEnd('\', '/'), [System.StringComparison]::OrdinalIgnoreCase)) {
      foreach ($generated in @(
        'STATE.md',
        'DECISIONS.md',
        'CLAUDE.md',
        'AGENTS.md',
        'GEMINI.md',
        'handoffs',
        'handoffs/INDEX.md',
        'reference',
        'reference/INDEX.md',
        'specs',
        'specs/INDEX.md',
        'inbox',
        'inbox/README.md',
        'protocol/advisory.md',
        'protocol/recall-gates.json',
        'maps',
        'maps/node-roots.json',
        'maps/node-map.json',
        'maps/NODE-MAP.md',
        'maps/OBSIDIAN-NODE-LINKS.md'
      )) {
        [void]$index.relative.Add($generated)
        $name = [System.IO.Path]::GetFileName($generated).ToLowerInvariant()
        if (-not $index.basename.ContainsKey($name)) { $index.basename[$name] = [System.Collections.Generic.List[string]]::new() }
        if (-not $index.basename[$name].Contains($generated)) { $index.basename[$name].Add($generated) }
    }
    [void]$index.relative.Add('maps')
  }
  return $index
}

function Resolve-NodeLink {
  param(
    [string]$Value,
    [string]$FileRelDir,
    [string]$RootPath,
    $RootIndex,
    [string]$RootName,
    [System.Collections.Generic.List[object]]$OtherRoots
  )

  if (-not (Test-PathLikeValue $Value)) { return @{ kind = 'note'; value = $Value } }
  $normalized = Convert-ToForwardSlash $Value.Trim()
  $candidates = [System.Collections.Generic.List[string]]::new()
  if ($normalized.StartsWith('/')) {
    $candidates.Add($normalized.TrimStart('/'))
  } else {
    if ($FileRelDir) { $candidates.Add((Convert-ToForwardSlash (Join-Path $FileRelDir $normalized))) }
    $candidates.Add($normalized)
  }

  foreach ($candidate in $candidates) {
    $candidate = (Normalize-RelativePath $candidate).TrimEnd('/')
    if ($candidate -eq '.') { $candidate = '' }
    if ($RootIndex.relative.Contains($candidate)) { return @{ kind = 'resolved'; value = $Value; target = $candidate } }
  }

  $basename = [System.IO.Path]::GetFileName($normalized).ToLowerInvariant()
  if ($basename -and $RootIndex.basename.ContainsKey($basename)) {
    $matches = @($RootIndex.basename[$basename])
    if ($matches.Count -eq 1) { return @{ kind = 'resolved'; value = $Value; target = $matches[0] } }
    return @{ kind = 'ambiguous'; value = $Value }
  }

  # Fallback 1: leading-slash value whose first segment duplicates the root's own basename.
  if ($normalized.StartsWith('/')) {
    $rootBasename = Split-Path -Leaf $RootPath
    $stripped = $normalized.TrimStart('/')
    $firstSegment = ($stripped -split '/')[0]
    if ($firstSegment -and $firstSegment.Equals($rootBasename, [System.StringComparison]::OrdinalIgnoreCase)) {
      $retry = ($stripped -split '/', 2)
      $retryCandidate = if ($retry.Count -gt 1) { $retry[1] } else { '' }
      $retryCandidate = $retryCandidate.TrimEnd('/')
      if ($RootIndex.relative.Contains($retryCandidate)) { return @{ kind = 'resolved'; value = $Value; target = $retryCandidate } }
    }
  }

  # Fallback 2: unique hit in another registered root.
  if ($OtherRoots -and $OtherRoots.Count) {
    $cross = Resolve-CrossRootLink -Value $Value -OtherRoots $OtherRoots
    if ($cross.kind -ne 'broken') { return $cross }
  }

  return @{ kind = 'broken'; value = $Value }
}

function Add-LinkIssue {
  param(
    [System.Collections.Generic.List[string]]$Issues,
    [string]$Key,
    [string]$Value,
    [string]$FileRelDir,
    [string]$RootPath,
    $RootIndex,
    [string]$RootName,
    [System.Collections.Generic.List[object]]$OtherRoots
  )

  if (-not $Value) { return $null }
  $resolution = Resolve-NodeLink -Value $Value -FileRelDir $FileRelDir -RootPath $RootPath -RootIndex $RootIndex -RootName $RootName -OtherRoots $OtherRoots
  if ($resolution.kind -eq 'broken') { $Issues.Add("broken-link:$Value") }
  elseif ($resolution.kind -eq 'ambiguous') { $Issues.Add("ambiguous-link:$Value") }
  elseif ($resolution.kind -eq 'resolved' -and $resolution.resolvedRoot) { $Issues.Add("resolved-cross-root:$Value->$($resolution.resolvedRoot)/$($resolution.target)") }
  return $resolution
}

function Parse-NodeBlock {
  param(
    [string]$Text,
    [string]$RelPath,
    [string]$RootPath,
    $RootIndex,
    [System.Collections.Generic.List[string]]$Issues,
    [string]$RootName,
    [System.Collections.Generic.List[object]]$OtherRoots
  )

  $links = [ordered]@{
    directoryPath = ''
    nextSequentialFile = ''
    relatedWorkflows = @()
    aiExecutionPrompts = @()
  }
  # Raw links preserve the author-facing contract. Resolved links preserve their canonical,
  # root-relative coordinates so generated bridges do not reinterpret a source-relative value
  # from the maps/ directory and accidentally escape the repository.
  $resolvedLinks = [ordered]@{
    directoryPath = ''
    nextSequentialFile = ''
    relatedWorkflows = @()
    aiExecutionPrompts = @()
  }

  $strict = Get-StrictNodeVerdict $Text
  $lines = $Text -split "`n"
  $inFence = $false
  $marker = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($lines[$i] -match '^\s*```') {
      $inFence = -not $inFence
      continue
    }
    if ($inFence) { continue }
    if ($lines[$i] -match '^\s*(?:#{2,3}\s*)?(?:[^\w\r\n]+\s*)?(?:\*\*)?\s*SYSTEM NODE LINKS\s*(?:\*\*)?\s*:?\s*$') {
      $marker = $i
    }
  }

  if ($marker -lt 0) {
    return [ordered]@{ nodeBlock = 'missing'; links = $links; resolvedLinks = $resolvedLinks }
  }

  $fields = @{}
  for ($i = $marker + 1; $i -lt $lines.Count; $i++) {
    $line = $lines[$i]
    if ($line -match '^\s*#{1,6}\s+' -and $line -notmatch 'SYSTEM NODE LINKS') { break }
    $match = [regex]::Match(
      $line,
      '^\s*(?:>\s*)?(?:[-*]\s*)?(?:\*\*)?\s*(Directory Path|Directory|Dir Path|Path|Next Sequential File|Next File|Next Sequential|Related Workflows|Related Workflow|Related Files|Related|AI Execution Prompts|AI Prompts|AI Execution Prompt|Execution Prompts)\s*(?:\*\*)?\s*:\s*(.*?)\s*$',
      'IgnoreCase'
    )
    if (-not $match.Success) { continue }
    $key = $match.Groups[1].Value.ToLowerInvariant()
    $canonical = switch -Regex ($key) {
      '^(directory path|directory|dir path|path)$' { 'directoryPath'; break }
      '^(next sequential file|next file|next sequential)$' { 'nextSequentialFile'; break }
      '^(related workflows|related workflow|related files|related)$' { 'relatedWorkflows'; break }
      default { 'aiExecutionPrompts' }
    }
    if (-not $fields.ContainsKey($canonical)) { $fields[$canonical] = $match.Groups[2].Value.Trim() }
  }

  if ($fields.ContainsKey('directoryPath')) {
    $values = @(Select-PathLikeValues -Values @(Split-LinkValues $fields.directoryPath) -Issues $Issues -Key 'Directory Path')
    $links.directoryPath = if ($values.Count) { $values[0] } else { '' }
  }
  if ($fields.ContainsKey('nextSequentialFile')) {
    $values = @(Select-PathLikeValues -Values @(Split-LinkValues $fields.nextSequentialFile) -Issues $Issues -Key 'Next Sequential File')
    $links.nextSequentialFile = if ($values.Count) { $values[0] } else { '' }
  }
  if ($fields.ContainsKey('relatedWorkflows')) { $links.relatedWorkflows = @(Select-PathLikeValues -Values @(Split-LinkValues $fields.relatedWorkflows) -Issues $Issues -Key 'Related Workflows') }
  if ($fields.ContainsKey('aiExecutionPrompts')) { $links.aiExecutionPrompts = @(Select-PathLikeValues -Values @(Split-LinkValues $fields.aiExecutionPrompts) -Issues $Issues -Key 'AI Execution Prompts') }

  $relDir = Convert-ToForwardSlash ([System.IO.Path]::GetDirectoryName($RelPath))
  if ($relDir -eq '.') { $relDir = '' }
  $directoryResolution = Add-LinkIssue $Issues 'Directory Path' $links.directoryPath $relDir $RootPath $RootIndex $RootName $OtherRoots
  if ($directoryResolution -and $directoryResolution.kind -eq 'resolved' -and -not $directoryResolution.resolvedRoot) {
    $resolvedLinks.directoryPath = $directoryResolution.target
  }
  $nextResolution = Add-LinkIssue $Issues 'Next Sequential File' $links.nextSequentialFile $relDir $RootPath $RootIndex $RootName $OtherRoots
  if ($nextResolution -and $nextResolution.kind -eq 'resolved' -and -not $nextResolution.resolvedRoot) {
    $resolvedLinks.nextSequentialFile = $nextResolution.target
  }
  $resolvedRelated = [System.Collections.Generic.List[string]]::new()
  foreach ($value in $links.relatedWorkflows) {
    $resolution = Add-LinkIssue $Issues 'Related Workflows' $value $relDir $RootPath $RootIndex $RootName $OtherRoots
    if ($resolution -and $resolution.kind -eq 'resolved' -and -not $resolution.resolvedRoot) { $resolvedRelated.Add($resolution.target) }
    else { $resolvedRelated.Add($value) }
  }
  $resolvedLinks.relatedWorkflows = @($resolvedRelated)
  $resolvedPrompts = [System.Collections.Generic.List[string]]::new()
  foreach ($value in $links.aiExecutionPrompts) {
    $resolution = Add-LinkIssue $Issues 'AI Execution Prompts' $value $relDir $RootPath $RootIndex $RootName $OtherRoots
    if ($resolution -and $resolution.kind -eq 'resolved' -and -not $resolution.resolvedRoot) { $resolvedPrompts.Add($resolution.target) }
    else { $resolvedPrompts.Add($value) }
  }
  $resolvedLinks.aiExecutionPrompts = @($resolvedPrompts)

  $required = @('directoryPath', 'nextSequentialFile', 'relatedWorkflows', 'aiExecutionPrompts')
  $present = 0
  foreach ($key in $required) { if ($fields.ContainsKey($key)) { $present++ } }
  $status = if ($strict) { 'strict' } elseif ($present -eq 4) { 'legacy' } elseif ($present -gt 0) { 'partial' } else { 'missing' }
  return [ordered]@{ nodeBlock = $status; links = $links; resolvedLinks = $resolvedLinks }
}

function Parse-NodeFile {
  param(
    [string]$FilePath,
    [string]$RootName,
    [string]$RootPath,
    [Nullable[bool]]$Tracked,
    $RootIndex,
    [System.Collections.Generic.List[object]]$OtherRoots
  )

  $rel = Get-RelativePath $RootPath $FilePath
  $issues = [System.Collections.Generic.List[string]]::new()
  $mtime = (Get-Item -LiteralPath $FilePath).LastWriteTimeUtc.ToString('o')
  try {
    $raw = [System.IO.File]::ReadAllText($FilePath)
    $text = ($raw.TrimStart([char]0xFEFF) -replace "`r`n", "`n" -replace "`r", "`n")
  } catch {
    $issues.Add('unreadable')
    return [ordered]@{
      root = $RootName
      relPath = $rel
      title = [System.IO.Path]::GetFileNameWithoutExtension($FilePath)
      type = ''
      context = ''
      envelopeTop = 'missing'
      nodeBlock = 'missing'
      links = [ordered]@{ directoryPath = ''; nextSequentialFile = ''; relatedWorkflows = @(); aiExecutionPrompts = @() }
      resolvedLinks = [ordered]@{ directoryPath = ''; nextSequentialFile = ''; relatedWorkflows = @(); aiExecutionPrompts = @() }
      issues = @($issues)
      waivedIssues = @()
      tracked = $Tracked
      mtimeUtc = $mtime
    }
  }

  $envelope = Parse-Envelope -Text $text -FallbackTitle ([System.IO.Path]::GetFileNameWithoutExtension($FilePath))
  $node = Parse-NodeBlock -Text $text -RelPath $rel -RootPath $RootPath -RootIndex $RootIndex -Issues $issues -RootName $RootName -OtherRoots $OtherRoots
  return [ordered]@{
    root = $RootName
    relPath = $rel
    title = $envelope.title
    type = $envelope.type
    context = $envelope.context
    envelopeTop = $envelope.envelopeTop
    nodeBlock = $node.nodeBlock
    links = $node.links
    resolvedLinks = $node.resolvedLinks
    issues = @($issues)
    waivedIssues = @()
    tracked = $Tracked
    mtimeUtc = $mtime
  }
}

function Get-NodeData {
  $registry = Get-Content -LiteralPath $RegistryPath -Raw | ConvertFrom-Json
  $allFiles = [System.Collections.Generic.List[object]]::new()
  $rootSummaries = [System.Collections.Generic.List[object]]::new()

  $rootIndexes = @{}
  foreach ($rootRecord in $registry) {
    $rootPath = Resolve-RootPath $rootRecord
    $rootIndexes[$rootRecord.name] = Build-RootIndex $rootPath $rootRecord
  }

  foreach ($rootRecord in $registry) {
    $rootPath = Resolve-RootPath $rootRecord
    $trackGit = Test-TrackGit $rootRecord
    $trackedSet = if ($trackGit) { Get-GitTrackedSet $rootPath } else { $null }
    $rootIndex = $rootIndexes[$rootRecord.name]
    $otherRoots = [System.Collections.Generic.List[object]]::new()
    foreach ($other in $registry) {
      if ($other.name -eq $rootRecord.name) { continue }
      $otherRoots.Add(@{ Name = $other.name; Index = $rootIndexes[$other.name] })
    }
    $rootFiles = @(Get-RootFiles $rootRecord)
    # For git-tracked roots, drop untracked candidates git already ignores (.gitignore or
    # .git/info/exclude — e.g. AI-harness worktrees). Git's ignore data is the one source of
    # junk-truth; a second scanner-side exclude list drifts as tools invent new junk dirs.
    # ls-files (not check-ignore --stdin): PowerShell pipes CRLF into native stdin, which git
    # reads as part of the filename and then C-quotes the output — nothing matches.
    if ($trackedSet -and $rootFiles.Count) {
      $ignoredOut = @(& git -C $rootPath -c core.quotepath=false ls-files --others --ignored --exclude-standard -- '*.md' 2>$null)
      if ($ignoredOut.Count) {
        $ignoredSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($p in $ignoredOut) { if ($p) { [void]$ignoredSet.Add((Convert-ToForwardSlash $p)) } }
        $kept = [System.Collections.Generic.List[object]]::new()
        foreach ($rf in $rootFiles) {
          if (-not $ignoredSet.Contains((Get-RelativePath $rootPath $rf.FullName))) { $kept.Add($rf) }
        }
        $rootFiles = @($kept)
      }
    }
    $records = [System.Collections.Generic.List[object]]::new()
    foreach ($file in $rootFiles) {
      $rel = Get-RelativePath $rootPath $file.FullName
      $tracked = if ($trackGit) { $trackedSet.Contains($rel) } else { $null }
      $record = Parse-NodeFile -FilePath $file.FullName -RootName $rootRecord.name -RootPath $rootPath -Tracked $tracked -RootIndex $rootIndex -OtherRoots $otherRoots
      $waivedIssueSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
      foreach ($entry in @($rootRecord.waivedIssues)) { [void]$waivedIssueSet.Add([string]$entry) }
      $activeIssues = [System.Collections.Generic.List[string]]::new()
      $waivedIssues = [System.Collections.Generic.List[string]]::new()
      foreach ($issue in @($record.issues)) {
        if ($waivedIssueSet.Contains("$rel|$issue")) { $waivedIssues.Add($issue) }
        else { $activeIssues.Add($issue) }
      }
      $record.issues = @($activeIssues)
      $record.waivedIssues = @($waivedIssues)
      $records.Add($record)
    }

    $strictBoth = 0; $legacy = 0; $partial = 0; $missing = 0; $trackedCount = 0; $untrackedCount = 0
    foreach ($record in $records) {
      if ($record.tracked -eq $true) { $trackedCount++ }
      elseif ($record.tracked -eq $false) { $untrackedCount++ }

      if ($record.envelopeTop -eq 'strict' -and $record.nodeBlock -eq 'strict') { $strictBoth++ }
      elseif ($record.envelopeTop -eq 'partial' -or $record.nodeBlock -eq 'partial') { $partial++ }
      elseif ($record.envelopeTop -eq 'missing' -or $record.nodeBlock -eq 'missing') { $missing++ }
      else { $legacy++ }
    }

    $rootSummaries.Add([ordered]@{
      name = $rootRecord.name
      path = $rootRecord.path
      counts = [ordered]@{
        total = $records.Count
        strictBoth = $strictBoth
        legacy = $legacy
        partial = $partial
        missing = $missing
        tracked = if ($trackGit) { $trackedCount } else { $null }
        untracked = if ($trackGit) { $untrackedCount } else { $null }
      }
      # First-level dirs that contributed entries; the drift check compares this against the
      # previous map so a brand-new dir prints loudly at the regen it appears.
      topDirs = @($records | ForEach-Object { if ($_.relPath -match '/') { ($_.relPath -split '/')[0] } else { '(root)' } } | Sort-Object -Unique)
    })
    foreach ($record in $records) { $allFiles.Add($record) }
  }

  $sorted = @($allFiles | Sort-Object @{ Expression = 'root'; Ascending = $true }, @{ Expression = 'relPath'; Ascending = $true })
  return [ordered]@{
    generatedUtc = (Get-Date).ToUniversalTime().ToString('o')
    roots = @($rootSummaries)
    files = $sorted
  }
}

function Escape-MarkdownCell([string]$Value) {
  if ($null -eq $Value -or $Value -eq '') { return '' }
  return (($Value -replace '\|', '\|') -replace "`r?`n", ' ')
}

function New-RelativeMarkdownLink([string]$RootName, [string]$RelPath) {
  $safe = Convert-ToForwardSlash $RelPath
  if ($RootName -eq 'mdr-kernel') { return "[$safe](../$safe)" }
  return ('`' + $safe + '`')
}

function Convert-ToMarkdownHref([string]$RelPath) {
  $safe = Convert-ToForwardSlash $RelPath
  $segments = $safe -split '/'
  $encoded = foreach ($segment in $segments) {
    [System.Uri]::EscapeDataString($segment)
  }
  return ($encoded -join '/')
}

function New-ObsidianFileLink([string]$RelPath, [string]$Label) {
  $href = '../' + (Convert-ToMarkdownHref $RelPath)
  $text = if ($Label) { $Label } else { $RelPath }
  $text = $text -replace '\]', '\]'
  return "[$text]($href)"
}

function New-ObsidianNodeLinksMarkdown($Data) {
  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add('Title: Obsidian Node Links')
  $lines.Add('Type: Generated Obsidian Graph Bridge / Do Not Hand-Edit')
  $lines.Add('Context: Generated Markdown-link bridge that lets Markdown tools see mdr-kernel node-map relationships.')
  $lines.Add('')
  $lines.Add('# Obsidian Node Links')
  $lines.Add('')
  $lines.Add('Generated from `maps/node-map.json` by `bin/update-node-map.ps1`. Do not hand-edit.')
  $lines.Add('')
  $lines.Add('This file gives Markdown graph tools real links for the node map. The source files remain canonical; this generated bridge exists so graph views, backlinks, and outgoing links can see the retrieval structure.')
  $lines.Add('')
  $lines.Add("generatedUtc: $($Data.generatedUtc)")
  $lines.Add('')

  $localFiles = @($Data.files | Sort-Object root, relPath)
  foreach ($file in $localFiles) {
    $lines.Add("## $(New-ObsidianFileLink $file.relPath $file.relPath)")
    $lines.Add('')
    $lines.Add("- Title: $(Escape-MarkdownCell $file.title)")
    $typeText = Escape-MarkdownCell $file.type
    if (-not $typeText) { $typeText = 'none recorded' }
    $contextText = Escape-MarkdownCell $file.context
    if (-not $contextText) { $contextText = 'none recorded' }
    $lines.Add("- Type: $typeText")
    $lines.Add("- Context: $contextText")

    $next = if ($file.resolvedLinks.nextSequentialFile) { $file.resolvedLinks.nextSequentialFile } else { $file.links.nextSequentialFile }
    if ($next) {
      $lines.Add("- Next Sequential File: $(New-ObsidianFileLink $next $next)")
    } else {
      $lines.Add('- Next Sequential File: none recorded')
    }

    if ($file.resolvedLinks.relatedWorkflows.Count) {
      $related = @($file.resolvedLinks.relatedWorkflows | ForEach-Object { New-ObsidianFileLink $_ $_ })
      $lines.Add("- Related Workflows: $($related -join ', ')")
    } else {
      $lines.Add('- Related Workflows: none recorded')
    }

    if ($file.resolvedLinks.aiExecutionPrompts.Count) {
      $prompts = @($file.resolvedLinks.aiExecutionPrompts | ForEach-Object { New-ObsidianFileLink $_ $_ })
      $lines.Add("- AI Execution Prompts: $($prompts -join ', ')")
    } else {
      $lines.Add('- AI Execution Prompts: none recorded')
    }

    if ($file.issues.Count) {
      $lines.Add("- Node Map Issues: $(Escape-MarkdownCell ($file.issues -join '; '))")
    }
    if ($file.waivedIssues.Count) {
      $lines.Add("- Waived Node Map Issues: $(Escape-MarkdownCell ($file.waivedIssues -join '; '))")
    }
    $lines.Add('')
  }

  $lines.Add('---')
  $lines.Add('')
  $lines.Add('## SYSTEM NODE LINKS')
  $lines.Add('')
  $lines.Add('Directory Path: maps/')
  $lines.Add('Next Sequential File: maps/NODE-MAP.md')
  $lines.Add('Related Workflows: maps/node-map.json, maps/node-roots.json, specs/node-map.md, docs/node-map-usage-guide.md')
  $lines.Add('AI Execution Prompts: bin/update-node-map.ps1')

  return ($lines -join "`n") + "`n"
}

function New-NodeMapMarkdown($Data) {
  $lines = [System.Collections.Generic.List[string]]::new()
  $lines.Add('Title: Node Map')
  $lines.Add('Type: Generated Node Map / Do Not Hand-Edit')
  $lines.Add('Context: Generated dashboard for Markdown retrieval envelopes and SYSTEM NODE LINKS across registered mdr-kernel roots.')
  $lines.Add('')
  $lines.Add('# Node Map')
  $lines.Add('')
  $lines.Add('Generated from `maps/node-roots.json` by `bin/update-node-map.ps1`.')
  $lines.Add('')
  $lines.Add("generatedUtc: $($Data.generatedUtc)")
  $lines.Add('')
  $lines.Add('## Root Counts')
  $lines.Add('')
  $lines.Add('| Root | Total | Strict Both | Legacy | Partial | Missing | Tracked | Untracked |')
  $lines.Add('| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |')
  foreach ($root in $Data.roots) {
    $tracked = if ($null -eq $root.counts.tracked) { '' } else { $root.counts.tracked }
    $untracked = if ($null -eq $root.counts.untracked) { '' } else { $root.counts.untracked }
    $lines.Add("| $($root.name) | $($root.counts.total) | $($root.counts.strictBoth) | $($root.counts.legacy) | $($root.counts.partial) | $($root.counts.missing) | $tracked | $untracked |")
  }

  $lines.Add('')
  $lines.Add('## Indexed Files')
  $lines.Add('')
  $lines.Add('| Root | File | Title | Envelope | Node | Issues |')
  $lines.Add('| --- | --- | --- | --- | --- | --- |')
  foreach ($file in $Data.files) {
    $issueText = if ($file.issues.Count) { ($file.issues -join '; ') } else { '' }
    $lines.Add("| $($file.root) | $(New-RelativeMarkdownLink $file.root $file.relPath) | $(Escape-MarkdownCell $file.title) | $($file.envelopeTop) | $($file.nodeBlock) | $(Escape-MarkdownCell $issueText) |")
  }

  $untracked = @($Data.files | Where-Object { $_.tracked -eq $false })
  if ($untracked.Count) {
    $lines.Add('')
    $lines.Add('## Captures Awaiting Commit')
    $lines.Add('')
    foreach ($file in $untracked) {
      $lines.Add("- $(New-RelativeMarkdownLink $file.root $file.relPath) - $($file.title)")
    }
  }

  $waivedSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
  $registry = Get-Content -LiteralPath $RegistryPath -Raw | ConvertFrom-Json
  foreach ($root in $registry) {
    foreach ($entry in $root.waivedLegacy) { [void]$waivedSet.Add("$($root.name)|$(Convert-ToForwardSlash $entry)") }
  }

  $broken = @($Data.files | Where-Object { $_.issues | Where-Object { $_ -match '^(broken-link|ambiguous-link):' } })
  $legacy = @($Data.files | Where-Object { ($_.envelopeTop -eq 'legacy' -or $_.nodeBlock -eq 'legacy') -and -not $waivedSet.Contains("$($_.root)|$($_.relPath)") })
  $missing = @($Data.files | Where-Object { ($_.envelopeTop -in @('missing', 'partial') -or $_.nodeBlock -in @('missing', 'partial')) -and -not $waivedSet.Contains("$($_.root)|$($_.relPath)") })
  $known = @($Data.files | Where-Object { $waivedSet.Contains("$($_.root)|$($_.relPath)") })
  $waivedIssues = @($Data.files | Where-Object { $_.waivedIssues.Count -gt 0 })

  $lines.Add('')
  $lines.Add('## Issues')
  $lines.Add('')
  $lines.Add('### Broken or Ambiguous Links')
  if ($broken.Count) {
    foreach ($file in $broken) {
      $linkIssues = @($file.issues | Where-Object { $_ -match '^(broken-link|ambiguous-link):' })
      $lines.Add("- $($file.root)/$($file.relPath): $($linkIssues -join '; ')")
    }
  } else { $lines.Add('- None.') }

  $lines.Add('')
  $lines.Add('### Legacy or Off-Shape Nodes')
  if ($legacy.Count) {
    foreach ($file in $legacy) { $lines.Add("- $($file.root)/$($file.relPath) - top $($file.envelopeTop), node $($file.nodeBlock)") }
  } else { $lines.Add('- None outside known waived files.') }

  $lines.Add('')
  $lines.Add('### Missing or Partial Nodes')
  if ($missing.Count) {
    foreach ($file in $missing) { $lines.Add("- $($file.root)/$($file.relPath) - top $($file.envelopeTop), node $($file.nodeBlock)") }
  } else { $lines.Add('- None outside known waived files.') }

  $lines.Add('')
  $lines.Add('### Known Waived Legacy Files')
  if ($known.Count) {
    foreach ($file in $known) { $lines.Add("- $($file.root)/$($file.relPath)") }
  } else { $lines.Add('- None.') }

  $lines.Add('')
  $lines.Add('### Known Waived Link Issues')
  if ($waivedIssues.Count) {
    foreach ($file in $waivedIssues) { $lines.Add("- $($file.root)/$($file.relPath): $($file.waivedIssues -join '; ')") }
  } else { $lines.Add('- None.') }

  $lines.Add('')
  $lines.Add('---')
  $lines.Add('')
  $lines.Add('## SYSTEM NODE LINKS')
  $lines.Add('')
  $lines.Add('Directory Path: maps/')
  $lines.Add('Next Sequential File: maps/node-map.json')
  $lines.Add('Related Workflows: maps/node-roots.json, specs/node-map.md, docs/node-map-usage-guide.md')
  $lines.Add('AI Execution Prompts: bin/update-node-map.ps1')

  return ($lines -join "`n") + "`n"
}

function Get-IssueKeys($Data) {
  $keys = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
  foreach ($file in $Data.files) {
    foreach ($issue in $file.issues) {
      if ($issue -match '^(broken-link|ambiguous-link|unreadable)') {
        [void]$keys.Add("$($file.root)|$($file.relPath)|$issue")
      }
    }
  }
  return $keys
}

if ($Repair) {
  if (-not $Path) { throw "-Repair requires an explicit -Path <file>" }
  $full = Resolve-InputPath $Path
  Assert-UnderBrain $full
  $registry = Get-Content -LiteralPath $RegistryPath -Raw | ConvertFrom-Json
  $root = Get-RegisteredRootForPath -FullPath $full -Registry $registry
  if (-not $root) { throw "Path is not under a registered root: $Path" }
  if ($root.writePolicy -ne 'repair-allowed') {
    throw "Refusing to repair: root '$($root.name)' is index-only, not write-allowed"
  }
  $rootPath = Resolve-RootPath $root
  $rel = Get-RelativePath $rootPath $full
  $waivedSet = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
  foreach ($entry in $root.waivedLegacy) { [void]$waivedSet.Add((Convert-ToForwardSlash $entry)) }
  if ($waivedSet.Contains($rel)) {
    throw "Refusing to repair: $rel is in waivedLegacy (waived files are never auto-repaired)"
  }
  if (-not (Test-Path -LiteralPath $full)) { throw "File not found: $Path" }

  $raw = [System.IO.File]::ReadAllText($full)
  $text = ($raw.TrimStart([char]0xFEFF) -replace "`r`n", "`n" -replace "`r", "`n")
  if (Get-StrictNodeVerdict $text) {
    Write-Host "${rel}: node block already strict, nothing to repair"
    exit 0
  }

  $plan = Get-NodeRepairPlan $text
  if (-not $plan) {
    Write-Host "${rel}: cannot repair mechanically (fields missing, reordered, or no node block) - needs manual fix"
    exit 1
  }

  Write-Host "${rel}: node block legacy -> strict"
  foreach ($action in $plan.actions) { Write-Host "  + $action" }

  if (-not $Apply) {
    Write-Host "(dry run; pass -Apply to write)"
    exit 0
  }

  [System.IO.File]::WriteAllText($full, $plan.newText, [System.Text.UTF8Encoding]::new($false))
  Write-Host "wrote $rel"
  exit 0
}

if ($Path) {
  $full = Resolve-InputPath $Path
  $registry = Get-Content -LiteralPath $RegistryPath -Raw | ConvertFrom-Json
  $root = Get-RegisteredRootForPath -FullPath $full -Registry $registry
  if (-not $root) { throw "Path is not under a registered root: $Path" }
  $rootPath = Resolve-RootPath $root
  $trackGit = Test-TrackGit $root
  $trackedSet = if ($trackGit) { Get-GitTrackedSet $rootPath } else { $null }
  $rel = Get-RelativePath $rootPath $full
  $tracked = if ($trackGit) { $trackedSet.Contains($rel) } else { $null }
  $otherRoots = [System.Collections.Generic.List[object]]::new()
  foreach ($other in $registry) {
    if ($other.name -eq $root.name) { continue }
    $otherRoots.Add(@{ Name = $other.name; Index = (Build-RootIndex (Resolve-RootPath $other) $other) })
  }
  $record = Parse-NodeFile -FilePath $full -RootName $root.name -RootPath $rootPath -Tracked $tracked -RootIndex (Build-RootIndex $rootPath $root) -OtherRoots $otherRoots
  $record | ConvertTo-Json -Depth 8
  exit 0
}

$data = Get-NodeData

# Drift checks against the previously committed map: silent absorption of a new junk dir (or a
# mass delete) becomes a loud [drift] line at the exact regen where it starts. Advisory only.
if (Test-Path -LiteralPath $NodeMapJsonPath) {
  try {
    $prevMap = Get-Content -LiteralPath $NodeMapJsonPath -Raw | ConvertFrom-Json
    foreach ($newRoot in $data.roots) {
      $prevRoot = @($prevMap.roots | Where-Object { $_.name -eq $newRoot.name })[0]
      if (-not $prevRoot) { continue }
      $prevTotal = [int]$prevRoot.counts.total
      $delta = [int]$newRoot.counts.total - $prevTotal
      $jump = [math]::Max(20, [math]::Ceiling($prevTotal * 0.15))
      if ([math]::Abs($delta) -ge $jump) {
        Write-Host ("[drift] root '{0}' file count moved {1} -> {2} ({3}) in one regen - new junk dir or mass delete? inspect before committing" -f $newRoot.name, $prevTotal, $newRoot.counts.total, $(if ($delta -gt 0) { "+$delta" } else { "$delta" }))
      }
      if ($prevRoot.PSObject.Properties['topDirs']) {
        $prevDirs = @($prevRoot.topDirs)
        foreach ($d in @($newRoot.topDirs | Where-Object { $prevDirs -notcontains $_ })) {
          $n = @($data.files | Where-Object { $_.root -eq $newRoot.name } | Where-Object {
            $seg = if ($_.relPath -match '/') { ($_.relPath -split '/')[0] } else { '(root)' }; $seg -eq $d
          }).Count
          Write-Host ("[drift] NEW top-level dir indexed under root '{0}': '{1}' ({2} file(s)) - intended? add to maps/node-roots.json excludes if not" -f $newRoot.name, $d, $n)
        }
      }
    }
  } catch { Write-Host "[drift] previous map unreadable - drift checks skipped this regen" }
}

if ($Check) {
  $json = ($data | ConvertTo-Json -Depth 12) + "`n"
  $markdown = New-NodeMapMarkdown $data
  $obsidianMarkdown = New-ObsidianNodeLinksMarkdown $data
  $currentIssues = Get-IssueKeys $data
  $newIssues = @($currentIssues)
  if (Test-Path -LiteralPath $NodeMapJsonPath) {
    $previous = Get-Content -LiteralPath $NodeMapJsonPath -Raw | ConvertFrom-Json
    $previousIssues = Get-IssueKeys $previous
    $newIssues = @($currentIssues | Where-Object { -not $previousIssues.Contains($_) })
  }
  foreach ($root in $data.roots) {
    Write-Host ("{0}: total={1} strictBoth={2} legacy={3} partial={4} missing={5}" -f $root.name, $root.counts.total, $root.counts.strictBoth, $root.counts.legacy, $root.counts.partial, $root.counts.missing)
  }
  $staleOutputs = [System.Collections.Generic.List[string]]::new()
  if (-not (Test-GeneratedOutputFresh -Path $NodeMapJsonPath -Content $json -IgnoreLinePatterns @('^\s*"generatedUtc"\s*:'))) {
    $staleOutputs.Add('maps/node-map.json')
  }
  if (-not (Test-GeneratedOutputFresh -Path $NodeMapMarkdownPath -Content $markdown -IgnoreLinePatterns @('^generatedUtc:'))) {
    $staleOutputs.Add('maps/NODE-MAP.md')
  }
  if (-not (Test-GeneratedOutputFresh -Path $ObsidianNodeLinksPath -Content $obsidianMarkdown -IgnoreLinePatterns @('^generatedUtc:'))) {
    $staleOutputs.Add('maps/OBSIDIAN-NODE-LINKS.md')
  }
  if ($newIssues.Count) {
    Write-Error ("New node-map issues: " + ($newIssues -join '; '))
    exit 1
  }
  if ($staleOutputs.Count) {
    Write-Error ("Generated node-map output is stale: " + ($staleOutputs -join ', ') + ". Run bin/update-node-map.ps1.")
    exit 1
  }
  Write-Host 'node-map check passed'
  exit 0
}

$json = ($data | ConvertTo-Json -Depth 12) + "`n"
$markdown = New-NodeMapMarkdown $data
$obsidianMarkdown = New-ObsidianNodeLinksMarkdown $data
[void](Write-AtomicIfChanged -TargetPath $NodeMapJsonPath -Content $json -IgnoreLinePatterns @('^\s*"generatedUtc"\s*:'))
[void](Write-AtomicIfChanged -TargetPath $NodeMapMarkdownPath -Content $markdown -IgnoreLinePatterns @('^generatedUtc:'))
[void](Write-AtomicIfChanged -TargetPath $ObsidianNodeLinksPath -Content $obsidianMarkdown -IgnoreLinePatterns @('^generatedUtc:'))
