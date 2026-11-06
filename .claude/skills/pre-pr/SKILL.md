---
name: pre-pr
description: Take finished work to open pull requests — run the checks for what changed, review the diff against docs/REVIEW.md and fix findings, commit, and open the PR (or a gh stack of PRs when the changes are related). Use when the user runs /pre-pr, asks to open/ship a PR, or when work in a worktree is done.
---

# Pre-PR

There is no AI reviewer in CI; this is the review. Report evidence, not claims.

## 1. Scope

- Never work on `main`. Move the work into a worktree under `.claude/worktrees/` if it isn't in one.
- Look at the change: `git status --short`, `git diff main...HEAD --stat`.
- Pick its tier from **Risk tiers** in `docs/REVIEW.md`.
- Pick the shape:
  - **Single PR** for one coherent change.
  - **Stack** (`gh stack`) when each layer builds on the one below and is reviewable alone, e.g. migration + db → core → UI, or refactor → feature. Each layer passes checks on its own.
  - **Unrelated changes** get separate worktrees and PRs.

## 2. Verify

- Run `scripts/check.sh` in the background. Fix failures before reviewing and keep the output for the PR body.
- UI change: launch the app with the `run` skill and screenshot the changed screen.
- New migration: confirm the db was backed up first.

## 3. Review

Review with fresh context, following `docs/REVIEW.md`. For a stack, review each layer against its parent.

- Run `code-review` at the tier's level: `low`, `medium` or `high`.
- High tier: also run `security-review`.

Fix every Important finding. Fix the rest or give a one-line reason. Repeat steps 2–3 on the new changes only, up to 3 rounds. If findings keep coming, stop and question the approach with the user.

If the same kind of finding recurs, propose a fix at the source: an `AGENTS.md` rule, a `docs/REVIEW.md` line, or a lint.

## 4. Commit and open the PR

- Branches: `claude/<topic>`; stack layers `claude/<topic>-<layer>`.
- Commit messages: `fix(scope): …` for fixes, a plain imperative sentence for features.
- Never pass `--no-verify`; the pre-push hook re-runs `scripts/check.sh`.

Write each PR body to a scratchpad file, following `.github/pull_request_template.md`:

- **What changed**: the resulting behavior, not the file list.
- **Testing**: commands actually run and results, plus screenshots.
- **Risk**: the tier and what could break.
- **How to revert**: whether `git revert` is enough, or what else is needed (restore the db backup, a settings.toml format change, re-register the bridge manifest).
- Open findings with their reasons.

End each body with `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.

### Single PR

```sh
git switch -c claude/<topic>        # if still on the worktree's default branch
git add -A && git commit -m "<message>"
git push -u origin HEAD
gh pr create --base main --title "<title>" --body-file <body.md>
```

If the branch already has a PR, push and run `gh pr edit --body-file <body.md>`.

### Stack

Plan the layers, then build bottom-up from `main`:

```sh
gh stack init claude/<topic>-<layer1>
git add <layer1 files> && git commit -m "<message>"
gh stack add -Am "<message>" claude/<topic>-<layer2>
gh stack view --short
gh stack submit --auto --open                    # pushes every layer, one PR each
gh pr edit claude/<topic>-<layerN> --title "<title>" --body-file <bodyN.md>
```

To fix a layer: `gh stack checkout <branch>` (or `gh stack down`/`up`), commit, `gh stack rebase --upstack`, `gh stack push`. After lower PRs merge, `gh stack sync --prune`.

## 5. Watch CI

For each PR (every layer of a stack):

1. Wait in the background: `gh pr checks <pr> --watch --fail-fast`.
2. On failure, read the log: `gh run view <run-id> --log-failed | tail -80` (run id from `gh pr checks <pr> --json name,state,link`).
3. Reproduce with `scripts/check.sh`, fix on the PR's branch, commit (`fix(ci): …` or `fix(<scope>): …`) and push. For a stack, then `gh stack rebase --upstack` and `gh stack push`.
4. Repeat until all checks pass. Skipped checks count as passing.

If the failure isn't caused by the PR (it fails on `main` too, or it's CI setup), fix it in its own PR or on `main` if the user allows, then merge `main` in. Stop and ask after 3 failed rounds on one check, or when the failure is flaky or infrastructure.

## 6. Report

List each PR URL with its risk tier and final check results.
