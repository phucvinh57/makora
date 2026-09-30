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
| **AI suggestion** | The model's proposal for an activity's category, with a confidence. The user can **accept** it (it becomes a pin), change it, or **reject** it. |
| **Resolved category** | The category an activity is actually shown under: the pin if there is one, otherwise the AI suggestion the user has not rejected. |

## Bounded contexts

Who depends on whom (an arrow means "imports from"):

```
host ──────────► tracking, taxonomy, categorization, reporting, ai
ai ────────────► categorization   implements the Classifier port
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
| **categorization** | How an activity comes to be filed under a category: pins, AI suggestions, rejection, acceptance, and running the model over uncategorized activities. | [categorization/](../src-tauri/src/categorization/) |
| **reporting** | Read models: the history list, statistics, the AI review queue. Never writes. | [reporting/](../src-tauri/src/reporting/) |
| **ai** | Infrastructure, not a context: llama.cpp inference and model downloads, behind the `Classifier` port. | [ai/](../src-tauri/src/ai/) |
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

- **Categorization** — [`ActivityClassification`](../src-tauri/src/categorization/domain/classification.rs) is the aggregate for one activity's pin and AI suggestion. It owns:
  - a pin wins over a suggestion;
  - unpinning rejects the active suggestion, so "Uncategorized" sticks and the model learns from it;
  - the model can never overwrite a pin;
  - accepting a suggestion turns it into a pin;
  - merging two activities carries the pin across unless the two disagree (`PinConflict`).

  Nothing else decides these. Use cases call `pin_to`, `unpin`, `suggest`, `withdraw_suggestion`, `accept_suggestion`, `carry_pin_into`. [`store.rs`](../src-tauri/src/categorization/store.rs) loads a batch of aggregates and writes back only the difference, in a fixed number of queries.
- **Tracking** — [`session_policy.rs`](../src-tauri/src/tracking/domain/session_policy.rs) decides what counts as a block: under one second is noise and is dropped, a resume within two seconds of a close is the same block, and shell windows are ignored. [`FocusStateMachine`](../src-tauri/src/tracking/domain/focus_machine.rs) tracks the current block across focus, tab and presence changes. [`SourceKey`](../src-tauri/src/tracking/domain/source_key.rs) and `canonical_title` define identity.
- **Taxonomy** — [`CategoryTree`](../src-tauri/src/taxonomy/domain.rs) answers "which categories are under this one" and "in what order can a branch be deleted".

**Value objects** replace primitives: `SessionId`, `ActivityId` and `CategoryId` (serialized as plain numbers, so the frontend contract is unchanged), `Confidence` (clamped 0..=1), `SourceKey`, `KeyKind`, `CategorySource`, `TimeRange`.

## The flows

**Writing (tracking).** The window watchers and the extension endpoint send domain events (`FocusChange`, `TabChange`, `PresenceChange`) to a `SessionTracker`. `FocusStateMachine` decides what they mean and calls a `SessionRecorder`, which `SqliteSessionRecorder` implements: it finds or creates the source and activity and opens, merges, closes or drops the session.

**Writing (categorization).** A person pins or unpins (`ClassificationSvc`), or the model runs (`AiRunState::run`). The run loads the taxonomy, the activities without a suggestion and the user's past corrections; hands them to a `Classifier`; and saves each answer as it arrives, so a stopped run keeps its progress. The concrete classifier is [`LlamaClassifier`](../src-tauri/src/ai/llm.rs), wired in `lib.rs`.

**Reading.** Reporting is a separate, read-only model. Every query joins the `activity_category` view (migration 7), which is the SQL form of the resolved-category rule: pin, else unrejected AI suggestion. The rule therefore exists once in Rust and once in SQL, side by side, rather than being re-derived in each query. `CategoryScope` is the one shared filter over resolved categories.

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
