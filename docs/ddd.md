# Domain-driven design in Makora

How the Rust backend (`src-tauri/src`) is organised, and why. Read this before adding a feature: it says where the code belongs and which way dependencies may point.

## The language

These words mean one thing each, in code, UI and conversation.

| Term | Meaning |
|---|---|
| **Source** | Where activity comes from: a website's domain, or an executable. Identified by a `SourceKey` (`kind` + `key`). |
| **Activity** | What was being done: a source, a page path and a title. Its identity is `(source, path, title)`. |
| **Session** | One block of tracked time on an activity. A session opens when an app or tab gains focus and closes when it loses it. |
| **Category** | A node in the user's category tree. Names are unique only among siblings. |
| **Pin** | The user's own decision that an activity belongs to a category. |
| **Rule** | A matcher the user wrote — an app, a site, a page or a pattern — that files every activity it matches under a category, past and future. |
| **Embedding** | A vector made from an activity's app, page and title by a small model, stored when the activity is saved. Activities that look alike have vectors that point the same way. |
| **AI suggestion** | The model's label for an activity. For each unlabeled activity the model is shown the five activities most similar to it that the user has filed, by pin or by rule (found by ranking embeddings), so it sees how things that actually look like the one it is labeling were filed. The user can **accept** it (it becomes a pin), change it, or **reject** it. |
| **Resolved category** | The category an activity is actually shown under: the pin if there is one, otherwise the winning rule, otherwise the AI suggestion the user has not rejected. |

## Bounded contexts

Who depends on whom (an arrow means "imports from"):

```
host ──────────► tracking, taxonomy, categorization, reporting, ai
ai ────────────► categorization   implements the Classifier port (one per method kind)
tracking ──────► categorization   carries a pin across a merge; drops labels of pruned activities
taxonomy ──────► categorization   deleting a category removes its pins and labels
categorization ► taxonomy         reads the tree to run the model
reporting ─────► taxonomy         expands a category filter to its subtree
reporting reads every context's tables, through the activity_category view
```

| Context | Owns | Code |
|---|---|---|
| **tracking** | Turning focus, tab and presence changes into sessions; what counts as a session; source and activity identity. | [tracking/](../src-tauri/src/tracking/) |
| **taxonomy** | The category tree: creating, renaming, re-parenting, deleting. | [taxonomy/](../src-tauri/src/taxonomy/) |
| **categorization** | How an activity comes to be filed under a category: pins, rules, AI suggestions, rejection, acceptance, and running the model over activities no pin or rule covers. | [categorization/](../src-tauri/src/categorization/) |
| **reporting** | Read models: the history list, statistics, the AI review queue. Never writes. | [reporting/](../src-tauri/src/reporting/) |
| **ai** | Infrastructure, not a context: llama.cpp embedding and inference and model downloads, behind the `Embedder` and `Classifier` ports. | [ai/](../src-tauri/src/ai/) |
| **host** | The Tauri shell: IPC commands, tray, error mapping. Glue only. | [host/](../src-tauri/src/host/) |

[`shared/`](../src-tauri/src/shared/) is the small kernel every context may use: typed ids, `Confidence`, and the one place that knows the stored time format.

## Layers inside a context

Each context uses the same shape (not every context needs every layer):

| Layer | Contains | May depend on |
|---|---|---|
| `domain/` | Entities, value objects and rules as plain Rust. No Diesel, no Tauri, no I/O. | `shared` and other contexts' *domain* only |
| `ports.rs` | Traits for what the context needs from outside (`SessionRecorder`, `Classifier`) or offers to drivers (`SessionTracker`). | `domain` |
| `application/` | Use cases: load, ask the domain to change, save, in one transaction. | `domain`, `ports`, `store` |
| `store.rs` | Diesel queries for that context's tables. | `db` |
| `adapters/` | Translate the outside world into domain events (window watchers, the browser-extension endpoint). | everything inside the context |

