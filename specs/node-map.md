Title: Node map
Type: Spec / Link-Health Contract
Context: Defines the generated node-map subsystem that keeps mdr-kernel link health small, inspectable, and accurate.

# Node map

The node map is the generated link-health layer for `mdr-kernel`. It turns the Markdown envelope
standard into two generated artifacts:

- `maps/node-map.json` for tools and scripts.
- `maps/NODE-MAP.md` for humans and agents.

It records every registered file's title, type, context and node links, and surfaces broken,
ambiguous or off-shape nodes. It does not choose what an agent reads: `STATE.md` plus
`reference/INDEX.md` select the reading slice. The map is evidence that the declared structure is
intact, so an index row or a node link can be trusted without opening the file.

The envelope is the schema. The node map is the link-health index.

## Source files

`maps/node-roots.json` is the registry. A root has:

- `name`: stable root id.
- `path`: absolute path or path relative to the repo root.
- `writePolicy`: `repair-allowed` or `index-only`.
- `trackGit`: whether tracked/untracked counts should be reported.
- `includes` and `excludes`: optional wildcard filters.
- `waivedLegacy`: files that should not be auto-repaired even if they are off-shape.
- `waivedIssues`: exact `<relative-path>|<issue>` entries for intentional placeholders; waived
  findings remain visible in generated evidence but do not count as active retrieval defects.

Public `mdr-kernel` ships with one root: the repo itself.

## Generated files

Run:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
```

The script scans registered Markdown files, parses the top envelope, parses the terminal
`SYSTEM NODE LINKS` block, resolves local and cross-root links, and writes:

- `maps/node-map.json`
- `maps/NODE-MAP.md`
- `maps/OBSIDIAN-NODE-LINKS.md`

The generated files are not hand-edited. Change the source Markdown or `maps/node-roots.json`,
then regenerate.

## Checks and repairs

Check mode fails when any generated output differs from current tracked Markdown (ignoring only
the generation timestamp), or when a new broken, ambiguous, or unreadable issue appears:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1 -Check
```

Path mode prints the parsed node record for one file:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1 -Path README.md
```

Repair mode fixes a narrow, mechanical class of terminal node-block defects: missing separator
before `SYSTEM NODE LINKS` or trailing content after the four canonical fields. It only writes
inside roots marked `repair-allowed`:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1 -Repair -Path README.md
pwsh -NoProfile -File bin/update-node-map.ps1 -Repair -Path README.md -Apply
```

## Startup contract and link resolution

Links are resolved from their source file first and then from the registered root. The JSON keeps
both author-facing link text and resolved root coordinates; the Obsidian bridge uses the resolved
coordinates so a source-relative link cannot be reinterpreted from `maps/` and escape the repo.

`close-session.ps1` stages only its owned close set and then regenerates the map, staging the
generated outputs with it. This keeps new documents in link-health truth as tracked files without
sweeping another session's work.

The startup contract selects the reading slice from `STATE.md` plus `reference/INDEX.md`; the
node map is not the router. `brain-doctor` warns if the generated outputs are not in exact parity
with tracked Markdown, and bootstrap regenerates them when a fresh clone creates its starter
memory files. The generated map files are coordination files, so regenerate and commit them under
your own `STATE.lock`.

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: ../CONTRIBUTING.md
Related Workflows: ../maps/node-roots.json, ../maps/NODE-MAP.md, ../maps/node-map.json, ../bin/update-node-map.ps1
AI Execution Prompts: ../protocol/CONTRACT.md
