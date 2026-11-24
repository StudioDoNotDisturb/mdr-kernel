#requires -Version 7.0
<#
.SYNOPSIS
  Synthetic, fixture-isolated tests for the decision-authority tools.
.DESCRIPTION
  Copies the tools into two throwaway temp directories (one plain, one git-initialised) and asserts
  the intended refusal text and exit code for each case. Never touches the real DECISIONS.md,
  ratification ledger or STATE.lock of the memory it lives in.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$source = Split-Path -Parent $PSScriptRoot
$engine = $env:MDR_TEST_ENGINE
$tools = 'check-decision-history.ps1','add-decision.ps1','build-rulings.ps1','ratify-decision.ps1','initialize-decision-authority.ps1','state-lock.ps1'
function New-Fixture([string]$tag) {
  $dir = Join-Path ([IO.Path]::GetTempPath()) ("mdr-authority-$tag-" + [guid]::NewGuid().ToString('N'))
  New-Item -ItemType Directory -Path "$dir/bin","$dir/protocol" -Force | Out-Null
  foreach ($n in $tools) { Copy-Item -LiteralPath "$source/bin/$n" -Destination "$dir/bin/$n" }
  if ($engine) {
    Copy-Item -LiteralPath $engine -Destination (Join-Path $dir "bin/mdr-kernel$(if ($IsWindows) { '.exe' })")
    # The native freshness check compares the binary with indexed compiler inputs.
    foreach ($path in (& git -C $source ls-files -- native)) {
      $dest = Join-Path $dir $path
      New-Item -ItemType Directory -Path (Split-Path -Parent $dest) -Force | Out-Null
      Copy-Item -LiteralPath (Join-Path $source $path) -Destination $dest
    }
  }
  return $dir
}
$fixture = New-Fixture 'a'
$utf8 = [Text.UTF8Encoding]::new($false)
$passed = 0
$started = [Diagnostics.Stopwatch]::StartNew()
function Put([string]$path,[string]$text) { [IO.File]::WriteAllText((Join-Path $fixture $path),$text,$utf8) }
function PutBytes([string]$path,[string]$text,[bool]$bom) {
  $b = $utf8.GetBytes($text); if ($bom) { $b = [byte[]](0xEF,0xBB,0xBF) + $b }
  [IO.File]::WriteAllBytes((Join-Path $fixture $path), [byte[]]$b)
}
function Read([string]$path) { [IO.File]::ReadAllText((Join-Path $fixture $path)) }
function Run([string]$script,[string[]]$arguments) {
  $script:output = @(& pwsh -NoProfile -File "$fixture/bin/$script" @arguments 2>&1) -join "`n"
  $script:code = $LASTEXITCODE
}
function Assert([bool]$condition,[string]$name) {
  if (-not $condition) { throw "FAIL $name`n$output" }
  $script:passed++; Write-Host "PASS $name"
}
function Check([string]$text,[bool]$ok,[string]$name,[string]$reason='',[string[]]$extra=@()) {
  Put 'case.md' $text
  Run 'check-decision-history.ps1' (@('-Path',"$fixture/case.md") + $extra)
  Assert (($code -eq 0) -eq $ok -and (-not $reason -or $output -match $reason)) $name
}
function HashOf([string]$file,[string]$id) {
  Run 'check-decision-history.ps1' @('-Path',"$fixture/$file",'-HashOf',$id)
  if ($code -ne 0 -or $output -notmatch 'ENTRY_SHA256=([0-9a-f]{64})') { throw "FAIL hash of $id in $file`n$output" }
  return $Matches[1]
}
function RatLine([string]$id,[string]$sha) { '{"id":"' + $id + '","entry_sha256":"' + $sha + '","ratified_at":"2026-01-05T00:00:00Z","quote":"synthetic test fixture","surface":"terminal"}' }

