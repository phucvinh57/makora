# Trait class diagrams

Traits in `src-tauri/src`, their implementers (`..|>`) and their dependents (`-->` holds an `Arc`/`Box<dyn …>`, `..>` calls or takes it as a parameter). Split by layer.

## 1. Tracking

```mermaid
classDiagram
direction LR

class SessionTracker {
  <<trait>>
  +handle_focus(FocusChange)
  +handle_tab(TabChange)
  +handle_presence(PresenceChange)
  +close_current(at)
}
class SessionRecorder {
  <<trait>>
  +start(app, title, url, at) SessionId
  +end(id, at)
  +refine_title(id, app, title, url)
  +close_dangling()
}
class ExclusionPolicy {
  <<trait>>
  +is_excluded(app, title, url) bool
}
class SettingsStore {
  <<trait>>
  +get() Settings
  +set_min_session_seconds(s)
  +add_exclusion(e)
  +remove_exclusion(e)
  +set_llm_model(m)
  +set_embedding_model(m)
  +set_auto_categorize_hours(h)
}
class ActivityIndexer {
  <<trait>>
  +refresh()
}
class FocusWatcher {
  <<trait>>
  +start(tx Sender~WatchEvent~)
}
class ToplevelProvider {
  <<trait>>
  +NAME const
  +probe() Option~Self~
  +run(emit)
}

class FocusStateMachine
class SqliteSessionRecorder
class SettingsExclusions
class TomlSettings
class BackgroundIndexer
class LinuxFocusWatcher
class WindowsFocusWatcher
class MacFocusWatcher
class GnomeProvider
class WlrProvider

SessionTracker <|.. FocusStateMachine
SessionRecorder <|.. SqliteSessionRecorder
ExclusionPolicy <|.. SettingsExclusions
SettingsStore <|.. TomlSettings
ActivityIndexer <|.. BackgroundIndexer
FocusWatcher <|.. LinuxFocusWatcher
FocusWatcher <|.. WindowsFocusWatcher
FocusWatcher <|.. MacFocusWatcher
ToplevelProvider <|.. GnomeProvider
ToplevelProvider <|.. WlrProvider

FocusStateMachine --> SessionRecorder
FocusStateMachine --> ExclusionPolicy
SqliteSessionRecorder --> SettingsStore
SqliteSessionRecorder --> ActivityIndexer
SettingsExclusions --> SettingsStore
LinuxFocusWatcher ..> ToplevelProvider : probes Gnome, then Wlr
```

## 2. Application services and AI ports

```mermaid
classDiagram
direction LR

class CategorySvc {
  <<trait>>
  +get_list() Vec~Category~
  +get(id)
  +create(new)
  +update(id, new)
  +delete(id)
}
class RuleSvc {
  <<trait>>
  +list()
  +create(draft)
  +update(id, draft)
  +delete(id)
  +preview(kind, field, value)
  +source_suggestions()
  +path_suggestions(domain)
}
class ClassificationSvc {
  <<trait>>
  +assign_sessions(ids, cat)
  +assign_activities(ids, cat)
  +assign_ai(verdicts)
  +accept_ai_labels(ids)
  +reset()
}
class EmbeddingSvc {
  <<trait>>
  +get_missing(model, limit)
  +save(model, vectors)
  +get_vectors(model, ids)
  +get_candidates(model)
}
class HistorySvc {
  <<trait>>
  +get_sessions(page, size, filter)
  +get_activity_stats(range)
}
class LabelingSvc {
  <<trait>>
  +get_activities_to_label(recheck)
  +get_review_page(...)
  +get_review_groups(...)
  +get_feedback_stats()
}
class Embedder {
  <<trait>>
  +embed(model, texts) Vec~Embedding~
}
class Classifier {
  <<trait>>
  +classify(model, taxonomy, activities, related, on_answer)
}
class SettingsStore {
  <<trait>>
}
class ActivityIndexer {
  <<trait>>
  +refresh()
}

class SqliteCategories
class SqliteRules
class SqliteClassification
class SqliteEmbeddings
class SqliteHistory
class SqliteLabeling
class LlamaEmbedder
class LlamaClassifier
class BackgroundIndexer
class AiRunDeps {
  <<struct>>
}

CategorySvc <|.. SqliteCategories
RuleSvc <|.. SqliteRules
ClassificationSvc <|.. SqliteClassification
EmbeddingSvc <|.. SqliteEmbeddings
HistorySvc <|.. SqliteHistory
LabelingSvc <|.. SqliteLabeling
Embedder <|.. LlamaEmbedder
Classifier <|.. LlamaClassifier
ActivityIndexer <|.. BackgroundIndexer

BackgroundIndexer --> EmbeddingSvc
BackgroundIndexer --> Embedder
BackgroundIndexer --> SettingsStore

AiRunDeps --> CategorySvc
AiRunDeps --> LabelingSvc
AiRunDeps --> ClassificationSvc
AiRunDeps --> Classifier
AiRunDeps --> Embedder
AiRunDeps --> EmbeddingSvc
```