The rule: **dependencies point inward, toward `domain/`**. `host` and `ai` sit outside and depend on the contexts, never the reverse.

## Where the rules live

**One rule, one place.** These are the rules the code enforces, and the only file that enforces each.

- **Categorization** — [`ActivityClassification`](../src-tauri/src/categorization/domain/classification.rs) is the aggregate for one activity's pin and AI suggestion (one method's answer, or the user's rejection of it). It owns:
  - a pin wins over a suggestion;
  - unpinning rejects the active suggestion, so "Uncategorized" sticks and the model learns from it;
  - the model can never overwrite a pin;
  - accepting a suggestion turns it into a pin;
  - merging two activities carries the pin across unless the two disagree (`PinConflict`).

  Nothing else decides these. A rule sits between the two: it never overrides a pin, and the model is not asked about an activity a rule covers (that precedence is also in the `activity_category` view). Use cases call `pin_to`, `unpin`, `suggest`, `withdraw_suggestion`, `accept_suggestion`, `carry_pin_into`. [`store.rs`](../src-tauri/src/categorization/store.rs) loads a batch of aggregates and writes back only the difference, in a fixed number of queries: a pin goes to `activities`, the model's answer or the user's rejection of it to `ai_labels` (one row per activity, `rejected` says which).
- **Retrieval** — [`retrieval.rs`](../src-tauri/src/categorization/domain/retrieval.rs) owns what text stands for an activity (`activity_text`) and `top_related`, which ranks the filed activities by the similarity of their embeddings to an activity's and keeps the closest `RELATED_COUNT`, never the activity itself.
- **Rules** — [`rules.rs`](../src-tauri/src/categorization/domain/rules.rs) owns what a rule is and which one wins. A rule is an app, a site, a page (a domain and a path prefix, matched on whole path segments) or a glob pattern over the URL, title or app name; everything is compared case-insensitively. When several match one activity, the most specific wins: page (longer path first), then app or site, then pattern (more literal text first), and a tie goes to the oldest rule. [`rules_store.rs`](../src-tauri/src/categorization/rules_store.rs) stores the winner per activity in `rule_matches` and rewrites it whenever a rule changes, a category with rules is deleted, or a new activity appears, so reads never evaluate a rule. Rules are retroactive for that reason: saving one re-evaluates all of history in the same transaction.
- **Tracking** — [`session_policy.rs`](../src-tauri/src/tracking/domain/session_policy.rs) decides what counts as a block: under one second is noise and is dropped, a resume within two seconds of a close is the same block, and shell windows are ignored. [`FocusStateMachine`](../src-tauri/src/tracking/domain/focus_machine.rs) tracks the current block across focus, tab and presence changes. [`SourceKey`](../src-tauri/src/tracking/domain/source_key.rs) and `canonical_title` define identity.
- **Taxonomy** — [`CategoryTree`](../src-tauri/src/taxonomy/domain.rs) answers "which categories are under this one" and "in what order can a branch be deleted".

**Value objects** replace primitives: `SessionId`, `ActivityId` and `CategoryId` (serialized as plain numbers, so the frontend contract is unchanged), `Confidence` (clamped 0..=1), `SourceKey`, `KeyKind`, `CategorySource`, `TimeRange`.

## The flows

**Writing (tracking).** The window watchers and the extension endpoint send domain events (`FocusChange`, `TabChange`, `PresenceChange`) to a `SessionTracker`. `FocusStateMachine` decides what they mean and calls a `SessionRecorder`, which `SqliteSessionRecorder` implements: it finds or creates the source and activity and opens, merges, closes or drops the session.

