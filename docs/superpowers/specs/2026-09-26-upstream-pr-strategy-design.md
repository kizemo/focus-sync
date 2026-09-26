# Upstream PR Strategy Design (2026-09-26)

## Goal

Submit tree view (issue #499) and dialog focus sync (new feature) to upstream `aleksey-hoffman/sigma-file-manager` with high merge probability. Respect maintainer's time and existing plans; preserve fork self-use as a fallback.

## Key Context (verified 2026-09-26)

- **issue #499** is **maintainer-owned and reopened** (2026-08-17). Maintainer said: "Not yet, this was accidentally marked as done. Will appear in the next update." Milestone: `v2.0.0-beta.3`, project status: In progress.
- **Maintainer merge cadence**: 2~3 months per PR (last merged 2026-06-28).
- **Maintainer style**: single-feature PRs, reasonable size, conventional commits.
- **PR rejection precedents**: hetima's PRs (486, 443) both closed — likely because maintainer wants to implement features himself.
- **Our PR #546** (custom startup path): opened 2026-09-25, 0 reviews so far.

## Strategy: Progressive Communication (方案 A)

Two-step approach, ask before sending. Tree first (existing issue, maintainer has signaled intent), dialog focus sync later (new feature, no maintainer signal).

### Step 1 — Comment on issue #499

Post a respectful inquiry asking the maintainer's preference. Do **not** open a PR yet.

**Comment draft:**

```markdown
Hi @aleksey-hoffman — Thanks for reopening #499 and clarifying it was accidentally closed. I see it's planned for v2.0.0-beta.3.

In the meantime I built a tree view in my fork ([kizemo/sigma-file-manager](https://github.com/kizemo/sigma-file-manager), branch `feat/tree-sidebar-v6-1`, 6 commits, ~3000 lines incl. tests + docs). It's been my daily driver for a couple weeks.

Design notes in case it's useful:
- Pinia store for expansion state (avoids imperative ref forwarding — this bit me hard in earlier iterations)
- Tree syncs with address bar / favorites / split-view active pane
- 259 unit tests passing

If it would help, I'm happy to clean it up (drop fork markers, rebase onto upstream/main) and send a PR. If you'd rather ship your own design, no worries — I'll keep using my fork.
```

**Post via:** `gh issue comment 499 --repo aleksey-hoffman/sigma-file-manager --body "..."`

### Step 2 — Branch based on maintainer's response

| Response from maintainer | Action |
|---|---|
| "Send PR" / "Yes please" / "Sounds good" | Cherry-pick 6 tree commits to clean branch `pr/tree-sidebar` based on `upstream/main`. Drop `[fork-keep]` markers. Open PR with conventional-commit title (`feat(navigator): add sidebar tree view`). Reference issue #499. |
| "Wait, I'm close to done" | Freeze PR plan. Keep fork-only. Watch upstream `feat/*` branches + commits for tree-related work. |
| "No thanks" | Archive PR plans. Permanently fork-only. |
| No response (2~4 weeks) | Post polite follow-up comment. If still silent, start fork-only self-publishing. |

### Step 3 (later) — Repeat for dialog focus sync

When the feature is implemented and verified on fork, post a similar comment in a **new issue** (since #499 doesn't cover it). Maintainer has not pre-claimed this feature, so PR-friendly tone is appropriate.

Open issue title: "Feature Request: file dialog follows current navigator directory on focus return"

Comment draft will follow the same template as Step 1.

## Out of Scope

- Reopening #499 ourselves (maintainer controls issue state)
- Sending PR without prior maintainer signal
- Modifying fork `productName` / version (separate work stream)

## Risks

| Risk | Mitigation |
|---|---|
| Maintainer silent for weeks | Follow-up comment at 2 weeks; final follow-up at 4 weeks; switch to fork-only |
| Maintainer rejects with "I'll do it myself" | Accept gracefully, fork-only path confirmed |
| PR feedback requires significant rework | Budget 4~8 hours after PR opens |
| Upstream rebase conflicts during cherry-pick | Up to 3 hours; resolve preferring upstream style |

## Acceptance Criteria

- [x] Strategy documented in this spec
- [ ] Step 1 comment posted within 24h of spec approval
- [ ] Maintainer's response tracked in `handoff.md` and updated in this spec
- [ ] Decision branch executed based on response
- [ ] PR opened (if approved) within 1 week of maintainer approval
- [ ] Dialog focus sync step (Step 3) executed after feature lands in fork

## Spec Self-Review Notes

- **Placeholders**: None — Step 1 comment draft is verbatim text.
- **Internal consistency**: Strategy is "ask first", all steps follow that principle.
- **Scope**: One communication action + one PR (conditional) — appropriately bounded.
- **Ambiguity**: Step 2 branches are explicit; no overlap between fork-only and PR paths.