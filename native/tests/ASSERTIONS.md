Title: [Living] Kernel regression assertion map
Type: Test reference
Context: Use when tracing a legacy suite assertion to its native Cargo test.

# Kernel regression assertion map

| Suite | Reported engine count | Mapped Cargo assertions |
| --- | ---: | ---: |
| session-safety | 64 | 62 |
| private-onboarding | 87 | 87 |
| envelope-check | 17 | 17 |
| recall | 86 | 86 |
| decision-authority | 67 | 66 |
| node-map-retrieval | 15 | 13 |
| remote-privacy-unit | 18 | 17 |
| skill-structure | 30 | 30 |

The node-map script has 13 named assertions. The engine-mode rerun emitted 13 assertion lines. The supplied count of 15 did not reproduce. Recall also lists six legacy-only branches excluded from the reported engine count. Each Cargo test names the cases it groups. The map counts the legacy assertions it carries forward; fixture checks and additional native tests are outside those totals.

## session-safety

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| acquire prints a fresh session proof | `acquire` |
| \$action refuses config identity without changing lock | `lock_proof_and_recovery` with Acquire, Heartbeat, Release, and CheckOwner |
| \$action refuses another same-model session | `lock_proof_and_recovery` with Acquire, Heartbeat, Release, and CheckOwner |
| CheckOwner verifies explicit identity and our acquired token | `lock_proof_and_recovery` |
| \$action refuses tokenless legacy ownership | `lock_proof_and_recovery` with Acquire, Heartbeat, Release, and CheckOwner |
| matching token does not override a different recorded host | `lock_proof_and_recovery` |
| live recovery valve alone cannot prove abandonment | `lock_proof_and_recovery` |
| recovery reason alone cannot take a fresh-heartbeat lock without the live valve | `lock_proof_and_recovery` |
| old-but-live same-model holder is not taken over by acquire | `lock_proof_and_recovery` |
| stale heartbeat alone cannot authorize forced release | `lock_proof_and_recovery` |
| explicit independent recovery evidence permits stale recovery | `lock_proof_and_recovery` |
| empty lock never proves ownership | `lock_proof_and_recovery` |
| unreadable lock can be recovered only by explicit evidence | `lock_proof_and_recovery` |
| pre-commit fails closed when a required utility is absent | `hook_fails_closed_without_git_or_policy` |
| pre-commit specifically refuses unavailable PowerShell | Retired. The old hook required a PowerShell interpreter. The release hook requires the binary. |
| pre-commit fails closed when a gate helper is absent | `hook_fails_closed_without_git_or_policy` |
| config-only attribution refuses without modifying the message | `hook_attribution` |
| explicit session identity adds exactly one trailer | `hook_attribution` |
| explicit identity and trailer disagreement refuses | `hook_attribution` |
| explicit message trailer alone remains supported | `hook_attribution` |
| blank, unknown or duplicate attribution refuses | `hook_attribution` with blank, unknown, and duplicate cases |
| BRAIN_MODEL whitespace is trimmed in the trailer | `hook_attribution` |
| BRAIN_MODEL=unknown is refused without modifying the message | `hook_attribution` |
| bootstrap refuses config identity before creating scaffold or lock | `bootstrap_commit_and_close_boundaries` |
| bootstrap preserves and refuses a preexisting foreign index | `bootstrap_commit_and_close_boundaries` |
| scaffold commit holds only scaffold outputs | `bootstrap_commit_and_close_boundaries` |
| scaffold commit leaves a foreign untracked file untracked | `bootstrap_commit_and_close_boundaries` |
| bootstrap keeps a lock its caller already owned | `bootstrap_commit_and_close_boundaries` |
| bootstrap with a leftover token acquires and releases only its own lock | `bootstrap_commit_and_close_boundaries` |
| close fixture has no remote destination | `bootstrap_commit_and_close_boundaries` |
| coordination hook refuses config-only identity even with correct token | `bootstrap_commit_and_close_boundaries` |
| coordination hook refuses missing session proof | `bootstrap_commit_and_close_boundaries` |
| coordination hook refuses a tokenless lock record | `bootstrap_commit_and_close_boundaries` |
| coordination hook accepts only verified session ownership | `bootstrap_commit_and_close_boundaries` |
| coordination hook refuses the right token under a different model | `bootstrap_commit_and_close_boundaries` |
| real commit of a coordination file without the lock is refused | `bootstrap_commit_and_close_boundaries` |
| real ordinary commit needs no lock and gets one trailer | `bootstrap_commit_and_close_boundaries` |
| real commit with neither BRAIN_MODEL nor trailer is refused | `bootstrap_commit_and_close_boundaries` |
| real commit with only an explicit message trailer is accepted | `bootstrap_commit_and_close_boundaries` |
| engine: a new file without a Least-file trailer is refused | `bootstrap_commit_and_close_boundaries` |
| engine: a binary built from other sources refuses to guard the commit | `bootstrap_commit_and_close_boundaries` |
| engine: MDR_ENGINE=legacy runs the shell gates instead | Retired. The release removes the legacy selection. The native stale-binary refusal remains tested. |
| close refuses foreign staging without unstaging or committing it | `bootstrap_commit_and_close_boundaries` |
| foreign-only dirty tree creates no artificial map commit | `bootstrap_commit_and_close_boundaries` |
| close leaves a foreign untracked file unstaged and untracked | `bootstrap_commit_and_close_boundaries` |
| quoted comma-separated Also paths both reach the checkpoint | `bootstrap_commit_and_close_boundaries` |
| unowned inbox and unrelated work remain outside close | `bootstrap_commit_and_close_boundaries` |
| DryRun never repairs a handoff or leaves a lock | `bootstrap_commit_and_close_boundaries` |
| unproven owner cannot repair handoff before close refuses | `bootstrap_commit_and_close_boundaries` |
| test holder releases its own lock by acquired proof | `bootstrap_commit_and_close_boundaries` |
| foreign staging refuses before handoff repair or lock acquire | `bootstrap_commit_and_close_boundaries` |
| handoff close commits exactly the close set and releases the lock | `bootstrap_commit_and_close_boundaries` |
| handoff close leaves foreign untracked work in place | `bootstrap_commit_and_close_boundaries` |

