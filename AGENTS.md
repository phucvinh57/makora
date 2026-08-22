# Codex workflow

The primary agent is the execution owner. It runs as GPT-5.6 Luna at xhigh reasoning and is responsible for repository edits, validation, and the final response.

## Planning

- For non-trivial changes, spawn the `planner` agent before editing, wait for its result, and use its evidence-based plan to guide execution.
- Planning is unnecessary for a tiny, obvious, low-risk change that can be completed and verified directly.
- The primary agent remains responsible for checking the plan against the repository and the user's request.

## Advisory

- When the user says "consult advisor", "ask advisor", "use Sol", or otherwise requests advisory review, spawn the `advisor` agent with the exact decision or review question and wait for its result before proceeding.
- Also consult `advisor` before committing to a consequential architecture, security, data-migration, compatibility, or public-API decision when an independent opinion would materially reduce risk.
- Treat the advisor's response as advice, not authority. The primary agent owns the final decision and should briefly report any important tradeoff or disagreement.

## Execution

- The primary Luna agent performs implementation after planning and advisory are complete.
- Do not delegate implementation to `planner` or `advisor`; both are read-only.
- Keep delegation bounded: give each agent one concrete question, wait for its result, and summarize only the useful conclusions.
