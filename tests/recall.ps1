#requires -Version 7.0
<#
.SYNOPSIS
  Hermetic behavior tests for recall, prompt adaptation and the portable fallback.
.DESCRIPTION
  Uses only a unique temporary directory, no Git, external app, network, semantic service or GPU.
  Assertions require the intended status/output, not merely any nonzero process exit.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$failures = 0
$checks = 0
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$fixture = Join-Path $tempRoot ('kernel-recall-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path (Join-Path $fixture 'bin'), (Join-Path $fixture 'protocol')
$seed = [IO.File]::ReadAllText((Join-Path $root 'protocol/recall-gates.example.json'))
$gatePath = Join-Path $fixture 'protocol/recall-gates.json'
$examplePath = Join-Path $fixture 'protocol/recall-gates.example.json'
[IO.File]::WriteAllText($examplePath, $seed)
$matcherPath = Join-Path $fixture 'bin/brain-recall.ps1'
foreach ($name in @('brain-recall.ps1', 'hook-recall.ps1', 'prepare-work.ps1')) {
  Copy-Item -LiteralPath (Join-Path $root "bin/$name") -Destination (Join-Path $fixture "bin/$name")
}
function Expect([string]$Name, [bool]$Condition, [string]$Detail = '') {
  $script:checks++
  if ($Condition) { Write-Host "PASS $Name" }
  else { $script:failures++; Write-Host "FAIL $Name $Detail" }
}
function Run([string]$Script, [string[]]$Arguments = @(), [string]$InputText = '') {
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = Join-Path $PSHOME $(if ($IsWindows) { 'pwsh.exe' } else { 'pwsh' })
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardInput = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  # Hosts and PowerShell 7 pipes send UTF-8; emulate that instead of the Windows OEM default.
  $start.StandardInputEncoding = [Text.UTF8Encoding]::new($false)
  foreach ($arg in (@('-NoProfile', '-NonInteractive', '-File', (Join-Path $fixture "bin/$Script")) + $Arguments)) { $start.ArgumentList.Add($arg) }
  $process = [Diagnostics.Process]::new()
  $process.StartInfo = $start
  try {
    $null = $process.Start()
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $writing = $process.StandardInput.WriteAsync($InputText)
    if (-not $writing.Wait(25000)) { $process.Kill($true); throw 'fixture stdin timed out' }
    $process.StandardInput.Close()
    if (-not $process.WaitForExit(25000)) { $process.Kill($true); throw 'fixture process timed out' }
    return @{ code = $process.ExitCode; output = $stdout.GetAwaiter().GetResult(); error = $stderr.GetAwaiter().GetResult() }
  } finally { $process.Dispose() }
}
function Reset-Gates { [IO.File]::WriteAllText($gatePath, $seed) }
function Save-Gates($Value) { [IO.File]::WriteAllText($gatePath, ($Value | ConvertTo-Json -Depth 15)) }
function Blocked($Result, [string]$Reason) { $Result.code -eq 2 -and $Result.error -match "RECALL_STATUS=BLOCKED reason=$Reason" }
function Hook([string]$Payload) { Run 'hook-recall.ps1' @() $Payload }
function Degraded($Result, [string]$Reason) {
  if ($Result.code -ne 0 -or $Result.error) { return $false }
  try {
    $context = ($Result.output | ConvertFrom-Json).hookSpecificOutput
    return $context.hookEventName -eq 'UserPromptSubmit' -and $context.additionalContext -match "RECALL_HOOK_STATUS=DEGRADED reason=$Reason;" -and $context.additionalContext -match 'prepare-work.ps1'
  } catch { return $false }
}
function Save-Rulings([string]$Class = 'operator-ruled', [string]$Id = 'D7') {
  $sources = @{}
  foreach ($pair in @(@('decisions', 'DECISIONS.md'), @('ratifications', 'protocol/decision-ratifications.jsonl'), @('gates', 'protocol/recall-gates.json'))) {
    $path = Join-Path $fixture $pair[1]
    $source = if (Test-Path -LiteralPath $path) { [IO.File]::ReadAllText($path) } else { '' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $sources[$pair[0]] = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes(($source -replace "`r`n", "`n").TrimEnd()))).Replace('-', '').ToLowerInvariant() }
    finally { $sha.Dispose() }
  }
  $doc = @{ sources = $sources; rulings = @(@{ id = $Id; class = $Class; rule = 'Read every required source. Preserve the complete operator rule, including this final clause.'; rules_out = 'Assuming a brief reminder is permission.' }) }
  [IO.File]::WriteAllText((Join-Path $fixture 'protocol/rulings.json'), ($doc | ConvertTo-Json -Depth 6))
}
try {
  Reset-Gates
  $r = Run 'brain-recall.ps1' @('-CheckGates')
  Expect 'example v1 schema passes' ($r.code -eq 0 -and $r.output -match 'RECALL_GATES_STATUS=VALID gates=3')
  [IO.File]::WriteAllText($gatePath, 'invalid runtime document')
  $r = Run 'brain-recall.ps1' @('-CheckGates', '-GatesPath', $examplePath)
  Expect 'explicit schema path validates example independently without changing runtime data' ($r.code -eq 0 -and $r.output -match 'RECALL_GATES_STATUS=VALID gates=3' -and [IO.File]::ReadAllText($gatePath) -ceq 'invalid runtime document')
  Expect 'schema path cannot redirect normal recall' (Blocked (Run 'brain-recall.ps1' @('-GatesPath', $examplePath, '-Objective', 'deploy')) 'gates-path-requires-check-only')
  Expect 'schema path cannot accompany increment even with CheckGates' (Blocked (Run 'brain-recall.ps1' @('-CheckGates', '-GatesPath', $examplePath, '-Increment', 'STORED-BUT-UNUSED')) 'gates-path-requires-check-only')
  Reset-Gates
  $r = Run 'brain-recall.ps1' @('-Objective', 'polish the typography', '-Brief')
  Expect 'healthy brief CLEAR is empty without error' ($r.code -eq 0 -and -not $r.output -and -not $r.error)
  $r = Run 'brain-recall.ps1' @('-Objective', 'deploy again', '-Json')
  $result = $r.output | ConvertFrom-Json
  Expect 'normal match preserves both gate records' ($r.code -eq 0 -and $result.status -eq 'MATCHED' -and $result.matched_count -eq 2 -and $result.gates[1].id -eq 'EXAMPLE-DOMAIN')
  $r = Run 'brain-recall.ps1' @('-GateId', 'AGENT-EVIDENCE')
  Expect 'direct lookup includes trap and required read' ($r.code -eq 0 -and $r.output -match 'AGENT-FABRICATED-COMPLETION' -and $r.output -match 'Required reads:')
  Expect 'unknown gate fails with the intended diagnostic' (Blocked (Run 'brain-recall.ps1' @('-GateId', 'NOT-HERE')) 'unknown-gate-id')
  foreach ($objective in @('champagne and fortune', 'pre-deploy check', 'post-roll back-check')) {
    $r = Run 'brain-recall.ps1' @('-Objective', $objective, '-Brief')
    Expect "literal boundaries clear: $objective" ($r.code -eq 0 -and -not $r.output)
  }
  $r = Run 'brain-recall.ps1' @('-Objective', '(roll back) now', '-Brief')
  Expect 'phrase with punctuation boundaries matches' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  foreach ($position in @('beginning', 'middle', 'end')) {
    $objective = switch ($position) { beginning { 'deploy ' + ('x' * 100000) }; middle { ('x' * 50000) + ' deploy ' + ('x' * 50000) }; end { ('x' * 100000) + ' deploy' } }
    $r = Run 'brain-recall.ps1' @('-FromStdin', '-Brief') $objective
    Expect "100k stdin trigger at $position survives" ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  }
  $r = Run 'brain-recall.ps1' @('-FromStdin', '-Brief') (('x' * 131065) + ' deploy')
  Expect 'objective exactly at 131072-character boundary passes' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  Expect 'objective over boundary is rejected, not truncated' (Blocked (Run 'brain-recall.ps1' @('-FromStdin', '-Brief') ('x' * 131073)) 'objective-too-large')
  foreach ($bad in @('broken-json', '[]', '[{"version":1,"gates":[]}]', '"scalar"', '{"version":1,"gates":null}', '{"version":2,"gates":[]}')) {
    [IO.File]::WriteAllText($gatePath, $bad)
    Expect "invalid schema rejected: $bad" (Blocked (Run 'brain-recall.ps1' @('-CheckGates')) 'invalid-gate-data')
  }
  foreach ($kind in @('scalar-triggers', 'negative-count', 'string-count', 'fraction-count', 'duplicate-gate', 'duplicate-trap', 'missing-right-move', 'unsafe-id', 'empty-context-side')) {
    $d = $seed | ConvertFrom-Json
    switch ($kind) {
      scalar-triggers { $d.gates[0].triggers = 'again' }
      negative-count { $d.gates[0].times_recurred = -1 }
      string-count { $d.gates[0].times_recurred = '1' }
      fraction-count { $d.gates[0].times_recurred = 1.5 }
      duplicate-gate { $d.gates[1].id = $d.gates[0].id }
      duplicate-trap { $d.gates[1].traps[0].id = $d.gates[0].traps[0].id }
      missing-right-move { $d.gates[0].traps[0].PSObject.Properties.Remove('right_move') }
      unsafe-id { $d.gates[0].id = 'G; command' }
      empty-context-side { $d.gates[0] | Add-Member context_rules @(@{ any = @(); with = @('workflow') }) }
    }
    Save-Gates $d
    Expect "schema blocks $kind" (Blocked (Run 'brain-recall.ps1' @('-Objective', 'ordinary work')) 'invalid-gate-data')
  }
  Save-Gates @{ version = 1; gates = @() }
  $r = Run 'brain-recall.ps1' @('-Objective', 'anything', '-Brief')
  Expect 'deliberate empty registry is healthy CLEAR' ($r.code -eq 0 -and -not $r.output)
  $d = $seed | ConvertFrom-Json
  $d.gates[0].triggers = @(); $d.gates[0].aliases = @()
  $d.gates[0] | Add-Member context_rules @(@{ any = @('remember'); with = @('workflow') })
  Save-Gates $d
  $r = Run 'brain-recall.ps1' @('-Objective', 'remember the workflow', '-Brief')
  Expect 'paired context needs and accepts both sides' ($r.code -eq 0 -and $r.output -match 'RECURRING-WORK')
  $r = Run 'brain-recall.ps1' @('-Objective', 'remember a birthday', '-Brief')
  Expect 'one context side alone stays CLEAR' ($r.code -eq 0 -and -not $r.output)
  $many = @(1..18 | ForEach-Object { @{ id = "GATE-$_"; label = 'Fixture'; times_recurred = $_; triggers = @('signal'); required_reads = @('reference/INDEX.md'); stop_signs = @(); traps = @() } })
  Save-Gates @{ version = 1; gates = $many }
  $r = Run 'brain-recall.ps1' @('-Objective', 'signal', '-Brief')
  $everyId = @($many | Where-Object { $r.output -notmatch ('(?m)^- ' + [regex]::Escape($_.id) + ' ') }).Count -eq 0
  Expect 'compact output preserves every matched gate and full-read path' ($r.code -eq 0 -and $r.output.Trim().Length -le 1400 -and $everyId -and $r.output -match 'NEEDS_READ=' -and $r.output -match 'For EACH gate ID:')
  $huge = @(1..40 | ForEach-Object { @{ id = ('G' + $_ + ('X' * 70)); label = 'Fixture'; times_recurred = 0; triggers = @('signal'); required_reads = @(); stop_signs = @(); traps = @() } })
  Save-Gates @{ version = 1; gates = $huge }
  Expect 'unrepresentable compact gate list fails visibly' (Blocked (Run 'brain-recall.ps1' @('-Objective', 'signal', '-Brief')) 'compact-budget-exceeded')
  Expect 'hook degrades instead of dropping overflow gates' (Degraded (Hook '{"prompt":"signal"}') 'matcher-failed')
  Reset-Gates
  $r = Run 'brain-recall.ps1' @('-Increment', 'STORED-BUT-UNUSED')
  $d = Get-Content -Raw -LiteralPath $gatePath | ConvertFrom-Json
  Expect 'recurrence mutation increments trap and gate exactly once' ($r.code -eq 0 -and $d.gates[0].times_recurred -eq 1 -and $d.gates[0].traps[0].times_recurred -eq 1)
  $d.gates[0].traps[0].times_recurred = [int]::MaxValue
  Save-Gates $d
  $before = [IO.File]::ReadAllText($gatePath)
  Expect 'recurrence overflow fails without changing data' ((Blocked (Run 'brain-recall.ps1' @('-Increment', 'STORED-BUT-UNUSED')) 'recurrence-count-overflow') -and [IO.File]::ReadAllText($gatePath) -ceq $before)
  Reset-Gates
  $r = Hook '{"user_input":"polish typography"}'
  Expect 'hook healthy CLEAR is empty exit zero' ($r.code -eq 0 -and -not $r.output -and -not $r.error)
  $r = Hook '{"user_input":"deploy again"}'
  $context = ($r.output | ConvertFrom-Json).hookSpecificOutput
  Expect 'hook MATCH returns valid typed context with all matching gates' ($r.code -eq 0 -and $context.hookEventName -eq 'UserPromptSubmit' -and $context.additionalContext -match 'EXAMPLE-DOMAIN' -and $context.additionalContext -match 'RECURRING-WORK')
  $r = Hook '{"user_input":"polish typography","prompt":"deploy"}'
  Expect 'nonblank user_input takes precedence over prompt' ($r.code -eq 0 -and -not $r.output -and -not $r.error)
  $r = Hook '{"prompt":"ship\u0000it\u0007 \u001b[0m deploy\u0000"}'
  Expect 'embedded NUL and control characters still match without error' ($r.code -eq 0 -and -not $r.error -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Hook '{"prompt":"\u0000\u0001\u0002"}'
  Expect 'control-only prompt is healthy CLEAR' ($r.code -eq 0 -and -not $r.output -and -not $r.error)
  $r = Hook '{"user_input":null,"prompt":"deploy"}'
  Expect 'null user_input can use string prompt' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Hook '{"prompt":"deploy"}'
  Expect 'prompt field is supported' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  foreach ($case in @(@('', 'empty-input'), @('{"prompt": "PRIVATE-CANARY"', 'invalid-json'), @('["deploy"]', 'payload-not-object'), @('[{"prompt":"deploy"}]', 'payload-not-object'), @('"deploy"', 'payload-not-object'), @('null', 'payload-not-object'), @('{"prompt":42}', 'prompt-not-string'), @('{"user_input":{},"prompt":"deploy"}', 'prompt-not-string'), @('{"prompt":" "}', 'missing-prompt'))) {
    $r = Hook $case[0]
    Expect "hook visibly degrades: $($case[1])" ((Degraded $r $case[1]) -and $r.output -notmatch 'PRIVATE-CANARY')
  }
  $r = Hook (@{ prompt = ('x' * 100000) + ' deploy' } | ConvertTo-Json -Compress)
  Expect 'hook preserves a late trigger in long prompt' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Hook (@{ prompt = 'PRIVATE-CANARY ' + ('x' * 131073) } | ConvertTo-Json -Compress)
  Expect 'hook bounds decoded prompt without echoing it' ((Degraded $r 'prompt-too-large') -and $r.output -notmatch 'PRIVATE-CANARY')
  Expect 'hook bounds raw payload' (Degraded (Hook (' ' * 1048577)) 'payload-too-large')
  [IO.File]::WriteAllText($gatePath, '{"version":1,"gates":null}')
  Expect 'malformed gate schema is visible through adapter' (Degraded (Hook '{"prompt":"ordinary"}') 'matcher-failed')
  $r = Run 'prepare-work.ps1' @('-Objective', 'ordinary')
  Expect 'fallback propagates malformed schema failure without READY' ($r.code -eq 2 -and $r.error -match 'PREPARE_WORK_STATUS=BLOCKED' -and $r.output -notmatch 'PREPARE_WORK_STATUS=READY')
  Reset-Gates
  $r = Run 'prepare-work.ps1' @('-FromStdin') (('x' * 100000) + ' deploy')
  Expect 'fallback supports long stdin and full required reads' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN' -and $r.output -match 'Required reads:' -and $r.output -match 'PREPARE_WORK_STATUS=READY')
  $d = $seed | ConvertFrom-Json
  $d.gates[1] | Add-Member rulings @('D7')
  Save-Gates $d
  Expect 'a missing cited ruling index blocks, never silently drops' (Blocked (Run 'brain-recall.ps1' @('-Objective', 'deploy')) 'invalid-rulings')
  Save-Rulings
  $r = Run 'brain-recall.ps1' @('-Objective', 'deploy')
  Expect 'full recall exposes complete cited ruling' ($r.code -eq 0 -and $r.output -match 'D7 \[operator-ruled\]' -and $r.output -match 'including this final clause\.')
  $r = Run 'brain-recall.ps1' @('-Objective', 'deploy', '-Brief')
  Expect 'compact recall retains cited ruling IDs and explicit expansion requirement' ($r.code -eq 0 -and $r.output -match 'rulings: D7' -and $r.output -match 'NEEDS_READ=' -and $r.output -match '2 reads')
  $r = Run 'brain-recall.ps1' @('-Ruling', '7')
  Expect 'direct ruling ID lookup returns full rule' ($r.code -eq 0 -and $r.output -match 'RULING_STATUS=FOUND count=1' -and $r.output -match 'including this final clause\.')
  $r = Run 'brain-recall.ps1' @('-Ruling', 'required source')
  Expect 'ruling words use literal substring search' ($r.code -eq 0 -and $r.output -match 'D7')
  $r = Run 'brain-recall.ps1' @('-Ruling', '*')
  Expect 'ruling words are not wildcard patterns' ($r.code -eq 1 -and $r.output -match 'RULING_STATUS=NONE')
  Save-Rulings 'proposed'
  Expect 'proposal cannot enter live ruling output' (Blocked (Run 'brain-recall.ps1' @('-Objective', 'deploy')) 'invalid-rulings')
  Save-Rulings 'operator-ruled' 'D8'
  Expect 'unknown cited decision blocks recall' (Blocked (Run 'brain-recall.ps1' @('-Objective', 'deploy')) 'invalid-rulings')
  Save-Rulings
  [IO.File]::WriteAllText((Join-Path $fixture 'DECISIONS.md'), 'changed after compilation')
  Expect 'stale decision source hash blocks recall' (Blocked (Run 'brain-recall.ps1' @('-Objective', 'deploy')) 'invalid-rulings')
  Reset-Gates
  $r = Run 'brain-recall.ps1' @('-Objective', 'deploy', '-Brief')
  Expect 'uncited optional ruling index does not block generic gates' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  # Non-ASCII triggers must survive every input route: argument, matcher stdin, hook JSON and fallback.
  $d = $seed | ConvertFrom-Json
  $d.gates[1].aliases = @($d.gates[1].aliases) + @('überprüfen', '部署')
  Save-Gates $d
  $r = Run 'brain-recall.ps1' @('-Objective', 'bitte überprüfen', '-Brief')
  Expect 'non-ASCII trigger matches from an argument' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Run 'brain-recall.ps1' @('-FromStdin', '-Brief') 'bitte überprüfen'
  Expect 'non-ASCII trigger matches from UTF-8 stdin' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Hook '{"prompt":"请部署"}'
  Expect 'non-ASCII CJK trigger in raw UTF-8 hook JSON stays CLEAR at a word-character boundary' ($r.code -eq 0 -and -not $r.output)
  $r = Hook '{"prompt":"请 部署 一下"}'
  Expect 'non-ASCII CJK trigger in raw UTF-8 hook JSON matches' ($r.code -eq 0 -and -not $r.error -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Hook '{"prompt":"bitte \u00fcberpr\u00fcfen"}'
  Expect 'escaped non-ASCII hook prompt matches' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN')
  $r = Run 'prepare-work.ps1' @('-FromStdin') 'bitte überprüfen'
  Expect 'fallback matches non-ASCII stdin and reports READY last' ($r.code -eq 0 -and $r.output -match 'EXAMPLE-DOMAIN' -and $r.output.TrimEnd() -match 'PREPARE_WORK_STATUS=READY[^\n]*$')
  # BOM and CRLF gate files (common after editing on Windows) validate and match.
  [IO.File]::WriteAllText($gatePath, ($seed -replace "`r?`n", "`r`n"), [Text.UTF8Encoding]::new($true))
  $r = Run 'brain-recall.ps1' @('-CheckGates')
  $r2 = Run 'brain-recall.ps1' @('-Objective', 'deploy', '-Brief')
  Expect 'BOM and CRLF gate file validates and matches' ($r.code -eq 0 -and $r.output -match 'VALID gates=3' -and $r2.code -eq 0 -and $r2.output -match 'EXAMPLE-DOMAIN')
  # Increment round trip keeps documented and adopter fields, key order and array shapes.
  $d = $seed | ConvertFrom-Json
  $d | Add-Member x_adopter ([pscustomobject]@{ a = @{ b = @{ c = @{ d = @{ e = @{ f = @{ g = @{ h = @{ i = @{ j = @{ k = @{ l = @{ m = 'deep' } } } } } } } } } } } } })
  $d.gates[1].traps[0] | Add-Member triggers @('solo')
  Save-Gates $d
  $r = Run 'brain-recall.ps1' @('-Increment', 'EXAMPLE-TRAP')
  $after = Get-Content -Raw -LiteralPath $gatePath | ConvertFrom-Json
  $keys = @($after.PSObject.Properties.Name)
  $check = Run 'brain-recall.ps1' @('-CheckGates')
  $kept = $after.description -eq $d.description -and $after.x_adopter.a.b.c.d.e.f.g.h.i.j.k.l.m -eq 'deep' -and ($keys -join ',') -eq 'version,updated,description,gates,x_adopter' -and $after.updated -match '^\d{4}-\d{2}-\d{2}$'
  $arrays = (Get-Content -Raw -LiteralPath $gatePath) -match '"triggers": \[\s*"solo"\s*\]'
  Expect 'increment round trip keeps fields, order and single-item arrays and stays valid' ($r.code -eq 0 -and $kept -and $arrays -and $after.gates[1].traps[0].times_recurred -eq 1 -and $check.code -eq 0)
  Reset-Gates
  Move-Item -LiteralPath $matcherPath -Destination "$matcherPath.saved"
  Expect 'missing matcher is visible through adapter' (Degraded (Hook '{"prompt":"deploy"}') 'missing-matcher')
  $r = Run 'prepare-work.ps1' @('-Objective', 'deploy')
  Expect 'missing matcher blocks fallback with stable diagnostic' ($r.code -eq 2 -and $r.error -match 'PREPARE_WORK_STATUS=BLOCKED reason=missing-matcher')
  [IO.File]::WriteAllText($matcherPath, 'exit 2')
  Expect 'matcher failure cannot become healthy CLEAR' (Degraded (Hook '{"prompt":"ordinary"}') 'matcher-failed')
  [IO.File]::WriteAllText($matcherPath, '[Console]::Error.WriteLine("warning"); exit 0')
  Expect 'matcher stderr with exit zero is a degradation' (Degraded (Hook '{"prompt":"ordinary"}') 'matcher-failed')
  [IO.File]::WriteAllText($matcherPath, 'Write-Output "unexpected output"')
  Expect 'unexpected matcher stdout fails visibly' (Degraded (Hook '{"prompt":"ordinary"}') 'invalid-matcher-output')
  $pidFile = Join-Path $fixture 'matcher.pid'
  [IO.File]::WriteAllText($matcherPath, ('[IO.File]::WriteAllText(''{0}'', [string]$PID); Start-Sleep -Seconds 60' -f $pidFile))
  $clock = [Diagnostics.Stopwatch]::StartNew()
  $r = Hook '{"prompt":"ordinary"}'
  $elapsed = $clock.Elapsed.TotalSeconds
  $childPid = if (Test-Path -LiteralPath $pidFile) { [int][IO.File]::ReadAllText($pidFile) } else { 0 }
  $alive = $childPid -and (Get-Process -Id $childPid -ErrorAction SilentlyContinue)
  Expect 'hung matcher is killed and degrades within adapter budget' ((Degraded $r 'matcher-timeout') -and $childPid -and -not $alive -and $elapsed -lt 20) "elapsed=$elapsed pid=$childPid alive=$([bool]$alive)"
} finally {
  # Resolve and check the exact target before recursive deletion; never derive a shell command.
  $resolved = [IO.Path]::GetFullPath($fixture)
  if ($resolved.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -and (Split-Path -Leaf $resolved) -like 'kernel-recall-*') {
    Remove-Item -LiteralPath $resolved -Recurse -Force
  } else { throw 'Refused cleanup outside the unique temporary fixture' }
}
if ($failures) { Write-Host "RECALL_TESTS=FAIL checks=$checks failures=$failures"; exit 1 }
Write-Host "RECALL_TESTS=PASS checks=$checks"
