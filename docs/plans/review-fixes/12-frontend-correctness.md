# 12-frontend-correctness

1. **WIP**: restore compact one-line disclosure in `AiCategorizationSection.tsx` only when a remote provider is selected ("Titles, URLs and category names are sent to {name}."); `AutoCategorizeSetting.tsx:100` render `<p>` only when there is text.
2. History bulk select (`routes/history/route.tsx:146,257,300`): selection = `selectedIds ∩ visible rows` (pattern from `ReviewQueue`'s `selectedGroups`); disable header checkbox while `isPlaceholderData`.
3. Switch to Local LLM (`AiCategorizationSection.tsx:37,236,383`): pick last local model from settings or first installed model, not ref/hard-coded; `blocker` explains `!modelsReady`.
4. Add-model dialog auto-reopen (`:282`): open only on transition to downloading, not on every mount.
5. History `searchInput` follows URL `q` (`route.tsx:135`); toast uses `||` (`:220`).
6. Loading flashes: `useCategories` exposes loading; `RulesTab`, `AiCategorizationSection` and `CategoryBreakdown` wait for it (skeleton) instead of treating `[]` as "no categories".
7. History source filter gains "Rule".
Verify: `bun run build`, `bun run lint`; manual in `bun tauri dev`.