## private-onboarding

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| fresh clone ships phase-zero doors routing to setup-private | `setup_start_and_destination_boundaries` |
| pre-setup startup blocks and routes to setup-private | `setup_start_and_destination_boundaries` |
| missing model id fails with an actionable command | `setup_start_and_destination_boundaries` |
| direct upstream bootstrap fails before memory is created | `setup_start_and_destination_boundaries` |
| refused bootstrap created no STATE.md | `setup_start_and_destination_boundaries` |
| one-command private-remote setup verifies, bootstraps, and pushes | `setup_start_and_destination_boundaries` |
| first setup prints the induction badge and readiness tagline | `setup_start_and_destination_boundaries` |
| setup pushed the initial private checkpoint | `setup_start_and_destination_boundaries` |
| setup regenerated the phase-zero doors into full contract doors | `setup_start_and_destination_boundaries` |
| setup leaves git config brain.model unset (display-only) | `setup_start_and_destination_boundaries` |
| a second clone runs setup once to recreate local approval and hooks | `setup_start_and_destination_boundaries` |
| second-clone setup skips the induction badge (scaffold came from history) | `setup_start_and_destination_boundaries` |
| second clone has local destination approval and hooks | `setup_start_and_destination_boundaries` |
| setup rerun in the same clone is idempotent | `setup_start_and_destination_boundaries` |
| close-session pushes private main without sweeping a stray file | `setup_start_and_destination_boundaries` |
| stray file stayed local and untracked | `setup_start_and_destination_boundaries` |
| shared close sync preserved an unrelated tracked edit without pushing it | `setup_start_and_destination_boundaries` |
| startup stays READY with an untracked stray file present | `setup_start_and_destination_boundaries` |
| close-session pushed the owned STATE update | `setup_start_and_destination_boundaries` |
| explicit origin/main ignores divergent remote.pushDefault | `setup_start_and_destination_boundaries` |
| offline checkpoint refuses a clone without prior destination approval | `offline_checkpoint_and_recovery` |
| offline checkpoint refuses an origin that no longer matches its approval pin | `offline_checkpoint_and_recovery` |
| offline GitHub checkpoint uses no live privacy probe | `offline_checkpoint_and_recovery` |
| offline checkpoint left remote unchanged and marked pending sync | `offline_checkpoint_and_recovery` |
| doctor names exact pending-sync recovery command | `offline_checkpoint_and_recovery` |
| sync-private live-verifies and recovers offline checkpoint | `offline_checkpoint_and_recovery` |
| recovery pushed checkpoint and cleared pending marker | `offline_checkpoint_and_recovery` |
| close-session delegates shared sync and rebases a non-conflicting checkpoint | `concurrent_sync_and_rejected_push` |
| single retry delivered local checkpoint | `concurrent_sync_and_rejected_push` |
| autostash restoration conflict fails before push | `concurrent_sync_and_rejected_push` |
| autostash conflict kept the remote unchanged and retained the lock | `concurrent_sync_and_rejected_push` |
| close accepts multiple -Also paths in comma form | `close_paths_and_approval` |
| comma-form -Also committed both named paths | `close_paths_and_approval` |
| close accepts a pre-staged deletion named in -Also without splitting | `close_paths_and_approval` |
| deletion and owned edits landed in ONE close commit with a clean tree | `close_paths_and_approval` |
| close warns on a no-match -Also path and still commits the matched one | `close_paths_and_approval` |
| write-approval renders argument errors as [FAIL], not raw exceptions | `close_paths_and_approval` |
| write-approval refuses to propose without an explicit session identity | `close_paths_and_approval` |
| write-approval flow closes with file, INDEX row, and auto-ridden audit | `close_paths_and_approval` |
| approved note and its audit entry both reached the private remote | `close_paths_and_approval` |
| approval gate refuses content swapped after approval and passes the approved bytes | `close_paths_and_approval` |
| approval gate treats a move into a gated path as a new gated file | `close_paths_and_approval` |
| close does not count the write-approval audit log as an unfolded inbox note | `close_paths_and_approval` |
| close refuses an INDEX row pointing at a left-behind file | `close_paths_and_approval` |
| startup names the identity divergence and the exact seal command | `identity_and_lock_wait` |
| conflicting retry fails closed after one attempt | `concurrent_sync_and_rejected_push` |
| conflict was aborted, lock retained, and pending sync stayed loud | `concurrent_sync_and_rejected_push` |
| rejected push keeps the lock, the local commit, and loud pending sync | `concurrent_sync_and_rejected_push` |
| kept lock is released by the token close printed | `concurrent_sync_and_rejected_push` |
| approved destination pin refuses a remote swap | `setup_start_and_destination_boundaries` |
| pre-push propagates the remote-swap refusal | `setup_start_and_destination_boundaries` |
| blocked push left swapped remote unchanged | `setup_start_and_destination_boundaries` |
| local-only setup succeeds | `setup_start_and_destination_boundaries` |
| local-only mode refuses redundant OfflineCheckpoint | `setup_start_and_destination_boundaries` |
| acquire refuses a config-fallback identity and creates no lock | `identity_and_lock_wait` |
| close refuses a config-only committing identity | `identity_and_lock_wait` |
| close with explicit -Model completes end to end | `identity_and_lock_wait` |
| -Model close stamped the Brain-Model trailer | `identity_and_lock_wait` |
| explicit -Model acquire records identity and a session token | `identity_and_lock_wait` |
| close refuses to adopt a live same-model lock token | `identity_and_lock_wait` |
| holder with the correct token closes over its own lock | `identity_and_lock_wait` |
| second acquire for the token-param leg holds a sealed lock | `identity_and_lock_wait` |
| holder recovers with the -Token parameter alone (no env var) | `identity_and_lock_wait` |
| drive2 leg holds a sealed lock | `identity_and_lock_wait` |
| live same-model refusal steers to -Token/-Wait with no takeover recipe | `identity_and_lock_wait` |
| -Release -Force refuses a fresh-heartbeat lock and leaves it in place | `identity_and_lock_wait` |
| fresh-lock recovery requires valve plus independent abandonment evidence | `identity_and_lock_wait` |
| stale same-model refusal requires independent evidence | `identity_and_lock_wait` |
| stale lock refuses force without evidence | `identity_and_lock_wait` |
| stale lock recovers with independent evidence and no live valve | `identity_and_lock_wait` |
| -Wait polls, then gives up on budget against a live holder | `identity_and_lock_wait` |
| -Wait acquires with a fresh token once the holder releases | `identity_and_lock_wait` |
| close accepts a UTC-1-dated handoff and says which date it used | `handoff_dates_and_staged_coordination` |
| missing-handoff failure names the accepted window and -SessionDate | `handoff_dates_and_staged_coordination` |
| staged-blob door check catches drift the working-tree check misses | `handoff_dates_and_staged_coordination` |
| pre-commit blocks the partially staged contract/door commit | `handoff_dates_and_staged_coordination` |
| pre-push/remote-identity/secret-audit are on the coordination allowlist | `handoff_dates_and_staged_coordination` |
| committing an enforcement helper without the lock is blocked | `handoff_dates_and_staged_coordination` |
| generic remote pins preserve path case and fold only scheme/host | `remote_privacy::tests::identities_urls_and_pins` |
| approval pin comparison is case-sensitive | `setup_start_and_destination_boundaries` |
| -Also stages a file inside a fully-untracked directory | `close_paths_and_approval` |
| local-only close refuses a non-main branch | `setup_start_and_destination_boundaries` |
| privacy reports MISSING_ORIGIN instead of LOCAL_ONLY when the pin is set | `setup_start_and_destination_boundaries` |
| doctor hard-fails a private-remote memory with no origin | `setup_start_and_destination_boundaries` |
| startup reports BLOCKED while the private remote is missing | `setup_start_and_destination_boundaries` |
| close refuses the degraded memory and records durable pending-sync | `setup_start_and_destination_boundaries` |
| doctor recovers after the origin is restored | `setup_start_and_destination_boundaries` |