## 3. DB repository traits

All implemented by `SqliteConnection`. The `Sqlite*` services take a connection from `Db` and call the repo traits on it.

```mermaid
classDiagram
direction LR

class ActivitiesRepo { <<trait>> +get() +get_by_identity() +insert() +get_pins_many() +set_category_many() +unpin_categories() +clear_all_pins() +get_prunable_many() +delete_many() +get_identities() }
class AiLabelsRepo { <<trait>> +get_many() +upsert_many() +clear_active_many() +reject_active_many() +delete_for_activities() +delete_for_categories() +delete_all() }
class CategoriesRepo { <<trait>> +get_list() +get() +filter_existing() +get_parent_pairs() +insert() +update() +delete_many() }
class SessionsRepo { <<trait>> +get() +get_last() +insert() +reopen() +set_end() +reassign_activity() +delete() +get_activity_ids_for() +get_open_activity_ids() +delete_open() }
class SourcesRepo { <<trait>> +get_by_key() +insert() +set_name() +delete_unused_many() }
class CategoryRulesRepo { <<trait>> +get_list() +insert() +update() +delete() +delete_for_categories() }
class RuleMatchesRepo { <<trait>> +replace_all() +set() }
class ActivityEmbeddingsRepo { <<trait>> +get_missing() +upsert_many() +get_many() +get_categorized() }
class ReviewRepo { <<trait>> +get_activities_to_label() +get_review_page() ... }
class RulesReportRepo { <<trait>> +get_rule_stats() +get_source_suggestions() +get_paths_of() +get_activities_for_preview() }
class HistoryRepo { <<trait>> +get_session_count() +get_sessions() +get_totals() +get_category_stats() +get_app_stats() +get_daily_stats() }

class SqliteConnection
class SqliteCategories
class SqliteRules
class SqliteClassification
class SqliteEmbeddings
class SqliteHistory
class SqliteLabeling
class SqliteSessionRecorder

ActivitiesRepo <|.. SqliteConnection
AiLabelsRepo <|.. SqliteConnection
CategoriesRepo <|.. SqliteConnection
SessionsRepo <|.. SqliteConnection
SourcesRepo <|.. SqliteConnection
CategoryRulesRepo <|.. SqliteConnection
RuleMatchesRepo <|.. SqliteConnection
ActivityEmbeddingsRepo <|.. SqliteConnection
ReviewRepo <|.. SqliteConnection
RulesReportRepo <|.. SqliteConnection
HistoryRepo <|.. SqliteConnection

SqliteCategories ..> CategoriesRepo
SqliteRules ..> CategoryRulesRepo
SqliteRules ..> RulesReportRepo
SqliteClassification ..> SessionsRepo
SqliteClassification ..> CategoriesRepo
SqliteEmbeddings ..> ActivityEmbeddingsRepo
SqliteHistory ..> HistoryRepo
SqliteLabeling ..> ReviewRepo
SqliteSessionRecorder ..> SessionsRepo
SqliteSessionRecorder ..> ActivitiesRepo
```

## Notes

- Repo-trait edges come from which files mention each trait. `SqliteClassification`, `SqliteRules` and `SqliteSessionRecorder` also reach `ActivitiesRepo`, `AiLabelsRepo`, `CategoryRulesRepo`, `RuleMatchesRepo` and `SourcesRepo` through the helper modules (`categorization/store.rs`, `categorization/rules_store.rs`, `tracking/store.rs`), which are omitted.
- `lib.rs` builds everything inside `setup` and registers services with `app.manage(Box<dyn …>)`; `host/commands/*` take them as `State<Box<dyn …>>`. That layer is omitted.
