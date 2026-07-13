Title: Node map
Type: Spec / Retrieval Contract
Context: Defines the generated node-map subsystem that keeps mdr-kernel routing small, inspectable, and accurate.

# Node map

The node map is the retrieval layer for `mdr-kernel`. It turns the Markdown envelope standard into
two generated artifacts:

- `maps/node-map.json` for tools and scripts.
- `maps/NODE-MAP.md` for humans and agents.

Before an agent reads broadly, it inspects a compact map of titles, types, contexts, and node
links, then selects the smallest relevant file set. This reduces prompt load and improves accuracy
because the agent starts from the repo's declared structure — not from filenames, guesswork, or
stale chat memory.

The envelope is the schema. The node map is the index.

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

## Startup contract

Links are resolved from their source file first and then from the registered root. The JSON keeps
both author-facing link text and resolved root coordinates; the Obsidian bridge uses the resolved
coordinates so a source-relative link cannot be reinterpreted from `maps/` and escape the repo.

`close-session.ps1` stages only the Markdown in its owned close set before regeneration. This
ensures new documents enter retrieval truth as tracked files without sweeping another lane's work.

The startup contract requires agents to use `STATE.md` plus `maps/NODE-MAP.md` or
`maps/node-map.json` to select a narrow reading slice before broad search. `brain-doctor` warns
if the generated outputs are not in exact parity with tracked Markdown, and bootstrap regenerates
them when a fresh clone creates its starter memory files.

An agent that does not consult the node map before reading broadly is not compliant with the
startup contract.

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: ../CONTRIBUTING.md
Related Workflows: ../maps/node-roots.json, ../maps/NODE-MAP.md, ../maps/node-map.json, ../bin/update-node-map.ps1
AI Execution Prompts: ../protocol/CONTRACT.md