## envelope-check

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| compliant header passes | `envelope_check` |
| title at 100 chars passes | `envelope_check` |
| title at 101 chars fails | `envelope_check` |
| context at 300 chars passes | `envelope_check` |
| context at 301 chars fails | `envelope_check` |
| two short sentences fail | `envelope_check` |
| abbreviations and initials do not split a sentence | `envelope_check` |
| decimals do not split a sentence | `envelope_check` |
| missing envelope is not this check's signal | `envelope_check` |
| a wrapped Context is joined and checked | `envelope_check` |
| text after the blank line is body, not Context | `envelope_check` |
| YAML front matter is checked | `envelope_check` |
| compliant YAML front matter passes | `envelope_check` |
| staged: compliant file passes | `envelope_check` |
| staged: overlong header blocks the commit | `envelope_check` |
| count reports tracked files over the limits | `envelope_check` |
| staged: an unstaged fix cannot mask the staged blob | `envelope_check` |

## recall

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| example v1 schema passes | `recall_schema_matching_and_budgets` |
| explicit schema path validates example independently without changing runtime data | `recall_schema_matching_and_budgets` |
| schema path cannot redirect normal recall | `recall_schema_matching_and_budgets` |
| schema path cannot accompany increment even with CheckGates | `recall_schema_matching_and_budgets` |
| healthy brief CLEAR is empty without error | `recall_schema_matching_and_budgets` |
| normal match preserves both gate records | `recall_schema_matching_and_budgets` |
| direct lookup includes trap and required read | `recall_schema_matching_and_budgets` |
| unknown gate fails with the intended diagnostic | `recall_schema_matching_and_budgets` |
| literal boundaries clear: \$objective | `recall_schema_matching_and_budgets` with three objectives |
| phrase with punctuation boundaries matches | `recall_schema_matching_and_budgets` |
| 100k stdin trigger at \$position survives | `recall_schema_matching_and_budgets` with beginning, middle, and end positions |
| objective exactly at 131072-character boundary passes | `recall_schema_matching_and_budgets` |
| objective over boundary is rejected, not truncated | `recall_schema_matching_and_budgets` |
| invalid schema rejected: \$bad | `recall_schema_matching_and_budgets` with six malformed documents |
| schema blocks \$kind | `recall_schema_matching_and_budgets` with nine schema mutations |
| deliberate empty registry is healthy CLEAR | `recall_schema_matching_and_budgets` |
| paired context needs and accepts both sides | `recall_schema_matching_and_budgets` |
| one context side alone stays CLEAR | `recall_schema_matching_and_budgets` |
| compact output preserves every matched gate and full-read path | `recall_schema_matching_and_budgets` |
| unrepresentable compact gate list fails visibly | `recall_schema_matching_and_budgets` |
| hook degrades instead of dropping overflow gates | `recall_schema_matching_and_budgets` |
| recurrence mutation increments trap and gate exactly once | `recall_schema_matching_and_budgets` |
| recurrence overflow fails without changing data | `recall_schema_matching_and_budgets` |
| hook healthy CLEAR is empty exit zero | `recall_hook_fallback_and_rulings` |
| hook MATCH returns valid typed context with all matching gates | `recall_hook_fallback_and_rulings` |
| nonblank user_input takes precedence over prompt | `recall_hook_fallback_and_rulings` |
| embedded NUL and control characters still match without error | `recall_hook_fallback_and_rulings` |
| control-only prompt is healthy CLEAR | `recall_hook_fallback_and_rulings` |
| null user_input can use string prompt | `recall_hook_fallback_and_rulings` |
| prompt field is supported | `recall_hook_fallback_and_rulings` |
| hook visibly degrades: \$(\$case[1]) | `recall_hook_fallback_and_rulings` with nine payloads |
| hook preserves a late trigger in long prompt | `recall_hook_fallback_and_rulings` |
| hook bounds decoded prompt without echoing it | `recall_hook_fallback_and_rulings` |
| hook bounds raw payload | `recall_hook_fallback_and_rulings` |
| malformed gate schema is visible through adapter | `recall_hook_fallback_and_rulings` |
| fallback propagates malformed schema failure without READY | `recall_hook_fallback_and_rulings` |
| fallback supports long stdin and full required reads | `recall_hook_fallback_and_rulings` |
| a missing cited ruling index blocks, never silently drops | `recall_hook_fallback_and_rulings` |
| full recall exposes complete cited ruling | `recall_hook_fallback_and_rulings` |
| compact recall retains cited ruling IDs and explicit expansion requirement | `recall_hook_fallback_and_rulings` |
| direct ruling ID lookup returns full rule | `recall_hook_fallback_and_rulings` |
| ruling words use literal substring search | `recall_hook_fallback_and_rulings` |
| ruling words are not wildcard patterns | `recall_hook_fallback_and_rulings` |
| proposal cannot enter live ruling output | `recall_hook_fallback_and_rulings` |
| unknown cited decision blocks recall | `recall_hook_fallback_and_rulings` |
| stale decision source hash blocks recall | `recall_hook_fallback_and_rulings` |
| uncited optional ruling index does not block generic gates | `recall_hook_fallback_and_rulings` |
| non-ASCII trigger matches from an argument | `recall_hook_fallback_and_rulings` |
| non-ASCII trigger matches from UTF-8 stdin | `recall_hook_fallback_and_rulings` |
| non-ASCII CJK trigger in raw UTF-8 hook JSON stays CLEAR at a word-character boundary | `recall_hook_fallback_and_rulings` |
| non-ASCII CJK trigger in raw UTF-8 hook JSON matches | `recall_hook_fallback_and_rulings` |
| escaped non-ASCII hook prompt matches | `recall_hook_fallback_and_rulings` |
| fallback matches non-ASCII stdin and reports READY last | `recall_hook_fallback_and_rulings` |
| BOM and CRLF gate file validates and matches | `recall_hook_fallback_and_rulings` |
| increment round trip keeps fields, order and single-item arrays and stays valid | `recall_hook_fallback_and_rulings` |
| engine: hook answers from the gates with the matcher script missing | `recall_hook_fallback_and_rulings` |
| engine: fallback is READY with the matcher script missing | `recall_hook_fallback_and_rulings` |
| missing matcher is visible through adapter | Retired. The legacy adapter spawned a script. The native matcher runs in process. |
| missing matcher blocks fallback with stable diagnostic | Retired. The legacy adapter spawned a script. The native matcher runs in process. |
| engine: a failing matcher script does not change the answer | `recall_hook_fallback_and_rulings` |
| matcher failure cannot become healthy CLEAR | Retired. Legacy child exit handling is inapplicable. Native schema and evaluation failures still degrade the hook. |
| engine: a noisy matcher script does not change the answer | `recall_hook_fallback_and_rulings` |
| matcher stderr with exit zero is a degradation | Retired. The in-process matcher has no child stderr. |
| engine: unexpected matcher-script output does not reach the hook | `recall_hook_fallback_and_rulings` |
| unexpected matcher stdout fails visibly | Retired. The in-process matcher has no child stdout. |
| engine: a hanging matcher script is never started and the hook answers at once | `recall_hook_fallback_and_rulings` |
| hung matcher is killed and degrades within adapter budget | Retired. The in-process matcher has no child to terminate. |

