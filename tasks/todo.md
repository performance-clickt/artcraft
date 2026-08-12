# Local scratchpad — ArtCraft MCP

Linear is the record; this file is the resume map for the next session.

## 2026-08-12 — Round 2: HM-928 done (In Review, PR #1); HM-916 still running

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