**Writing (embeddings).** When an activity is saved for the first time the recorder calls `ActivityIndexer::refresh`, and [`BackgroundIndexer`](../src-tauri/src/categorization/application/indexing.rs) embeds, on a thread of its own so tracking never waits, every activity that has no vector from the configured embedding model: the new one, the ones recorded before embeddings existed (it is also asked at startup), and all of them again after the embedding model is changed or downloaded. [`LlamaEmbedder`](../src-tauri/src/ai/embedding.rs) is a small model such as `all-minilm` run through the same llama.cpp and downloader as the LLM; vectors are stored in `activity_embeddings` with the name of the model that made them.

**Writing (categorization).** A person pins or unpins (`ClassificationSvc`), adds, edits or deletes a rule (`RuleSvc`), or the model runs (`AiRunState::run`). A person starts with no categories and needs to add only those: no examples are asked for. A run then:

1. loads the taxonomy (with none the run refuses, `no_categories`) and the activities still without a label, and embeds any activity the indexer has not reached yet;
2. loads the activities the user has filed, by pin or by rule — a rule counts even though the model is never asked about what a rule covers — and ranks them against each unlabeled activity, keeping the five nearest;
3. hands the activities to the `Classifier` ([`LlamaClassifier`](../src-tauri/src/ai/llm.rs), a local LLM) whose prompt for each one contains those five, not a fixed set, and saves each answer as it arrives, so a stopped run keeps its progress.

The two model names live in `settings.toml` under `[ai]`. The model cache keeps the embedding model and the LLM loaded together, so a second run does not reload either.

**Reading.** Reporting is a separate, read-only model. Every query joins the `activity_category` view (migration 8), which is the SQL form of the resolved-category rule: pin, else rule, else unrejected AI suggestion. The rule therefore exists once in Rust and once in SQL, side by side, rather than being re-derived in each query. `CategoryScope` is the one shared filter over resolved categories.

## Errors

Each context that can refuse a request has a typed error: `TaxonomyError` (name taken, parent missing, cycle, not found), `AiRunError` and `ModelDownloadError` (already running). [`host/error.rs`](../src-tauri/src/host/error.rs) is the one place they become `{ code, message }` for the frontend. [`src/api.ts`](../src/api.ts) turns that into an `ApiError` whose `String(err)` is the message and whose `code` can be branched on.

## Adding to the codebase

1. **Which context?** Ask whose language the change is in. If it needs a new word from the table above, that is a sign it belongs to a context of its own.
2. **A rule** goes in `domain/` as a plain method or function. If you reach for Diesel or a clock, it is not a domain rule.
3. **A change that spans several rows** is an application use case: load the aggregate, call it, save, in one transaction. Do not write the same rule in SQL for a write.
4. **A new read** goes in `reporting/` and joins `activity_category`; it does not recompute the resolved category.
5. **A new command** is a thin function in `host/commands/<context>.rs`: parse arguments, call one service, return `Result<_, IpcError>`. Register it in `lib.rs`.
6. **Talking to the outside** (an OS API, a model, a network call) is a port in the context plus an adapter behind it.
7. **New ids** get a newtype in `shared/ids.rs`; the repository layer stays in `i64`.

## Where it is not finished

These are known departures from the model above, not oversights to copy:

- The old [`domain/`](../src-tauri/src/domain/) module survives. It holds `types.rs` (the shapes the frontend sees, including `Category`, `SessionRow` and the review types) and the history and review-queue services. They belong in `taxonomy` and `reporting`.
- `application/` services (`SqliteSessionRecorder`, `SqliteCategories`, `SqliteClassification`) mix use-case logic with Diesel transactions. Only the `domain/` folders are free of Diesel.
- Contexts are not fully independent. `taxonomy` and `categorization` depend on each other (the delete cascade one way, the model run reading the tree the other), and `tracking` calls `categorization::store` directly for the pin carry and label cleanup. `CategoryTree` still works in raw `i64`.
- The top-apps statistic selects a bare `cat.color` while grouping by source, so a source spanning several categories gets an arbitrary colour.
- There are no automated tests (`CLAUDE.md`: not for now), so these rules are guarded by the structure above and by review, not by a suite.