## decision-authority

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| empty new log passes | `decision_structure_history_and_ratification` |
| bootstrap scaffold passes | `decision_structure_history_and_ratification` |
| proposal passes | `decision_structure_history_and_ratification` |
| quoted ruling passes | `decision_structure_history_and_ratification` |
| duplicate id refused | `decision_structure_history_and_ratification` |
| gap refused | `decision_structure_history_and_ratification` |
| wrong order refused | `decision_structure_history_and_ratification` |
| order checked across 3 entries | `decision_structure_history_and_ratification` |
| em dash heading passes | `decision_structure_history_and_ratification` |
| curly-quoted operator words pass | `decision_structure_history_and_ratification` |
| curly-quoted operator words use UTF-16 length | `decision_structure_history_and_ratification` |
| unclassified new entry refused | `decision_structure_history_and_ratification` |
| duplicate Authority lines refused | `decision_structure_history_and_ratification` |
| invented ruling without quote refused | `decision_structure_history_and_ratification` |
| work report refused | `decision_structure_history_and_ratification` |
| typed ratification refused | `decision_structure_history_and_ratification` |
| missing decision body refused | `decision_structure_history_and_ratification` |
| proposal cannot supersede a ruling (checker) | `decision_structure_history_and_ratification` |
| proposal cannot retire even a proposal | `decision_structure_history_and_ratification` |
| stamp without a backing field refused | `decision_structure_history_and_ratification` |
| field without a stamp refused | `decision_structure_history_and_ratification` |
| stamp dated differently from its entry refused | `decision_structure_history_and_ratification` |
| repository policy never applies to a fixture path | `decision_structure_history_and_ratification` |
| explicit -AuthorityFrom applies to a fixture path | `decision_structure_history_and_ratification` |
| LF, CRLF and BOM logs hash identically | `decision_structure_history_and_ratification` |
| stamp under the heading is excluded from the hash | `decision_structure_history_and_ratification` |
| first append to the bootstrap scaffold works | `decision_structure_history_and_ratification` |
| footer edits do not move the oldest entry hash | `decision_structure_history_and_ratification` |
| footer without a blank line is still excluded | `decision_structure_history_and_ratification` |
| add-decision preserves CRLF and BOM | `decision_structure_history_and_ratification` |
| proposal cannot supersede ruling (add-decision) | `decision_structure_history_and_ratification` |
| failed append restores exact bytes | `decision_structure_history_and_ratification` |
| first append to empty log works | `decision_structure_history_and_ratification` |
| proposal can supersede proposal | `decision_structure_history_and_ratification` |
| valid ratification accepted | `decision_structure_history_and_ratification` |
| forged ratification hash refused | `decision_structure_history_and_ratification` |
| entry edited after ratification loses it | `decision_structure_history_and_ratification` |
| ratified proposal cannot be superseded by a proposal | `decision_structure_history_and_ratification` |
| ratification survives a later supersession stamp | `decision_structure_history_and_ratification` |
| ratification ledger is append-only | `decision_structure_history_and_ratification` |
| ratify refuses redirected stdin and writes nothing | `decision_structure_history_and_ratification` |
| ratify refuses -NonInteractive and writes nothing | Retired. This switch belonged to the PowerShell host. Native ratification still checks terminal I/O and refuses redirected input. |
| compiler excludes unratified proposal and sorts gate IDs | `decision_rulings_and_index` |
| fresh compiled output passes -Check | `decision_rulings_and_index` |
| stale compiled output fails -Check after a decision change | `decision_rulings_and_index` |
| gate cannot cite proposal | `decision_rulings_and_index` |
| compiler drops superseded/retired and annotates superseded in part | `decision_rulings_and_index` |
| gate citing retired, superseded or unknown ids fails compilation | `decision_rulings_and_index` |
| empty log compiles to total=0 | `decision_rulings_and_index` |
| compiler refuses forged hash | `decision_rulings_and_index` |
| fixture compile ignores the live ledger | `decision_rulings_and_index` |
| legacy operator-ruled without operator words is not compiled | `decision_rulings_and_index` |
| BOM/CRLF log: staged -Check agrees with the working-tree build | `decision_rulings_and_index` |
| silent old-body rewrite caught against HEAD | `decision_rulings_and_index` |
| migration cannot exempt future entries | `decision_rulings_and_index` |
| unknown policy schema refused | `decision_rulings_and_index` |
| explicit legacy boundary preserves history | `decision_rulings_and_index` |
| committed boundary cannot move | `decision_rulings_and_index` |
| committed boundary cannot be deleted | `decision_rulings_and_index` |
| initializer requires the lock | `decision_initializer` |
| empty log starts at D1 with the bootstrap policy form | `decision_initializer` |
| initializer never rewrites a boundary | `decision_initializer` |
| existing history needs -PreserveLegacy | `decision_initializer` |
| uncommitted history cannot be preserved | `decision_initializer` |
| dirty DECISIONS.md refused | `decision_initializer` |
| legacy history preserved from max+1 without ratification | `decision_initializer` |
| canonical log honours the initialized boundary | `decision_initializer` |

