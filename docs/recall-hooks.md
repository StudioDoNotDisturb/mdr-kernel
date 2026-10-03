Title: Prompt recall adapters and their verification limits
Type: Guide / Hook Integration
Context: Use when connecting a host's prompt event to the portable recall adapter, verifying its behavior, or falling back to explicit recall before assigned work.

# Prompt recall adapters and their verification limits

`bin/brain-recall.ps1` is the recall authority. `bin/hook-recall.ps1` adapts a host's
`UserPromptSubmit` JSON input to that command. `bin/prepare-work.ps1` is the explicit fallback.
They use PowerShell 7 and local files; no inference service or GPU is involved.

## Configuration, adapter tests and real firing are separate

| Evidence | What it establishes | What remains unverified |
|---|---|---|
| A config file names `UserPromptSubmit` and `hook-recall.ps1` | The file contains the intended wiring | Whether the installed host loads it, uses that command, or sends this kind of prompt through it |
| `pwsh -NoProfile -File tests/recall.ps1` passes | The scripts handle the tested payloads, matching, errors and timeouts | Whether an interactive application fired the hook |
| A normal prompt on a named host/build produces the expected hook context | That surface and input route fired the adapter in that run | Other builds, headless mode, app-generated prompts or a different input route |

Record the host, version, input route and observed result when obtaining a live receipt. An old
receipt is useful evidence, not proof that this session fired. Doctor must keep configuration and
adapter evidence separate from runtime firing. Use the fallback whenever automatic recall is
unverified, unavailable or degraded.

## Minimal host configuration

The kernel provides the adapter, not an automatic installer for every host's hook settings. Merge
the relevant entry into existing settings; preserve other events. The following shape is for a host
that supports `UserPromptSubmit`, command hooks, JSON stdin and `hookSpecificOutput.additionalContext`.
It is a configuration example, not a compatibility claim for an untested host or build.

From the repository root, the command is:

```text
pwsh -NoProfile -File bin/hook-recall.ps1
```

For a host that reads this hook shape from `.claude/settings.json`, merge:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "pwsh -NoProfile -File bin/hook-recall.ps1",
            "timeout": 20
          }
        ]
      }
    ]
  }
}
```

The relative command requires the repository root as its working directory. A host that reads
`.codex/hooks.json` and supports a `commandWindows` override can resolve the root from Git instead:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "root=\"$(git rev-parse --show-toplevel)\" && pwsh -NoProfile -File \"$root/bin/hook-recall.ps1\"",
            "commandWindows": "pwsh -NoProfile -Command \"& (Join-Path ((& git rev-parse --show-toplevel).Trim()) 'bin/hook-recall.ps1')\"",
            "timeout": 20
          }
        ]
      }
    ]
  }
}
```

These commands require `pwsh` on PATH and a working directory inside the intended clone. Confirm
the installed host's loader and event behavior before relying on them. If it uses another event or
payload shape, use explicit recall until that adapter has been implemented and tested; do not rename
an event and assume its input and output protocols are identical.

## Verify behavior without claiming a live hook receipt

After bootstrap has created `protocol/recall-gates.json`, run:

```powershell
pwsh -NoProfile -File tests/recall.ps1
'{"prompt":"deploy again"}' | pwsh -NoProfile -File bin/hook-recall.ps1
'{"prompt":"polish typography"}' | pwsh -NoProfile -File bin/hook-recall.ps1
'{"prompt":42}' | pwsh -NoProfile -File bin/hook-recall.ps1
```

With the example gates, the first payload returns both matching gate IDs; the second is healthy
`CLEAR` and returns no output; the third returns JSON context with `RECALL_HOOK_STATUS=DEGRADED` and
the fallback command. Custom gates can change which ordinary phrases match. The fixture suite uses
its own copies and example data, so it does not alter your live gates or launch any application.

The adapter accepts a JSON object containing a string `user_input` or `prompt`. A nonblank string
`user_input` wins; a missing, null or blank `user_input` falls back to `prompt`; a nonstring value in
the field that is used is rejected. Arrays, scalars, malformed JSON and oversized input visibly
degrade. The limits are 1,048,576 decoded JSON-text characters, 131,072 prompt characters and a
15-second child-process budget; a matcher that exceeds the budget is killed with its process tree.
Complete prompts travel on stdin; they are never truncated to a prefix or copied into another
operating-system argument string. Stdin is decoded as UTF-8 on every platform (the Windows console
default code page is not used), so non-ASCII triggers match; embedded NUL or control characters are
treated as ordinary non-word characters.