$empty = "# Decisions`n`n_No decisions yet._`n`n---`n`n## SYSTEM NODE LINKS`nDirectory Path: ./`n"
$one = "## D1 - 2026-01-01 - One`n`n**Authority:** proposed`n`n**Decision:** Keep useful evidence.`n`n"
$two = "## D2 - 2026-01-02 - Two`n`n**Authority:** operator-ruled`n`n**Operator said (chat, 2026-01-02):** `"Keep every verified receipt intact.`"`n`n**Decision:** Preserve the full evidence, including uncertainty.`n`n"
function Ruled([int]$id,[string]$extra='',[string]$stamp='') {
  $s = if ($stamp) { "$stamp`n" } else { '' }
  "## D$id - 2026-01-0$id - Rule $id`n$s`n**Authority:** operator-ruled`n`n**Operator said (chat, 2026-01-0$id):** `"Rule number $id is what I want.`"`n`n**Decision:** Rule $id body text.`n`n$extra"
}
# The exact DECISIONS.md scaffold bootstrap-clone.ps1 writes, so the checker is tested against its consumer.
$bootstrapText = [IO.File]::ReadAllText("$source/bin/bootstrap-clone.ps1")
$scaffold = [regex]::Match($bootstrapText, "(?s)New-IfMissing 'DECISIONS\.md' @'\r?\n(?<t>.*?)\r?\n'@").Groups['t'].Value -replace "`r`n", "`n"
if (-not $scaffold.Contains('_No decisions yet._')) { throw 'Could not extract the bootstrap DECISIONS.md scaffold.' }