## node-map-retrieval

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| close stages new Markdown before node-map generation | `node_map_retrieval` |
| new Markdown is recorded as tracked retrieval truth | `node_map_retrieval` |
| source-relative link stores its canonical root coordinate | `node_map_retrieval` |
| exported bridge links stay inside the repository | `node_map_retrieval` |
| intentional skill-template placeholder is explicitly waived | `node_map_retrieval` |
| instructional node-field placeholders are explicitly waived | `node_map_retrieval` |
| generated maps pass -Check right after close | `node_map_retrieval` |
| doctor detects exact node-map parity drift | `node_map_retrieval` |
| durable write-approval audit log is not an inbox note | `node_map_retrieval` |
| check reports a new issue when the committed map has none | `node_map_retrieval` |
| a new issue that is a substring of the one known issue is still reported | `node_map_retrieval` |
| a tracked non-ASCII file name is tracked and links to it resolve | `node_map_retrieval` |
| a git root with Markdown but nothing tracked generates without crashing | `node_map_retrieval` |

## remote-privacy-unit

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| gh: PRIVATE accepted | `github_visibility_cases` |
| gh: PUBLIC refused | `github_visibility_cases` |
| live recheck refuses a remote flipped from private to public | `github_visibility_cases` |
| gh: INTERNAL refused | `github_visibility_cases` |
| gh: fork refused | `github_visibility_cases` |
| no-gh API 200 public refused | `github_visibility_cases` |
| no-gh API 200 private accepted | `github_visibility_cases` |
| no-gh API fork refused | `github_visibility_cases` |
| no-gh 404 plus authenticated git accepted as nonpublic | `github_visibility_cases` |
| no-gh 404 plus failed git refused | `github_visibility_cases` |
| no-gh 403 never falls through to git proof | `github_visibility_cases` |
| no-gh timeout/unknown failure refused | `github_visibility_cases` |
| HTTPS parsing normalizes identity | `identities_urls_and_pins` |
| SSH scp parsing normalizes identity | `identities_urls_and_pins` |
| SSH URL parsing normalizes identity | `identities_urls_and_pins` |
| embedded credentials refused | `identities_urls_and_pins` |
| upstream template refused before probes | `identities_urls_and_pins` |
| PowerShell compatibility floor is 7.0 | Retired. The binary has no PowerShell runtime requirement. |

## skill-structure

| Legacy assertion | Cargo test or reason retired |
| --- | --- |
| \$Label has no BOM | `skill_structure` for the shipped skill and its template |
| \$Label frontmatter opens at byte one | `skill_structure` for the shipped skill and its template |
| \$Label frontmatter block closes | `skill_structure` for the shipped skill and its template |
| \$Label frontmatter has '\$field' | `skill_structure` for the shipped skill and its template with six fields per file |
| trigger-queries.json exists | `skill_structure` |
| trigger-queries.json has '\$section' | `skill_structure` with three sections |
| trigger-queries.json '\$section' is non-empty | `skill_structure` with both trigger sections |
| trigger-queries.json '\$section' entry has query+why | `skill_structure` for all six trigger entries |

---

## SYSTEM NODE LINKS

Directory Path: native/tests/
Next Sequential File: ../../protocol/CONTRACT.md
Related Workflows: ../../CONTRIBUTING.md
AI Execution Prompts: ../../protocol/CONTRACT.md
