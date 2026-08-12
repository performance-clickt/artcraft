# Local scratchpad — ArtCraft MCP

Linear is the record; this file is the resume map for the next session.

## 2026-08-12 — ⛔ HALT STATE: round 4 mid-flight, halted on John's instruction before integration

1. **Phase:** ⛔ HALT STATE (user said "Halt there"). Round 4: 3 of 4 lanes delivered, HM-921 executor was STILL RUNNING at halt — it posts its own wrap-up + In Review to Linear when it finishes, so check HM-921 in Linear before anything else; it may complete after this checkpoint. If it's still In Progress with no wrap-up and no live processes (`pgrep -fl artcraft`, `pgrep -f vite`), the executor died mid-lane: inspect worktree /Users/johngreenhow/Artcraft/worktrees/hm-921 (branch john/hm-921-frontend-controlbridge-event-hook-reply-wrapper-scene-op) for uncommitted/unpushed work and possible stray dev processes (SIGTERM target/debug/artcraft, kill Vite, free :5173) before re-running.
2. **Delivered, In Review, NOT integrated:** HM-930 @ b151fbf8d2 (PR #7, npx nx; live Vite check deferred to QC); HM-934 @ 75ade16e1c (PR #8, enveloped fallback; deviations accepted: .layer moved into new sealed-router fn for testability, +NotFound/MethodNotAllowed variants, comment fix; pre-existing Allow-header 401 side channel pinned by characterization test → new-issue candidate); HM-922 @ f6e4a4d3bb (PR #9, mcp/artcraft-mcp scaffold, 46/46 tests, stdio round trip; inspector + with-app criteria deferred to QC; new-issue candidate: estimate_cost lacks deny_unknown_fields — same class as r3 external-contract rule).
3. **Worktrees live at halt:** hm-921, hm-922, hm-930, hm-934 (all off main @ 7f71127381). No integration branch created yet.
4. **Successor's next steps (in order):** (a) resolve HM-921 per item 1; (b) create integration worktree artcraft-mcp-r4 off main at an absolute path, merge the 4 lane branches (expected conflicts: spawn_control_server_thread.rs — HM-934 moved the .layer call into control_server/enveloped_fallback.rs seal fn while HM-921 doesn't touch it, so likely only trivial; _docs/dev_setup.md between HM-930 and nothing else this round), pre-checks (cargo check, cargo test -p artcraft --lib, npm run build + npm test in mcp/artcraft-mcp, bash -n script), push; (c) merge gate (orchestrator:merge-gate, applyFixes true, carry all accepted deviations above + both new-issue candidates as KNOWN); (d) reflect; (e) QC with RUST_LOG=debug — must close ALL deferred evidence: HM-930 live Vite/npx check (no global nx on PATH), HM-922 inspector smoke + real logged_in/credits/estimate vs live app, HM-934 live 401/404/405 curls, HM-921 acceptance (scene ops tab open/closed + timeout), r3 probe-14 UI half (dismiss task card, curl id), M3 milestone check; (f) ASK JOHN for merge approval (do not self-land), then land/prune/lessons/status update.
5. **Open for John:** Done verdicts HM-915..920, HM-928 (In Review, PRs merged); HM-929 option (a) ratification; delete superseded Lessons Log duplicates (canonical = lessons-log-4a8de456b76a); merge-approval for r4 when gates are green.
6. **Lesson candidates queued (unapplied):** HM-934's verify-framework-source + prove-leak-ownership + reflect-rechecks-prose; HM-922's status-code-fallback and in-memory-MCP-round-trip lessons; estimate_cost deny_unknown_fields extension of the external-contract rule.

## 2026-08-12 — Round 4 in flight: HM-921/922/930/934 (superseded)