# Run as CI does: a caller's session identity or lock token must not leak into fixture locks
# (state-lock seals an inherited BRAIN_LOCK_TOKEN and then prints no new token).
$savedEnvironment = @{}
foreach ($name in 'BRAIN_MODEL','BRAIN_LOCK_TOKEN','BRAIN_OBJECTIVE','MDR_FORCE_LIVE_LOCK') {
  $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
  Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue
}
try {
  # --- structure and authority --------------------------------------------------------------
  Check $empty $true 'empty new log passes'
  Check $scaffold $true 'bootstrap scaffold passes'
  Check $one $true 'proposal passes'
  Check ($two+$one) $true 'quoted ruling passes'
  Check ($one+$one) $false 'duplicate id refused' 'Duplicate decision id D1'
  Check ($two.Replace('D2','D3')+$one) $false 'gap refused' 'Missing decision id D2'
  Check ($one+$two) $false 'wrong order refused' 'not strictly descending: D1 precedes D2'
  Check ((Ruled 3)+$one+$two) $false 'order checked across 3 entries' 'D1 precedes D2'
  Check ($two.Replace(' - 2026-01-02 - ',' — 2026-01-02 — ')+$one) $true 'em dash heading passes'
  Check ($two.Replace('"Keep every verified receipt intact."','“🧾🧾🧾🧾🧾🧾”')+$one) $true 'curly-quoted operator words use UTF-16 length'
  Check ($one.Replace('**Authority:** proposed', '')) $false 'unclassified new entry refused' 'needs exactly one \*\*Authority:\*\* line.*found 0'
  Check ($one.Replace('**Authority:** proposed', "**Authority:** proposed`n`n**Authority:** operator-ruled")) $false 'duplicate Authority lines refused' 'needs exactly one \*\*Authority:\*\* line.*found 2'
  Check ($two.Replace('**Operator said (chat, 2026-01-02):** "Keep every verified receipt intact."','')+$one) $false 'invented ruling without quote refused' 'operator-ruled but has no \*\*Operator said'
  Check ($one.Replace('proposed','record')) $false 'work report refused' 'work report belongs in a handoff'
  Check ($one.Replace('proposed','operator-ratified')) $false 'typed ratification refused' 'never typed'
  Check ($one.Replace('**Decision:**','**Why:**')) $false 'missing decision body refused' 'D1 needs one \*\*Decision:\*\* body'

  # --- supersession as data -------------------------------------------------------------------
  $propSupersedesRule = "## D3 - 2026-01-03 - Three`n`n**Authority:** proposed`n`n**Decision:** Replace it.`n`n**Supersedes:** D2`n`n" + $two.Replace("Two`n", "Two`n_Status: superseded by D3 (2026-01-03)_`n") + $one
  Check $propSupersedesRule $false 'proposal cannot supersede a ruling (checker)' 'D3 is proposed and cannot supersede D2'
  $propRetires = "## D3 - 2026-01-03 - Three`n`n**Authority:** proposed`n`n**Decision:** Drop it.`n`n**Retires:** D1`n`n" + $two + $one.Replace("One`n", "One`n_Status: retired by D3 (2026-01-03)_`n")
  Check $propRetires $false 'proposal cannot retire even a proposal' 'D3 is proposed and cannot retire D1'
  Check ((Ruled 3) + (Ruled 2 '' '_Status: retired by D3 (2026-01-03)_') + $one) $false 'stamp without a backing field refused' 'D3 does not list D2'
  Check ((Ruled 3 "**Retires:** D2`n`n") + (Ruled 2) + $one) $false 'field without a stamp refused' 'D2 has no .*retired by D3'
  Check ((Ruled 3 "**Retires:** D2`n`n") + (Ruled 2 '' '_Status: retired by D3 (2026-01-09)_') + $one) $false 'stamp dated differently from its entry refused' 'dated 2026-01-09, not D3'

  # --- the fixture/policy boundary --------------------------------------------------------------
  Put 'protocol/decision-authority.json' '{"schema_version":1,"authority_from":2}'
  $legacy = $two + "## D1 - 2026-01-01 - Old`n`n**Decision:** An old unclassified entry.`n`n"
  Check $legacy $false 'repository policy never applies to a fixture path' 'D1 needs exactly one \*\*Authority:\*\* line'
  Check $legacy $true 'explicit -AuthorityFrom applies to a fixture path' '' @('-AuthorityFrom','2')
  Remove-Item -LiteralPath "$fixture/protocol/decision-authority.json"

  # --- one hashing rule: EOL, BOM, stamps and footer do not move it ----------------------------
  $base = $two + $one
  PutBytes 'lf.md' $base $false
  PutBytes 'crlf.md' ($base -replace "`n", "`r`n") $false
  PutBytes 'bom.md' ($base -replace "`n", "`r`n") $true
  $h = HashOf 'lf.md' 'D2'
  Assert ($h -eq (HashOf 'crlf.md' 'D2') -and $h -eq (HashOf 'bom.md' 'D2')) 'LF, CRLF and BOM logs hash identically'
  Put 'stamped.md' ((Ruled 3 "**Supersedes in part:** D2`n`n") + $two.Replace("Two`n", "Two`n_Status: superseded in part by D3 (2026-01-03)_`n") + $one)
  Assert ((HashOf 'stamped.md' 'D2') -eq $h) 'stamp under the heading is excluded from the hash'
  Put 'scaffold.md' $scaffold
  Run 'add-decision.ps1' @('-Path',"$fixture/scaffold.md",'-Title','First proposal','-Body',"**Authority:** proposed`n`n**Decision:** Record the observed behavior.")
  Assert ($code -eq 0 -and (Read 'scaffold.md') -match '(?m)^## D1 - ' -and (Read 'scaffold.md') -notmatch '_No decisions yet\._') 'first append to the bootstrap scaffold works'
  $h1 = HashOf 'scaffold.md' 'D1'
  Put 'scaffold2.md' ((Read 'scaffold.md').Replace('Next Sequential File: STATE.md', 'Next Sequential File: README.md'))
  Assert ((HashOf 'scaffold2.md' 'D1') -eq $h1) 'footer edits do not move the oldest entry hash'
  Put 'scaffold3.md' ((Read 'scaffold.md') -replace "(?m)^---\n\n## SYSTEM NODE LINKS", "---`n## SYSTEM NODE LINKS")
  Assert ((HashOf 'scaffold3.md' 'D1') -eq $h1) 'footer without a blank line is still excluded'

  # --- add-decision ------------------------------------------------------------------------------
  PutBytes 'crlfbom.md' ($base -replace "`n", "`r`n") $true
  Run 'add-decision.ps1' @('-Path',"$fixture/crlfbom.md",'-Title','Third','-Body',"**Authority:** proposed`n`n**Decision:** Keep the line endings.")
  $bytes = [IO.File]::ReadAllBytes("$fixture/crlfbom.md")
  $text = $utf8.GetString($bytes, 3, $bytes.Length - 3)
  Assert ($code -eq 0 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF -and $text -notmatch '(?<!\r)\n' -and $text -match '^## D3 - ') 'add-decision preserves CRLF and BOM'
  Put 'case.md' ($two+$one)
  Run 'add-decision.ps1' @('-Path',"$fixture/case.md",'-Title','Bad replacement','-Body',"**Authority:** proposed`n`n**Decision:** Replace the ruling.`n`n**Supersedes:** D2")
  Assert ($code -ne 0 -and $output -match 'D3 is proposed and cannot supersede D2' -and $output -match 'rolled back') 'proposal cannot supersede ruling (add-decision)'
  Assert ((Read 'case.md') -ceq ($two+$one)) 'failed append restores exact bytes'
  Put 'case.md' $empty
  Run 'add-decision.ps1' @('-Path',"$fixture/case.md",'-Title','First proposal','-Body',"**Authority:** proposed`n`n**Decision:** Record the observed behavior.")
  Assert ($code -eq 0 -and (Read 'case.md') -match '## D1') 'first append to empty log works'
  Run 'add-decision.ps1' @('-Path',"$fixture/case.md",'-Title','Second proposal','-Body',"**Authority:** proposed`n`n**Decision:** Refine the observation.`n`n**Supersedes:** D1")
  Assert ($code -eq 0 -and (Read 'case.md') -match '_Status: superseded by D2') 'proposal can supersede proposal'

  # --- ratification records ------------------------------------------------------------------------
  Put 'case.md' ($two+$one)
  Put 'rat.jsonl' (RatLine 'D1' (HashOf 'case.md' 'D1'))
  Run 'check-decision-history.ps1' @('-Path',"$fixture/case.md",'-RatificationsPath',"$fixture/rat.jsonl")
  Assert ($code -eq 0 -and $output -match 'ratified=1') 'valid ratification accepted'
  Put 'forged.jsonl' (RatLine 'D1' ('ab'*32))
  Run 'check-decision-history.ps1' @('-Path',"$fixture/case.md",'-RatificationsPath',"$fixture/forged.jsonl")
  Assert ($code -ne 0 -and $output -match 'D1 no longer matches the hash it was ratified at') 'forged ratification hash refused'
  Put 'case.md' ($two+$one.Replace('useful','edited'))
  Run 'check-decision-history.ps1' @('-Path',"$fixture/case.md",'-RatificationsPath',"$fixture/rat.jsonl")
  Assert ($code -ne 0 -and $output -match 'D1 no longer matches') 'entry edited after ratification loses it'
  $propOverRatified = "## D3 - 2026-01-03 - Three`n`n**Authority:** proposed`n`n**Decision:** Replace it.`n`n**Supersedes:** D1`n`n" + $two + $one.Replace("One`n", "One`n_Status: superseded by D3 (2026-01-03)_`n")
  Check $propOverRatified $false 'ratified proposal cannot be superseded by a proposal' 'D3 is proposed and cannot supersede D1' @('-RatificationsPath',"$fixture/rat.jsonl")
  Check ((Ruled 3 "**Supersedes:** D1`n`n") + $two + $one.Replace("One`n", "One`n_Status: superseded by D3 (2026-01-03)_`n")) $true 'ratification survives a later supersession stamp' 'ratified=1' @('-RatificationsPath',"$fixture/rat.jsonl")
  Put 'case.md' ($two+$one)
  Put 'rat-base.jsonl' ((RatLine 'D1' (HashOf 'case.md' 'D1')) + "`n" + (RatLine 'D2' (HashOf 'case.md' 'D2')))
  Run 'check-decision-history.ps1' @('-Path',"$fixture/case.md",'-RatificationsPath',"$fixture/rat.jsonl",'-RatificationsBaselinePath',"$fixture/rat-base.jsonl")
  Assert ($code -ne 0 -and $output -match 'is append-only: an existing ratification record was changed or removed') 'ratification ledger is append-only'

  # --- ratify-decision refuses non-interactive callers and writes nothing --------------------------
  Put 'DECISIONS.md' ($two+$one)
  $ledger = "$fixture/protocol/decision-ratifications.jsonl"
  $script:output = @('RATIFY D1' | & pwsh -NoProfile -File "$fixture/bin/ratify-decision.ps1" -Id D1 2>&1) -join "`n"; $script:code = $LASTEXITCODE
  Assert ($code -eq 3 -and $output -match 'RATIFICATION_STATUS=REFUSED' -and $output -match 'standard input is redirected' -and -not (Test-Path -LiteralPath $ledger)) 'ratify refuses redirected stdin and writes nothing'
  $script:output = @(& pwsh -NoProfile -NonInteractive -File "$fixture/bin/ratify-decision.ps1" -Id D1 2>&1) -join "`n"; $script:code = $LASTEXITCODE
  Assert ($code -eq 3 -and $output -match 'started with -NonInteractive' -and -not (Test-Path -LiteralPath $ledger)) 'ratify refuses -NonInteractive and writes nothing'

  # --- build-rulings -------------------------------------------------------------------------------
  Put 'protocol/recall-gates.json' '{"gates":[{"id":"A-1","rulings":["D2"]},{"id":"A_1","rulings":["D2"]}]}'
  Run 'build-rulings.ps1' @()
  $compiled = Read 'protocol/rulings.json' | ConvertFrom-Json
  Assert ($code -eq 0 -and $compiled.total -eq 1 -and $compiled.rulings[0].id -eq 'D2' -and
    ($compiled.rulings[0].gates -join ',') -ceq 'A_1,A-1') 'compiler excludes unratified proposal and sorts gate IDs'
  Run 'build-rulings.ps1' @('-Check')
  Assert ($code -eq 0 -and $output -match 'RULINGS_STATUS=OK total=1') 'fresh compiled output passes -Check'
  Put 'DECISIONS.md' ((Ruled 3) + $two + $one)
  Run 'build-rulings.ps1' @('-Check')
  Assert ($code -eq 1 -and $output -match 'RULINGS_STATUS=STALE') 'stale compiled output fails -Check after a decision change'
  Put 'protocol/recall-gates.json' '{"gates":[{"id":"TEST","rulings":["D1"]}]}'
  Run 'build-rulings.ps1' @()
  Assert ($code -ne 0 -and $output -match 'not live rulings.*D1 \(cited by TEST\)') 'gate cannot cite proposal'
  Put 'DECISIONS.md' ((Ruled 4 "**Supersedes in part:** D3`n`n") + (Ruled 3 "**Supersedes:** D2`n`n" '_Status: superseded in part by D4 (2026-01-04)_') + (Ruled 2 "**Retires:** D1`n`n" '_Status: superseded by D3 (2026-01-03)_') + (Ruled 1 '' '_Status: retired by D2 (2026-01-02)_'))
  Put 'protocol/recall-gates.json' '{"gates":[]}'
  Run 'build-rulings.ps1' @()
  $compiled = Read 'protocol/rulings.json' | ConvertFrom-Json
  Assert ($code -eq 0 -and (@($compiled.rulings.id) -join ',') -eq 'D4,D3' -and $compiled.rulings[1].rule -match 'superseded in part') 'compiler drops superseded/retired and annotates superseded in part'
  Put 'protocol/recall-gates.json' '{"gates":[{"id":"OLD","rulings":["D1"]},{"id":"REPLACED","rulings":["D2"]},{"id":"GHOST","rulings":["D9"]},{"id":"LIVE","rulings":["D3"]}]}'
  Run 'build-rulings.ps1' @()
  Assert ($code -eq 1 -and $output -match 'D1 \(cited by OLD\)' -and $output -match 'D2 \(cited by REPLACED\)' -and $output -match 'D9 \(cited by GHOST\)' -and $output -notmatch 'D3 \(cited') 'gate citing retired, superseded or unknown ids fails compilation'
  Put 'protocol/recall-gates.json' '{"gates":[]}'
  Put 'DECISIONS.md' $empty
  Run 'build-rulings.ps1' @()
  Assert ($code -eq 0 -and (Read 'protocol/rulings.json' | ConvertFrom-Json).total -eq 0) 'empty log compiles to total=0'
  Put 'DECISIONS.md' ($two+$one)
  Put 'protocol/decision-ratifications.jsonl' ('{"id":"D1","entry_sha256":"'+('0'*64)+'","ratified_at":"2026-01-02T00:00:00Z","quote":"synthetic test fixture","surface":"terminal"}')
  Run 'build-rulings.ps1' @()
  Assert ($code -ne 0 -and $output -match 'no longer matches') 'compiler refuses forged hash'
  Put 'case.md' ($two+$one)
  Run 'build-rulings.ps1' @('-DecisionsPath',"$fixture/case.md",'-OutPath',"$fixture/case-rulings.json")
  Assert ($code -eq 0 -and (@((Read 'case-rulings.json' | ConvertFrom-Json).rulings.id) -join ',') -eq 'D2') 'fixture compile ignores the live ledger'
  Put 'protocol/decision-ratifications.jsonl' ''
  Put 'protocol/decision-authority.json' '{"schema_version":1,"authority_from":3}'
  Put 'DECISIONS.md' ($two.Replace('**Operator said (chat, 2026-01-02):** "Keep every verified receipt intact."', '') + $one)
  Run 'build-rulings.ps1' @()
  Assert ($code -eq 0 -and (Read 'protocol/rulings.json' | ConvertFrom-Json).total -eq 0) 'legacy operator-ruled without operator words is not compiled'
  Remove-Item -LiteralPath "$fixture/protocol/decision-authority.json"

  # --- index/HEAD history and the migration boundary in a git fixture ------------------------------
  PutBytes 'DECISIONS.md' (($two+$one) -replace "`n", "`r`n") $true
  & git -C $fixture init -q
  & git -C $fixture config core.hooksPath disabled-test-hooks
  & git -C $fixture config core.autocrlf false
  if ($engine) { & git -C $fixture add -- native }
  & git -C $fixture config user.name 'Authority test'
  & git -C $fixture config user.email 'test@example.invalid'
  Run 'build-rulings.ps1' @()
  & git -C $fixture add -- DECISIONS.md protocol
  Run 'build-rulings.ps1' @('-Check','-FromIndex')
  Assert ($code -eq 0 -and $output -match 'RULINGS_STATUS=OK') 'BOM/CRLF log: staged -Check agrees with the working-tree build'
  & git -C $fixture commit -qm baseline
  if ($LASTEXITCODE -ne 0) { throw 'Fixture commit failed' }
  PutBytes 'DECISIONS.md' (($two+$one.Replace('useful','quietly revised')) -replace "`n", "`r`n") $true
  & git -C $fixture add -- DECISIONS.md
  Run 'check-decision-history.ps1' @('-FromIndex')
  Assert ($code -ne 0 -and $output -match 'D1 differs from HEAD .*append-only') 'silent old-body rewrite caught against HEAD'
  & git -C $fixture reset -q -- DECISIONS.md
  PutBytes 'DECISIONS.md' (($two+$one) -replace "`n", "`r`n") $true
  Put 'protocol/decision-authority.json' '{"schema_version":1,"authority_from":4}'
  & git -C $fixture add -- protocol/decision-authority.json
  Run 'check-decision-history.ps1' @('-FromIndex')
  Assert ($code -ne 0 -and $output -match 'only decision entries already committed') 'migration cannot exempt future entries'
  Put 'protocol/decision-authority.json' '{"schema_version":2,"authority_from":3}'
  & git -C $fixture add -- protocol/decision-authority.json
  Run 'check-decision-history.ps1' @('-FromIndex')
  Assert ($code -ne 0 -and $output -match 'Invalid protocol/decision-authority\.json') 'unknown policy schema refused'
  Put 'protocol/decision-authority.json' '{"schema_version":1,"authority_from":3}'
  & git -C $fixture add -- protocol/decision-authority.json
  Run 'check-decision-history.ps1' @('-FromIndex')
  Assert ($code -eq 0) 'explicit legacy boundary preserves history'
  & git -C $fixture commit -qm migration
  Put 'protocol/decision-authority.json' '{"schema_version":1,"authority_from":5}'
  & git -C $fixture add -- protocol/decision-authority.json
  Run 'check-decision-history.ps1' @('-FromIndex')
  Assert ($code -ne 0 -and $output -match 'boundary is immutable') 'committed boundary cannot move'
  & git -C $fixture rm -q --cached -- protocol/decision-authority.json
  Run 'check-decision-history.ps1' @('-FromIndex')
  Assert ($code -ne 0 -and $output -match 'boundary cannot be deleted') 'committed boundary cannot be deleted'

  # --- initialize-decision-authority (own git fixture and a real fixture lock) ----------------------
  $fixture = New-Fixture 'init'
  & git -C $fixture init -q
  & git -C $fixture config core.autocrlf false
  if ($engine) { & git -C $fixture add -- native }
  & git -C $fixture config core.hooksPath disabled-test-hooks
  & git -C $fixture config user.name 'Authority test'
  & git -C $fixture config user.email 'test@example.invalid'
  Put 'DECISIONS.md' $scaffold
  Put 'protocol/recall-gates.json' '{"gates":[]}'
  $policyFile = "$fixture/protocol/decision-authority.json"
  Run 'initialize-decision-authority.ps1' @('-Model','fixture-model','-Token','not-a-token')
  Assert ($code -ne 0 -and $output -match 'Own STATE\.lock before initializing authority' -and -not (Test-Path -LiteralPath $policyFile)) 'initializer requires the lock'
  Run 'state-lock.ps1' @('-Acquire','-Model','fixture-model','-Objective','decision authority test')
  if ($output -notmatch 'session token: ([0-9a-f]{32})') { throw "FAIL fixture lock`n$output" }
  $lockArgs = @('-Model','fixture-model','-Token',$Matches[1])
  Run 'initialize-decision-authority.ps1' $lockArgs
  Assert ($code -eq 0 -and $output -match 'AUTHORITY_INITIALIZED from=D1' -and (Read 'protocol/decision-authority.json').Trim() -ceq '{"schema_version":1,"authority_from":1}') 'empty log starts at D1 with the bootstrap policy form'
  Run 'initialize-decision-authority.ps1' $lockArgs
  Assert ($code -ne 0 -and $output -match 'already exists; it is immutable') 'initializer never rewrites a boundary'
  Remove-Item -LiteralPath $policyFile
  & git -C $fixture add -- protocol/recall-gates.json
  & git -C $fixture commit -qm gates
  Put 'DECISIONS.md' ($two.Replace('**Authority:** operator-ruled', '') + $one.Replace('**Authority:** proposed', ''))
  Run 'initialize-decision-authority.ps1' $lockArgs
  Assert ($code -ne 0 -and $output -match 'explicitly use -PreserveLegacy' -and -not (Test-Path -LiteralPath $policyFile)) 'existing history needs -PreserveLegacy'
  Run 'initialize-decision-authority.ps1' ($lockArgs + '-PreserveLegacy')
  Assert ($code -ne 0 -and $output -match 'Commit existing history unchanged' -and -not (Test-Path -LiteralPath $policyFile)) 'uncommitted history cannot be preserved'
  & git -C $fixture add -- DECISIONS.md protocol/recall-gates.json
  & git -C $fixture commit -qm legacy
  Put 'DECISIONS.md' ((Read 'DECISIONS.md').Replace('useful', 'dirty'))
  Run 'initialize-decision-authority.ps1' ($lockArgs + '-PreserveLegacy')
  Assert ($code -ne 0 -and $output -match 'Commit existing history unchanged' -and -not (Test-Path -LiteralPath $policyFile)) 'dirty DECISIONS.md refused'
  & git -C $fixture checkout -q -- DECISIONS.md
  Run 'initialize-decision-authority.ps1' ($lockArgs + '-PreserveLegacy')
  Assert ($code -eq 0 -and $output -match 'AUTHORITY_INITIALIZED from=D3' -and -not (Test-Path -LiteralPath "$fixture/protocol/decision-ratifications.jsonl") -and (Read 'protocol/rulings.json' | ConvertFrom-Json).total -eq 0) 'legacy history preserved from max+1 without ratification'
  Run 'check-decision-history.ps1' @()
  Assert ($code -eq 0 -and $output -match 'authority_checked=0') 'canonical log honours the initialized boundary'
  Run 'state-lock.ps1' ($lockArgs + '-Release')

  Write-Host "DECISION_TESTS=PASS count=$passed seconds=$([int]$started.Elapsed.TotalSeconds)"
} catch { Write-Host "DECISION_TESTS=FAIL fixture=$fixture"; throw } finally {
  foreach ($name in $savedEnvironment.Keys) {
    if ($null -eq $savedEnvironment[$name]) { Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue }
    else { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
  }
}
# Keep tiny synthetic fixtures for diagnosis; no recursive deletion of a computed path.
