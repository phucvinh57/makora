# 13-frontend-structure

1. **IPC boundary**: generate TS types from Rust DTOs with `ts-rs` (derive on DTOs, export to `src/bindings/`); `types.ts` re-exports → no hand mirror. `api.ts` parses errors once: unknown code → `"unknown"`; add `provider_error`.
2. Typed dialog bus: `DialogDataMap` keyed by `DialogName` for `openDialog`/receiver (`hooks/useDialog.ts`).
3. Select values: small type-guard parsers instead of `as` (`RuleDialog.tsx:179,199`, `ExclusionsSection.tsx:106,124`, `ProviderDialog.tsx:191`); history filters as unions.
4. Query keys only via `queryKeys` (`RuleDialog.tsx:150,165`, `GeneralSection.tsx:12,22`).
5. One decision, one place: date-range math (`statistics/-components/date.ts` + `history/-components/timeRange.ts`) → `src/dateRange.ts`; rule constants (`RuleDialog.tsx:42-50`, `ExclusionsSection.tsx:10-18`) → `rules.ts`; `UNCATEGORIZED` + color → `categories.ts`; Add/Edit category dialogs → shared `CategoryForm`.
6. Dead code: `categoryPath`, `CategoryPicker.options/valueLabels`, unused registry entries.
7. Split `AiCategorizationSection` → `useCategorizationControls` + view; `RuleDialog` → `useRuleForm`; history route → `useHistorySelection`.
8. `biome migrate` (schema 2.5.4 → 2.5.15).
Verify: `bun run build`, `bun run lint`, manual smoke of every route.