1. **Lanes launched** (claimed + plan comments posted; worktrees off main @ 7f71127381): hm-921 frontend ControlBridge (Opus, HOLDS THE LIVE-APP SLOT — runs dev app with RUST_LOG=debug, curls scene ops tab open/closed); hm-922 MCP TS scaffold (Opus, mcp/artcraft-mcp new top dir, build+unit only, inspector deferred to QC); hm-930 npx-nx script fix (Sonnet, static verify, Vite check deferred — 5173 owned by 921); hm-934 enveloped fallback (Opus, auth-ordering proof via tower oneshot tests, live deferred). Surfaces disjoint. Branch names: 921/922/930 = Linear gitBranchNames, 934 = hm-934-enveloped-fallback.
2. **After lanes:** integration artcraft-mcp-r4 off main → expected conflicts only in build_control_router (// HM-934 line) → gate → reflect → QC (RUST_LOG=debug; close probe-14 UI half; HM-930 Vite check; HM-922 inspector smoke vs live app; M3 milestone check = scene ops open/closed + timeout) → merge approval ask → land → lessons.
3. **Open for John:** Done verdicts HM-915..920/928; HM-929 option (a); delete superseded Lessons Log duplicates.

## 2026-08-12 — Round 3 LANDED; M2 complete; session quiescent (superseded)

1. **Landed:** main @ cbb5ba03d6 (r3: HM-917/918/919/920 + gate fixes; reflect MERGE ×4; QC 20 PASS/0 FAIL; PRs #3–#6 merged). Worktrees + lane/integration branches pruned; live tree clean on main. CLAUDE.md r3 lessons added (external-contract rule; verify-first specs) — commit pending in next checkpoint commit. Canonical Lessons Log = linear.app/clickt/document/lessons-log-4a8de456b76a (12 entries); older duplicate retitled "superseded copy — do not use" (John: safe to delete; a third seed-era doc may also exist). M2 status update posted on the project (onTrack).
2. **Board:** In Review awaiting John's Done: HM-915,916,917,918,919,920,928. Backlog: HM-921 (frontend ControlBridge — now unblocked, next lane), HM-922 (MCP scaffold — unblocked, 917 merged), HM-929/930/932–936 (hardening/follow-ups, unblocked), HM-923/924 (blocked by 922), HM-925/926/927 (M4/M5 chain). HM-931 = canceled throwaway.
3. **Round 4 (next session or on John's go):** lanes HM-921 (frontend, Opus — first frontend lane; root.tsx/tauri-events/tauri-api allowlisted; contract comment on the issue is authoritative, brief corrected) + HM-922 (MCP TS scaffold, Opus; new top-level mcp/artcraft-mcp, no repo-surface overlap with 921) can run in parallel. Optionally + HM-930/HM-934 (Sonnet, small, disjoint). QC next round: RUST_LOG=debug; close probe-14 UI half (dismiss a card, curl id).
4. **Open decisions for John:** merge-queue Done verdicts above; HM-929 option (a) ratification; delete duplicate Lessons Log docs.
5. **Cold-start successor:** step 1 — read this entry + list_issues; if John has moved issues Done, start round 4 per item 3 (claim → worktrees off main @ cbb5ba03d6+ → briefs per orchestrator pattern; remember single-instance rule only matters for lanes that run the app; HM-921 verification needs the live app + curl scene ops with 3D tab open AND closed).

## 2026-08-12 — Round 3: reflect MERGE ×4; QC RUNNING (23 probes, M2 check); follow-up issues being filed

1. Reflect @ a696d92d30: MERGE all four; allowlist exact; enum dedup clean; no token logging; routes above auth. 20/23 probes live-only → QC battery running now on r3 worktree (incl. ONE sanctioned cheapest-model image generation with estimate first; probe 3 signed-out case SKIPPED by coordinator; probe 14 dismissal via UI automation, may return DEFERRED; double-download probe expects 400 by design).
2. Subagent filing 5 follow-up issues (TaskId return/kill mirror; estimate_cost creds; enveloped 405 fallback; media username cache; stable cursors).
3. After QC: PASS → land r3 on main (John pre-approved r2 pattern — but ASK again for r3 merge: bigger surface), prune 5 worktrees + 4 lane branches, lessons loop (reflect observations 1-7 + lane candidates), M2 milestone summary comment on the project, checkpoint. FAIL → failing lane back In Progress w/ evidence, others unaffected, no land.
4. Reflect process observations queued for lessons: spec-as-fact (handle_request premise wrong in 3 bodies); external-contract-from-internal-struct rule (deny_unknown_fields); reuse-the-matching-contract-not-matching-data (dismissed filter); brief-vs-issue contradiction = stop-and-flag (wire string, closed); single-instance deferral compounding → plan live-lane or split criteria at authoring; wrap-up honesty norm (keep); {:?} on errors in credential paths lint idea.

## 2026-08-12 — Round 3: gate done (8/10 fixed @ a696d92d30, pushed); REFLECT RUNNING (superseded)

1. Gate fixed: unknown-field rejection on 4 generate bodies; /v1/tasks/{id} by-id query (new sqlite_tasks get_task_by_id.rs, runtime-checked, no .sqlx entry); search 401→NOT_LOGGED_IN; search cursor→400; download honors AppPreferences dir; download filename vetting + create_new; scene event no longer dumps payload to logs; shared require_tauri_state/require_signed_in_credentials. Skipped→follow-ups: stable created_at+id cursor keying (wire-format change); minor duplication accepted. 143 lib tests. Two disclosed beyond-allowlist gate edits: download_media_file.rs recipe, sqlite_tasks mod.rs+new query.
2. REFLECT RUNNING on r3 worktree @ a696d92d30 (4 issues vs acceptance + integrity checks).
3. Next: reflect → QC live battery (full endpoint matrix + gate probes [search+cursor 400, typo'd generate key 400, dismissed-task-by-id 200, double-download 400] + scene SCENE_BRIDGE_TIMEOUT/unknown-op + ONE cheap real image generation with estimate_cost first = M2 milestone check) → land → prune → file queued issues → lessons → checkpoint.

## 2026-08-12 — Round 3: all 4 lanes In Review; r3 GATE RUNNING (superseded)

1. **Phase:** HM-917 (@35dd55c3d1, PR #3), HM-918 (@2b9672a195, PR #5), HM-919 (@9dee11c6f8, PR #6), HM-920 (@c575721e0d, PR #4) all PASS → In Review. Integration branch artcraft-mcp-r3 @ 1abeccb73c (pushed): 4 merges + coordinator commits (conflict resolution keep-all-anchored-lines; ControlErrorCode deduped to 8 variants; brief's stale scene wire string fixed → control_scene_request_event, contract comment posted on HM-921). Pre-checks: cargo check clean, 124 lib tests pass.
2. **GATE RUNNING:** run wf_7bb90e09-2b0 (applyFixes true). Resume: Workflow { scriptPath: "/Users/johngreenhow/.claude/projects/-Users-johngreenhow-Artcraft-worktrees-artcraft-mcp-r3/0eeda523-943b-41da-b930-921650575429/workflows/scripts/merge-gate-wf_7bb90e09-2b0.js", resumeFromRunId: "wf_7bb90e09-2b0" }.
3. **Next after gate:** reflect (4 issues vs acceptance) → QC live matrix (this closes the round-wide deferred evidence: all endpoints incl. 401/403/404/409/504 paths, scene timeout with no frontend listener, /v1/tasks list, /v1/media list, /v1/media/download, AND one cheap real image generation — estimate_cost first, cheapest model, batch 1; M2 milestone check rides on this) → land → prune → checkpoint + learnings (queued lesson candidates from all 4 lanes + handle_request overgeneralization relay incident).
4. **New-issue candidates queued (file at round close):** estimate_cost anonymous pricing; global enveloped fallback for 405/unmatched; commands return TaskId (kills generate_image mirror + null task_ids); /v1/media two-hop; scene body limit review. HM-921 now unblocked-by-920-merge after landing.

## 2026-08-12 — Round 3 in flight: 4 lanes (HM-917/918/919/920) (superseded)

1. **Phase:** round 2 LANDED on main @ 3a647feb38 (HM-916+HM-928+gate fixes; QC 6/6); lessons applied (CLAUDE.md @ cfd44d8129, Lessons Log doc lessons-log-4fb79dcb5f5b, 6 entries). Round 3 launched: 4 Opus lanes claimed In Progress with plan comments, worktrees off main @ cfd44d8129.
2. **Lanes:** hm-917 (read endpoints), hm-918 (generation endpoints, NO real generations), hm-919 (tasks/media), hm-920 (Rust scene bridge; lib.rs + tauri_event_name.rs allowlisted). Worktrees /Users/johngreenhow/Artcraft/worktrees/hm-9xx, branches = Linear gitBranchNames. Managed contention: each lane adds ONE anchored line (`// HM-9xx`) in build_control_router ABOVE the auth .layer() (post-.layer routes escape auth); endpoints/mod.rs append-only; conflicts resolved by coordinator at integration.
3. **Round-3 deviation (disclosed on all 4 issues):** live curl verification deferred to integration QC — single-app-instance constraint; lanes verify via cargo check + unit tests only.
4. **Open decisions:** John's Done verdicts on HM-915/916/928 (In Review). HM-929 option (a) refuse-on-live-pid awaiting ratification (rec (a)). Possible duplicate seeded "Lessons Log" doc in Linear (save_document may have created a second doc; the canonical one is lessons-log-4fb79dcb5f5b with 6 entries) — John can delete a stray empty duplicate if one shows.
5. **Cold-start successor:** step 1 — check HM-917..920 states. All In Review → integration branch `artcraft-mcp-r3` off main in a fresh absolute-path worktree, merge 4 lanes (expect small conflicts in build_control_router + endpoints/mod.rs: keep ALL anchored lines, order 917→918→919→920, all above .layer()), cargo check + tests, merge gate, reflect, QC incl. FULL live curl matrix (this round's deferred evidence: all new endpoints + 401s + scene timeout path; still NO real generations without estimate_cost + cheapest model), land, prune. Any lane In Progress with no live executor → inspect worktree state before re-running.

## 2026-08-12 — Round 2: reflect = MERGE both; QC RUNNING

1. **Phase:** reflect returned MERGE for HM-916 and HM-928, zero FAILs; 6 acceptance points deferred to live QC (gate rewrote the files behind the executor's live evidence). QC agent now running the probe battery on worktrees/artcraft-mcp-r2 @ 2d3b1ed427: startup log, discovery-file mode/schema, loose-mode replacement, symlink redirect, 8-case auth matrix, logged_in signed-in half (signed-out half SKIPPED by coordinator decision — protecting John's real session; covered by review+unit tests).
2. **Reflect observations queued for learnings:** stale-evidence tracking after gate fixes; "pre-existing-state path" finding class (secrets to well-known paths need hostile-state handling in briefs); recurring root package-lock.json artifact (.gitignore candidate); unused uuid dep (accepted — consumed by HM-920 scene bridge shortly).
3. **Next:** QC PASS → land artcraft-mcp-r2 on main (merge commit), push, prune worktrees hm-916/hm-928/artcraft-mcp-r2 + local lane branches (keep origin), leave HM-916/HM-928 In Review; then checkpoint + learnings loop (user-gated). QC FAIL → failing lane back to In Progress with evidence, no land.

## 2026-08-12 — Round 2: gate done (8 fixed, 2 skipped→issues); REFLECT RUNNING (superseded)

1. **Phase:** merge gate complete on artcraft-mcp-r2. Fix commit 2d3b1ed427 pushed (unlink+create_new 0600 token file; logged_in=session.is_some(); Windows ACL warning; try_state; case-insensitive Bearer; error enum trimmed; nx doc pinned 21.2.3; tests 6/6; cargo check clean). Skipped findings routed: HM-929 expanded (atomic write + shutdown cleanup + live-instance guard w/ decision options, rec (a) refuse); HM-930 filed (npx nx script fix, branch AFTER r2 lands); startup-window login note commented on HM-922.
2. **REFLECT RUNNING** (Opus, read-only) on worktree /Users/johngreenhow/Artcraft/worktrees/artcraft-mcp-r2 judging HM-916/HM-928 vs acceptance post-fixes.
3. **Next:** reflect verdicts → QC probes (incl. any deferred-to-QC items; likely re-run live curl matrix on post-fix build) → land artcraft-mcp-r2 on main → push → prune worktrees hm-916/hm-928/artcraft-mcp-r2 + local lane branches → checkpoint + learnings loop (4 lesson proposals pending user: native deps; build-time estimate; gh pr create --repo; and gate-fix classes).

## 2026-08-12 — Round 2: both lanes In Review; GATE RUNNING (superseded)

1. **Phase:** integration staged, merge gate running. HM-916 PASS (branch @ d3a4ffa3a8, PR #2), HM-928 PASS (@ dfe480cc37, PR #1), both In Review with wrap-ups. HM-929 filed (discovery-file hardening, blockedBy HM-916, Low).
2. **Integration:** worktree /Users/johngreenhow/Artcraft/worktrees/artcraft-mcp-r2, branch artcraft-mcp-r2 off main @ 46d66ad715; both lanes merged clean + coordinator fix (dropped stray root package-lock.json from HM-916 lane). Pushed. Pre-checks green: SQLX_OFFLINE cargo check 1m11s clean.
3. **GATE RUNNING:** run wf_2e69c2f2-56d (applyFixes true). If session dies: resume Workflow { scriptPath: "/Users/johngreenhow/.claude/projects/-Users-johngreenhow-Artcraft-worktrees-artcraft-mcp-r2/0eeda523-943b-41da-b930-921650575429/workflows/scripts/merge-gate-wf_2e69c2f2-56d.js", resumeFromRunId: "wf_2e69c2f2-56d" }.
4. **Next after gate:** reflect pass (orchestrator:reflect, Opus, HM-916+HM-928 vs acceptance) → QC probes → land artcraft-mcp-r2 on main → push → prune worktrees hm-916/hm-928/artcraft-mcp-r2 + delete local lane branches → checkpoint + learnings loop. PR incident lesson pending user review (gh pr create defaults to upstream; use --repo performance-clickt/artcraft --base main).

## 2026-08-12 — Round 2: HM-928 done (In Review, PR #1); HM-916 still running (superseded)

- HM-928 PASS: branch john/hm-928-... @ dfe480cc37, PR https://github.com/performance-clickt/artcraft/pull/1, only _docs/dev_setup.md touched. Incident: gh pr create defaulted to upstream storytold/artcraft (stray public PR storytold#1893 opened+closed); HM-916 executor warned to use --repo performance-clickt/artcraft --base main. LESSON CANDIDATE for CLAUDE.md git section.
- HM-916 (Opus) still running in worktrees/hm-916.

## 2026-08-12 — Round 2 in flight: HM-916 + HM-928 lanes running (superseded)

1. **Phase:** round 2 launched, 2 disjoint lanes, waiting on executors. HM-915 PASS → In Review (awaiting John's Done verdict; ~2 min cold build, not 30-60 min — planning assumption corrected). HM-928 created this round (docs gaps found by HM-915).
2. **Lanes:**
   - HM-916 (Opus): control-server skeleton. Worktree /Users/johngreenhow/Artcraft/worktrees/hm-916, branch john/hm-916-control-server-skeleton-axum-thread-discovery-file-bearer off main @ 261fce17f0. Will run the dev app for curl verification (quits installed ArtCraft; Vite :5173 in use while verifying).
   - HM-928 (Sonnet): _docs/dev_setup.md fix. Worktree /Users/johngreenhow/Artcraft/worktrees/hm-928, branch john/hm-928-fix-_docsdev_setupmd-missing-cmake-global-nx-prerequisites off main @ 261fce17f0.
   - Surfaces disjoint (crates/**+Cargo.toml vs _docs/). Both end: PR to main + wrap-up + In Review. Never self-merge.
3. **Env changes so far (user-space, disclosed):** cargo-tauri 2.11.4, global nx 23.1.1, cmake 4.4.2 (brew, John-approved).
4. **Open decisions:** John to review/close HM-915 (In Review). Lessons proposals pending user review (native-deps-in-toolchain-check; dev_setup drift) — not yet applied anywhere.
5. **Cold-start successor:** step 1 — check HM-916/HM-928 in Linear. Both In Review with PRs → stage integration branch `artcraft-mcp-r2` off main in a fresh worktree at an absolute path, merge both lane branches, run merge gate (orchestrator:merge-gate) then reflect then QC, land on main, prune worktrees. Any lane still In Progress with no live executor → stale: inspect its worktree/branch state before re-running.

## 2026-08-12 (later) — Round 1: cmake blocker resolved, executor resumed

1. **Phase:** round 1, lane HM-915 resumed after halt. Executor hit missing `cmake` (boring-sys2 build dep) at 621/810 crates and halted per rules; John approved `brew install cmake` (v4.4.2 installed); executor resumed in background to re-run the build, verify logged-in launch, post wrap-up, move HM-915 → In Review.
2. **Frontend already verified PASS** (Vite :5173, 200 OK). Toolchain: node v22.22.2, rustc/cargo 1.96.0, cargo tauri 2.11.4 (installed by executor), global nx v23.1.1 (installed by executor — undocumented prerequisite of unix_frontend_dev.sh).
3. **Lesson candidates queued for learnings loop:** (a) verify native build deps (cmake etc.) up front, not just language toolchains; (b) `_docs/dev_setup.md` omits cmake and global nx → new-issue candidate for a docs fix.
4. **Cold-start successor:** step 1 — check HM-915: In Review + wrap-up = done, start round 2 (lane HM-916); still In Progress with no build running (`pgrep -f 'cargo tauri'`) = executor died, re-run both dev scripts from repo root (cmake now present), verify, wrap up, In Review.

## 2026-08-12 — Round 1 in flight (orchestrator) (superseded)

1. **Phase:** round 1, single lane launched, waiting on executor. Board: HM-914 Done; HM-915 In Progress (owned by this session's executor); HM-916..927 Backlog, all gated on HM-915→HM-916.
2. **Running background work:** Opus executor on HM-915 (agent notification pending in this session; not resumable cross-session — if session died, treat lane as stale, see step below).
3. **Lane HM-915:** verification-only issue; runs in the LIVE tree /Users/johngreenhow/Artcraft/artcraft-src on `main` @ bed3bad43c (accepted deviation, disclosed on the issue: no branch/worktree/PR, zero commits — stock dev-build proof). Executor will post one wrap-up comment and move to In Review. Hazard: two dev processes (Vite :5173, cargo tauri dev) may be running; executor terminates them on success.
4. **Open decisions:** none.
5. **Cold-start successor:** step 1 — check HM-915 in Linear: if In Review with wrap-up, proceed to round 2 (lane = HM-916, worktree branch `john/hm-916-...` off main, Opus); if still In Progress with no wrap-up and no live build processes (`pgrep -f 'cargo tauri'`), the executor died — kill stray Vite/cargo processes, re-run HM-915 per its plan comment, in the live tree, no commits.

Coordination docs committed and pushed as bed3bad43c on main (origin=performance-clickt/artcraft).