Healthy `CLEAR` writes nothing and exits zero. A match adds at most 1,400 characters of context,
retaining every matched gate ID, recurrence count, trap/read counts, cited ruling IDs and a direct
full-read instruction. Extra trap detail uses remaining space. If mandatory information cannot fit,
recall fails visibly instead of dropping later gates. `NEEDS_READ` means the agent still has to read
the full gates and their sources before acting.

Malformed data, a missing matcher, a nonzero matcher exit, any matcher stderr text, timeout or
unexpected matcher output produces a visible degradation message while the adapter exits zero so the
prompt can continue. That continuation does not authorize action without recall. Error messages
avoid echoing raw prompt or exception text.

| Status line | Emitted by | Exit | Meaning |
|---|---|---|---|
| `RECALL_STATUS=MATCHED` / `CLEAR` | `brain-recall.ps1` | 0 | Evaluation completed; `-Brief` prints nothing on `CLEAR` |
| `RECALL_STATUS=BLOCKED reason=<code>` (stderr) | `brain-recall.ps1` | 2 | Not evaluated: bad arguments, oversized objective, invalid gates or rulings, unknown gate or trap, overflow or compact budget |
| `RECALL_GATES_STATUS=VALID gates=<n>` | `brain-recall.ps1 -CheckGates` | 0 | Schema valid; says nothing about firing or trigger coverage |
| `RULING_STATUS=FOUND` / `NONE` | `brain-recall.ps1 -Ruling` | 0 / 1 | Live ruling lookup result |
| `RECALL_HOOK_STATUS=DEGRADED reason=<code>` | `hook-recall.ps1` | 0 | Recall was not evaluated; run the fallback. Codes: `empty-input`, `payload-too-large`, `invalid-json`, `payload-not-object`, `prompt-not-string`, `missing-prompt`, `prompt-too-large`, `missing-matcher`, `matcher-timeout`, `matcher-failed`, `invalid-matcher-output`, `adapter-error` |
| `PREPARE_WORK_STATUS=READY` | `prepare-work.ps1` | 0 | Printed only after a complete recall run |
| `PREPARE_WORK_STATUS=BLOCKED reason=<code>` (stderr) | `prepare-work.ps1` | matcher's code, or 2 | `missing-matcher` or `recall-failed`; no `READY` line |

## Explicit fallback and direct lookups

```powershell
pwsh -NoProfile -File bin/prepare-work.ps1 -Objective 'deploy again'
Get-Content -Raw objective.txt | pwsh -NoProfile -File bin/prepare-work.ps1 -FromStdin
pwsh -NoProfile -File bin/brain-recall.ps1 -GateId RECURRING-WORK
pwsh -NoProfile -File bin/brain-recall.ps1 -Ruling D7
```

The fallback prints full recall output and returns the matcher's nonzero exit with
`PREPARE_WORK_STATUS=BLOCKED` if recall fails. `PREPARE_WORK_STATUS=READY` means the command completed; matched gates and required
sources still need reading. The example `D7` must be replaced with a live ruling ID in your own log.

Ruling lookups and any gate with `rulings` citations require `protocol/rulings.json`. Recall checks
its source hashes against the current decision log, ratifications and gate data before showing rules.
Missing, stale, malformed or unknown cited rulings block recall instead of silently disappearing.
Use `pwsh -NoProfile -File bin/build-rulings.ps1 -Check` to diagnose freshness, then rebuild the
generated file under the normal coordination contract. Full output shows the complete compiled rule.
Projects with no citations can use generic recall without a ruling index; the shipped example
gates cite none. To start citing, see "Adding rulings to a gate" in the reference below.

Gate schema and authoring rules are in [Action recall gates](../reference/action-recall-gates.md).

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: reference/action-recall-gates.md
Related Workflows: bin/brain-recall.ps1, bin/hook-recall.ps1, bin/prepare-work.ps1, tests/recall.ps1
AI Execution Prompts: protocol/CONTRACT.md
