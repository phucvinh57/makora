# Persistence

One SQLite file, `makora.sqlite`, in the app-data directory (`AppPaths`, `crates/makora-core/src/paths.rs`). Migrations are in `crates/makora-core/src/db/migrations/` and applied by `db/migrate.rs`; `db/schema.rs` is the hand-written Diesel schema after the latest one. Back up the database before applying a new migration.

Each table belongs to one context. Others may read it, but only the owner writes it.

| Table                 | Owner           | Holds                                                           |
| --------------------- | --------------- | --------------------------------------------------------------- |
| `sources`             | Tracking        | Apps and domains                                                |
| `activities`          | Tracking (row), Categorization (`category_id`, the pin) | One thing done in a source          |
| `sessions`            | Tracking        | Time blocks on an activity                                      |
| `categories`          | Taxonomy        | The category tree                                               |
| `category_rules`      | Categorization  | The person's rules                                              |
| `rule_matches`        | Categorization  | The winning rule per activity                                   |
| `ai_labels`           | Categorization  | The model's suggestion per activity, and whether it was rejected |
| `activity_embeddings` | Categorization  | One vector per activity, tagged with the model that made it     |
| `ai_runs`             | Categorization  | One record per AI run                                           |
| `ai_unlabeled`        | Categorization  | Activities the model found no category for                      |
| `taxonomy_suggestions`| Categorization  | Proposed new categories, descriptions and merges                |
| `activity_category`   | Categorization (view) | The resolved category, read by Reporting                  |

## Tracking tables

```mermaid
erDiagram
    sources ||--o{ activities : "has"
    activities ||--o{ sessions : "has"
    sources {
        int id PK
        text kind "exe | domain"
        text key
        text name
    }
    activities {
        int id PK
        int source_id FK
        text path
        text title
        int category_id FK "the pin"
    }
    sessions {
        int id PK
        int activity_id FK
        text start_time
        text end_time
        text last_seen
    }
```

## Categorization tables

```mermaid
erDiagram
    activities ||--o| ai_labels : "suggestion"
    activities ||--o| rule_matches : "winning rule"
    activities ||--o| activity_embeddings : "vector"
    category_rules ||--o{ rule_matches : "wins for"
    categories ||--o{ category_rules : "files under"
    categories ||--o{ ai_labels : "suggested"
    categories ||--o{ rule_suggestion_dismissals : "not suggested"
    activities ||--o| ai_unlabeled : "no category fits"
    ai_runs ||--o{ taxonomy_suggestions : "proposed by"
    ai_labels {
        int activity_id PK
        int category_id FK
        bool rejected
    }
    rule_matches {
        int activity_id PK
        int rule_id FK
    }
    category_rules {
        int id PK
        int category_id FK
        text kind "app | site | page | pattern"
        text field "'' | url | title | app"
        text value
    }
    rule_suggestion_dismissals {
        text kind PK
        text field PK
        text value PK
        int category_id PK
        text dismissed_at
    }
    activity_embeddings {
        int activity_id PK
        text model
        blob vector
    }
    ai_unlabeled {
        int activity_id PK
        text model
        text answered_at
    }
    taxonomy_suggestions {
        int id PK
        text kind "new | describe | merge"
        text payload "JSON"
        text status "pending | accepted | dismissed"
        text created_at
        int run_id FK
    }
```

`rule_suggestion_dismissals` holds the rule suggestions the user dismissed, one per (matcher, category). They cascade with the category.

`ai_runs` holds the trigger, provider, model, status, timestamps, counts, tokens and timings of each run.

`ai_unlabeled` holds the activities the model answered "None" for. `ai_labels` only holds categories the model picked, so this is what tells a "None" answer from an activity never asked about. `ClassificationSvc::assign_ai` writes a row; `store::save_changes` deletes it once the activity has a pin or an active suggestion; a reset clears the table, and the row cascades with its activity.

`taxonomy_suggestions` holds the changes to the taxonomy proposed from the data. `payload` is JSON whose shape depends on `kind` (`domain/suggestions.rs`); a "new category" payload lists its member activity ids, so it is checked against `ai_unlabeled` when read rather than kept in step by foreign keys. `run_id` is the run that proposed it, `NULL` once that run's record is gone.

## Taxonomy table

```mermaid
erDiagram
    categories ||--o{ categories : "parent of"
    categories {
        int id PK
        text name
        text color
        int parent_id FK
        text description
    }
```

## The `activity_category` view

The view is the published language between Categorization and Reporting: the precedence rules written once in SQL.

```sql
SELECT a.id AS activity_id,
       COALESCE(a.category_id, r.category_id, l.category_id) AS category_id,
       CASE WHEN a.category_id IS NOT NULL THEN 'manual'
            WHEN r.id IS NOT NULL THEN 'rule'
            ELSE 'ai' END AS source,
       CASE WHEN a.category_id IS NULL THEN r.id END AS rule_id
FROM activities a
LEFT JOIN rule_matches m ON m.activity_id = a.id
LEFT JOIN category_rules r ON r.id = m.rule_id
LEFT JOIN ai_labels l ON l.activity_id = a.id AND l.rejected = 0
WHERE a.category_id IS NOT NULL OR r.id IS NOT NULL OR l.activity_id IS NOT NULL;
```

An activity with no row in the view is uncategorized.

## Transactions

Each application service does one use case in one transaction, so the database never shows half a change:

- Opening, closing or retitling a session, including matching a new activity against the rules and carrying a pin across.
- Creating, editing or deleting a rule, including rewriting `rule_matches`.
- Deleting a category branch, including releasing its pins and AI labels.
- Pinning, unpinning, accepting, and undoing an assignment.

Writes go through `Db::write`, reads through `Db::read` (`db/mod.rs`).
