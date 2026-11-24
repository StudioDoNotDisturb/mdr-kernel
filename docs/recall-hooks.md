Title: Prompt recall adapters and their verification limits
Type: Guide / Hook Integration
Context: Use when connecting a host's prompt event to the portable recall adapter, verifying its behavior, or falling back to explicit recall before assigned work.

# Prompt recall adapters and their verification limits

`mdr-kernel brain-recall` is the recall authority. `mdr-kernel hook-recall` adapts a host's
`UserPromptSubmit` JSON input to that command. `mdr-kernel prepare-work` is the explicit fallback.
They use the native binary and local files; no inference service or GPU is involved.

## Configuration, adapter tests and real firing are separate

| Evidence | What it establishes | What remains unverified |
|---|---|---|
| A config file names `UserPromptSubmit` and `mdr-kernel hook-recall` | The file contains the intended wiring | Whether the installed host loads it, uses that command, or sends this kind of prompt through it |
| `cargo test --locked --manifest-path native/Cargo.toml --test regressions recall::` passes | The native adapter handles the tested payloads, matching, errors and timeouts | Whether an interactive application fired the hook |
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
mdr-kernel hook-recall
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
            "command": "mdr-kernel hook-recall",
            "timeout": 20
          }
        ]
      }
    ]
  }
}
```

Run the command from the intended memory root. A host that reads
`.codex/hooks.json` and runs commands on the memory root can use the same PATH entry on every platform:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "mdr-kernel hook-recall",
            "timeout": 20
          }
        ]
      }
    ]
  }
}
```

These commands require `mdr-kernel` on PATH and a working directory inside the intended clone. Confirm
the installed host's loader and event behavior before relying on them. If it uses another event or
payload shape, use explicit recall until that adapter has been implemented and tested; do not rename
an event and assume its input and output protocols are identical.

## Repeat suppression with the native engine

With `bin/mdr-kernel` installed, the hook keeps a receipt for each session, so it does not
send a session the same traps twice. This needs a host that puts `session_id` in its hook payload.
A payload without one is never suppressed. Receipts change only what the hook sends:
`mdr-kernel brain-recall` and `mdr-kernel prepare-work` always print their full output.

The hook sends a trap again in three cases:

- the gate or trap changes, including its recurrence count;
- the payload carries a different `checkpoint`;
- the host sends a `SessionStart` payload for that session.

A host that runs `SessionStart` again after it compacts the context therefore gets every reminder
back. To wire that, add the same command under `SessionStart`, next to `UserPromptSubmit`:

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "mdr-kernel hook-recall", "timeout": 20 } ] }
    ]
  }
}
```

A `SessionStart` payload prints nothing. The receipts sit in the OS temp folder under
`mdr-kernel-recall-delivery/`. Each file is named by a hash of the memory root and the session, and
holds only trap fingerprints, never prompt text. If a receipt file can't be trusted (a link,
oversized, unreadable), the hook sends the reminder again; it never hides one. To reset every
session's receipts, delete that folder.

## MCP server for hosts without a shell

`bin/mdr-kernel mcp` is a read-only MCP server built into the binary. It speaks newline-delimited
JSON-RPC 2.0 on stdio and has three tools:

- `recall_gates`: an objective's matched gates;
- `get_gate_traps`: one gate in full;
- `find_history`: evidence pointers from STATE, the decision log and handoffs.

Each tool runs in the binary itself, so there is no separate backend to be down while the server
looks connected. The tools change only when you rebuild. Output is capped at 24,000 characters per call.
A binary built from other `native/` sources than the indexed ones answers every call with an error.

For a host that reads a project `.mcp.json`:

```json
{ "mcpServers": { "mdr-kernel": { "command": "bin/mdr-kernel", "args": ["mcp"] } } }
```

A host listing the server as connected only shows that the process started. Before relying on it,
call `recall_gates` with a real objective and check the gates it returns.

## Verify behavior without claiming a live hook receipt

After bootstrap has created `protocol/recall-gates.json`, run:

```powershell
cargo test --locked --manifest-path native/Cargo.toml --test regressions recall::
'{"prompt":"deploy again"}' | mdr-kernel hook-recall
'{"prompt":"polish typography"}' | mdr-kernel hook-recall
'{"prompt":42}' | mdr-kernel hook-recall
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
| `RECALL_STATUS=MATCHED` / `CLEAR` | `mdr-kernel brain-recall` | 0 | Evaluation completed; `-Brief` prints nothing on `CLEAR` |
| `RECALL_STATUS=BLOCKED reason=<code>` (stderr) | `mdr-kernel brain-recall` | 2 | Not evaluated: bad arguments, oversized objective, invalid gates or rulings, unknown gate or trap, overflow or compact budget |
| `RECALL_GATES_STATUS=VALID gates=<n>` | `mdr-kernel brain-recall -CheckGates` | 0 | Schema valid; says nothing about firing or trigger coverage |
| `RULING_STATUS=FOUND` / `NONE` | `mdr-kernel brain-recall -Ruling` | 0 / 1 | Live ruling lookup result |
| `RECALL_HOOK_STATUS=DEGRADED reason=<code>` | `mdr-kernel hook-recall` | 0 | Recall was not evaluated; run the fallback. Codes: `empty-input`, `payload-too-large`, `invalid-json`, `payload-not-object`, `prompt-not-string`, `missing-prompt`, `prompt-too-large`, `adapter-error` |
| `PREPARE_WORK_STATUS=READY` | `mdr-kernel prepare-work` | 0 | Printed only after a complete recall run |
| `PREPARE_WORK_STATUS=BLOCKED reason=<code>` (stderr) | `mdr-kernel prepare-work` | matcher's code, or 2 | `recall-failed`; no `READY` line |

## Explicit fallback and direct lookups

```powershell
mdr-kernel prepare-work -Objective 'deploy again'
mdr-kernel prepare-work -FromStdin < objective.txt
mdr-kernel brain-recall -GateId RECURRING-WORK
mdr-kernel brain-recall -Ruling D7
```

The fallback prints full recall output and returns the matcher's nonzero exit with
`PREPARE_WORK_STATUS=BLOCKED` if recall fails. `PREPARE_WORK_STATUS=READY` means the command completed; matched gates and required
sources still need reading. The example `D7` must be replaced with a live ruling ID in your own log.

Ruling lookups and any gate with `rulings` citations require `protocol/rulings.json`. Recall checks
its source hashes against the current decision log, ratifications and gate data before showing rules.
Missing, stale, malformed or unknown cited rulings block recall instead of silently disappearing.
Use `mdr-kernel build-rulings -Check` to diagnose freshness, then rebuild the
generated file under the normal coordination contract. Full output shows the complete compiled rule.
Projects with no citations can use generic recall without a ruling index; the shipped example
gates cite none. To start citing, see "Adding rulings to a gate" in the reference below.

Gate schema and authoring rules are in [Action recall gates](../reference/action-recall-gates.md).

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: reference/action-recall-gates.md
Related Workflows: native/src/recall.rs, native/tests/ASSERTIONS.md
AI Execution Prompts: protocol/CONTRACT.md
