# Spin-offs: problems found while fixing an issue

While fixing or designing one thing, you find another. The rule is that a problem should not end up
filed and then forgotten. Triage it first ([`issue-triage.md`](issue-triage.md)). If it belongs to
the work in hand, extend that work. Otherwise it gets **its own branch and its own PR**, never a ride
in the current one, and it is fixed now when it is eligible for automatic fixing
([`auto-fix.md`](auto-fix.md)).

This skill runs in Claude Code (cloud, CLI, desktop), Codex, GitHub Copilot, Zed and others. Detect
what the host can do; do not assume a tool exists because another host has it.

## Decide what to do with it

| Triage result | Eligible for automatic fixing? | Action |
|---|---|---|
| Case 1 (the work in hand) | — | Extend the current design; it ships in the current PR |
| Cases 2-3 | yes, `P2` / `P3` | **Fix now**, in a spin-off (below) |
| Cases 2-3 | yes, `P0` / `P1` | Design it, then ask the user to authorize the spin-off fix |
| Cases 2-3 | no | Design it with readiness and decisions (triage case 2 or 3); no fix |
| It blocks the current fix | either | Fix it first in its own PR. The current branch either waits for it or carries a ported copy that no-ops once the spin-off merges. Say which in both PRs |

Cap the number of fix spin-offs at about **three per parent task**. When there are more, design the
rest and list them in the closing summary for the next compaction run
([`compaction.md`](compaction.md)).

## Choose the mechanism

Take the first one the host supports:

| # | Mechanism | Use when | How |
|---|---|---|---|
| A | **Child session** | The host can start an independent agent session with its own checkout and its own PR (Claude Code cloud: the `create_session` tool) | Start it from the default branch with its own branch name and the prompt below. Its container builds independently, so this is the best option for Rust fixes that need a full build. |
| B | **Delegated task** | The host can hand a self-contained task to an agent that opens its own PR (a Codex cloud task, the GitHub Copilot coding agent, a subagent with a worktree) | Hand off the same prompt. With the Copilot coding agent this needs a GitHub issue to assign. Create one only when the user agrees, because filing normally creates no GitHub issue. |
| C | **Second branch, same session** | The agent can create branches, commit and push, but cannot start other sessions | Below |
| D | **Local branch, handed off** | Commits are possible but pushing or opening PRs is not (Zed, a local CLI without credentials, a sandbox) | As C, then report the branch and the exact push and PR commands |
| E | **None** | The host cannot create branches at all (a read-only or chat-only mode) | File and design only, and say in the closing summary that the fix was not possible here |

A host that restricts pushing to one assigned branch (some cloud environments) needs the user's
explicit permission for every other branch. Ask once, naming the branches, before using C. If
permission is refused, fall back to D or E.

### Mechanism C: a second branch in the same session

```bash
# 1. Park the current work: commit it on its own branch (preferred), or `git stash push -u`.
git fetch origin <default-branch>
git worktree add ../<repo>-<slug> -b <prefix>/<slug> origin/<default-branch>
#    <prefix> follows the host's convention (e.g. claude/, codex/); <slug> names the problem.
# 2. Work only inside the new worktree. Share the build cache so the Rust build is not repeated:
CARGO_TARGET_DIR=<repo>/target cargo test -p <crate> --lib --tests
# 3. Commit, push, open the PR; then return to the original worktree and remove the extra one:
git worktree remove ../<repo>-<slug>
```

A shared `CARGO_TARGET_DIR` matters. A second `target/` for this workspace is several GB (see
`CLAUDE.md`, "Building and testing"). Sequential use is safe; do not build in both worktrees at once.
Where worktrees are unavailable, commit or stash, `git switch -c <prefix>/<slug> origin/<default>`,
and switch back afterwards.

Finish the spin-off before resuming the parent task unless it is long. A spin-off left half-done on
a branch is the same backlog problem with a different name.

## The spin-off prompt

Self-contained, because a child session or delegated agent starts with no context:

```text
Fix <ISSUE-ID> in <repo>, on a new branch <prefix>/<slug> from <default-branch>.
Problem: <one paragraph, including the problem example>.
Design: specs/design/<slug>/ (readiness: ready; automatic fixing: eligible).
Follow specs/guides/autonomous_issue_fixing.md and the liquers-project skill's spin-off rules.
Scope: this issue only. Anything else you find goes through liquers-project triage, not into this PR.
Validate with: <the narrowest test command that covers it>.
Open one PR, close the issue's local status in the same PR (DOCS_STRUCTURE_GUIDE.md §4.3), and
drive the PR until CI is green.
Found while working on: <parent issue or design>, branch <parent branch>.
```

## Track and report

- In the parent PR description and closing summary, list every spin-off: issue ID, triage case,
  mechanism, and branch or PR link or session. If one was only filed, give the reason (ineligible,
  `P0`/`P1` awaiting authorization, cap reached, or mechanism E).
- A child session or delegated task that ends cleanly may not report back. Check its status before
  you finish, and report what you see.
- Never mark the parent issue `closed` on the strength of a spin-off that has not merged.
