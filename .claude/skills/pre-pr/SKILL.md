---
name: pre-pr
description: Take finished work to open pull requests — run the checks for what changed, review the diff against REVIEW.md and fix findings, commit, and open the PR (or a gh stack of PRs when the changes are related). Use when the user runs /pre-pr, asks to open/ship a PR, or when work in a worktree is done.
---

# Pre-PR

Makora has no AI reviewer in CI; this is the review. Report evidence, not claims.

## 1. Scope and shape

- Never work on `main`. If the work isn't in a worktree yet, move it into one under `.claude/worktrees/` first.
- Look at the change: `git status --short` and `git diff main...HEAD --stat`.
- Classify it by the **Risk tiers** in `REVIEW.md`. The highest tier touched wins.
- Choose the shape:
  - **Single PR**: one coherent change.
  - **Stack** (`gh stack`): related changes where each layer builds on the one below and is reviewable on its own. For example, migration + db layer → Tauri commands → UI, or a refactor followed by the feature that needs it. Keep each layer small and keep it passing checks on its own.
  - **Unrelated changes** don't go in a stack. Each gets its own worktree and its own PR.

## 2. Verify

- Run `scripts/check.sh` in the background. It picks Biome/build, `ext:check`, and clippy/`cargo test` from the changed paths.
- For a UI change, launch the app with the `run` skill and screenshot the changed screen.
- For a new migration, confirm the db was backed up first (AGENTS.md rule).

Fix failures before reviewing. Keep the output for the PR body.

## 3. Review

Review with fresh context, following `REVIEW.md`. For a stack, review each layer against its parent.

- Run the `code-review` skill: `low` for Low tier, `medium` for Medium, `high` for High.
- Run the `security-review` skill as well when a High-tier path is touched.

Fix every Important finding. Resolve the rest by fixing it or giving a one-line reason. Re-run steps 2–3, reviewing only the new changes, for at most 3 rounds. If findings are still coming after round 3, stop and question the approach with the user instead of patching lines.

If the same kind of finding keeps coming up, propose fixing it at the source: a rule in `AGENTS.md`, a line in `REVIEW.md`, or a lint rule.

## 4. Commit and open the PR

- Branch names: `claude/<topic>`; stack layers `claude/<topic>-<layer>`.
- Commit messages follow the repo's log: `fix(scope): …` for fixes, a plain imperative sentence for features.
- The pre-push hook re-runs `scripts/check.sh`. Never pass `--no-verify`.

Write each PR body to a file in the scratchpad, following `.github/pull_request_template.md`:

- **What changed**: the resulting behavior, not the file list.
- **Testing**: the commands actually run and their results, plus screenshots.
- **Risk**: the tier and what could break.
- **How to revert**: whether `git revert` is enough, or what else a revert needs (restore the db backup, a settings.toml format change, re-register the bridge manifest).
- Any findings left open, with their reasons.

End each body with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

### Single PR

```sh
git switch -c claude/<topic>        # if still on the worktree's default branch
git add -A && git commit -m "<message>"
git push -u origin HEAD
gh pr create --base main --title "<title>" --body-file <body.md>
```

If the branch already has a PR, push and run `gh pr edit --body-file <body.md>` instead.

### Stack

Plan the layers before committing, then build them bottom-up from `main`:

```sh
gh stack init claude/<topic>-<layer1>            # branches from main
git add <layer1 files> && git commit -m "<message>"
gh stack add -Am "<message>" claude/<topic>-<layer2>
gh stack view --short
gh stack submit --auto --open                    # pushes every layer, opens one PR per layer
gh pr edit claude/<topic>-<layerN> --title "<title>" --body-file <bodyN.md>   # for each layer
```

To fix a layer after review: check it out (`gh stack checkout <branch>`, or `gh stack down`/`up`), commit, then `gh stack rebase --upstack` and `gh stack push`. Once lower PRs merge, `gh stack sync --prune` rebases the rest onto `main`.

## 5. Report

List the PR URLs, each with its risk tier and check results.
