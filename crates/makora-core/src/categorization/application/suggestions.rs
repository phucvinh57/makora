//! Taxonomy suggestions: generating them at the end of an AI run, and listing, accepting and
//! dismissing them from the UI. Generation clusters the activities the model found no category
//! for and asks the model to name each new group, asks for a sharper description of each
//! category whose AI labels the user keeps moving out, and finds pairs of categories to merge.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use diesel::Connection;
use tracing::{info, warn};

use crate::categorization::domain::clustering::{cluster, samples, ClusterThresholds, Point};
use crate::categorization::domain::retrieval::SourceRef;
use crate::categorization::domain::suggestions::{
    centroid, find_merges, pick_to_describe, reconcile_new, unordered, CategoryFeedback, ClusterPlan, DescribeProposal,
    MergeCategory, NewCategoryProposal, SuggestionKind, SuggestionStatus, TaxonomySuggestion, DESCRIBE_QUIET_DAYS,
    MAX_NAMED_PER_RUN,
};
use crate::categorization::ports::{ActivityText, LlmSpec, MovedActivity, Suggester, UsageMeter};
use crate::db::activity_embeddings::ActivityEmbeddingsRepo;
use crate::db::models::{NewTaxonomySuggestionRow, TaxonomySuggestionRow};
use crate::db::taxonomy_suggestions::TaxonomySuggestionsRepo;
use crate::db::Db;
use crate::domain::types::Category;
use crate::reporting::suggestions::{SampleRow, SuggestionsReportRepo};
use crate::reporting::Window;
use crate::shared::ids::{ActivityId, CategoryId};
use crate::shared::time;
use crate::tracking::domain::source_key::url_location;

use super::embeddings::decode;

/// How many members stand for a group when the model names it.
const NAMING_SAMPLES: usize = 5;
/// How many titles a suggestion shows.
const SHOWN_TITLES: usize = 3;
/// How many kept and how many moved activities stand for a category when the model describes it.
const DESCRIBE_SAMPLES: i64 = 5;
/// How far back the user's answers count toward a description suggestion.
const FEEDBACK_DAYS: i64 = 30;

/// An activity the model found no category for, ready to cluster.
#[derive(Debug, Clone)]
pub struct UnlabeledActivity {
    pub point: Point,
    pub text: ActivityText,
}

/// A suggestion as the UI lists it. A "new category" suggestion's members are those still
/// unfiled.
#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionView {
    pub id: i64,
    pub suggestion: TaxonomySuggestion,
}

/// A stored suggestion, as generation matches against it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredSuggestion {
    pub id: i64,
    pub status: SuggestionStatus,
    /// When it was accepted or dismissed, or else made.
    pub at: Option<DateTime<Utc>>,
    pub suggestion: TaxonomySuggestion,
}

/// What says two categories may be one: the embeddings of their filed activities, the time filed
/// in each, and how many AI labels the user moved from one to another.
#[derive(Debug, Clone, Default)]
pub struct MergeSignals {
    pub vectors: Vec<(CategoryId, Vec<f32>)>,
    pub seconds: HashMap<CategoryId, f64>,
    pub corrections: HashMap<(CategoryId, CategoryId), u32>,
}

/// What generation decided to write.
#[derive(Debug, Clone, PartialEq)]
pub enum SuggestionWrite {
    Insert(TaxonomySuggestion),
    Update { id: i64, suggestion: TaxonomySuggestion },
}

pub trait TaxonomySuggestionSvc: Send + Sync {
    /// The pending suggestions, oldest first. One whose group has mostly been filed since, or
    /// whose category is gone, is left out.
    fn list(&self) -> Result<Vec<SuggestionView>>;
    /// Marks a "new category" suggestion accepted once its category exists, and returns its
    /// members still unfiled, for a run over them.
    fn accept_new(&self, id: i64) -> Result<Vec<ActivityId>>;
    /// Marks a suggestion accepted once the change it proposed is made.
    fn accept(&self, id: i64) -> Result<()>;
    fn dismiss(&self, id: i64) -> Result<()>;
    /// How many still-unfiled activities the model found no category for.
    fn count_unlabeled(&self) -> Result<usize>;

    /// The still-unfiled activities the model found no category for, with an embedding from
    /// `embedding_model`.
    fn get_unlabeled(&self, embedding_model: &str) -> Result<Vec<UnlabeledActivity>>;
    /// Every suggestion, pending or decided, for generation to match against.
    fn get_for_generation(&self) -> Result<Vec<StoredSuggestion>>;
    /// Per category, how the user answered the AI labels of activities seen since `since`.
    fn get_feedback(&self, since: DateTime<Utc>) -> Result<Vec<CategoryFeedback>>;
    /// What the user kept in `category` and what they moved out of it since `since`.
    fn get_describe_samples(&self, category: CategoryId, since: DateTime<Utc>) -> Result<(Vec<ActivityText>, Vec<MovedActivity>)>;
    /// The corrections between categories since `since` and, when there are any, the embeddings
    /// from `embedding_model` and the time of each category's filed activities.
    fn get_merge_signals(&self, embedding_model: &str, since: DateTime<Utc>) -> Result<MergeSignals>;
    /// Writes what generation decided, linked to `run_id`. Returns how many are new.
    fn save(&self, writes: &[SuggestionWrite], run_id: Option<i64>) -> Result<usize>;
}

pub struct SqliteTaxonomySuggestions {
    db: Db,
}

impl SqliteTaxonomySuggestions {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

/// Moves a pending suggestion to `status`, now; `false` when it was no longer pending.
fn decide(conn: &mut diesel::SqliteConnection, id: i64, status: SuggestionStatus) -> diesel::QueryResult<bool> {
    conn.set_status(id, SuggestionStatus::Pending.as_str(), status.as_str(), &time::to_db(Utc::now()))
}

fn decode_row(row: &TaxonomySuggestionRow) -> Option<(SuggestionStatus, TaxonomySuggestion)> {
    let kind = SuggestionKind::parse(&row.kind)?;
    let status = SuggestionStatus::parse(&row.status)?;
    match TaxonomySuggestion::decode(kind, &row.payload) {
        Ok(suggestion) => Some((status, suggestion)),
        Err(err) => {
            warn!(id = row.id, error = %err, "unreadable taxonomy suggestion");
            None
        }
    }
}

impl TaxonomySuggestionSvc for SqliteTaxonomySuggestions {
    fn list(&self) -> Result<Vec<SuggestionView>> {
        let mut conn = self.db.read()?;
        let (rows, open, categories) = (*conn).transaction(|conn| {
            let rows = conn.get_list(&[SuggestionStatus::Pending.as_str()])?;
            let open: HashSet<i64> = conn.get_unlabeled_ids()?.into_iter().collect();
            let categories: HashSet<i64> = crate::db::categories::CategoriesRepo::get_list(conn)?
                .into_iter()
                .map(|category| category.id)
                .collect();
            diesel::QueryResult::Ok((rows, open, categories))
        })?;
        let min_members = ClusterThresholds::default().min_members;
        let exists = |id: CategoryId| categories.contains(&i64::from(id));
        Ok(rows
            .iter()
            .filter_map(|row| {
                let (_, suggestion) = decode_row(row)?;
                let suggestion = match suggestion {
                    TaxonomySuggestion::New(mut proposal) => {
                        proposal.members.retain(|id| open.contains(&i64::from(*id)));
                        if proposal.members.len() < min_members {
                            return None;
                        }
                        proposal.parent_id = proposal.parent_id.filter(|id| exists(*id));
                        TaxonomySuggestion::New(proposal)
                    }
                    TaxonomySuggestion::Describe(proposal) => {
                        exists(proposal.category_id).then_some(TaxonomySuggestion::Describe(proposal))?
                    }
                    TaxonomySuggestion::Merge(proposal) => {
                        (exists(proposal.from) && exists(proposal.into)).then_some(TaxonomySuggestion::Merge(proposal))?
                    }
                };
                Some(SuggestionView { id: row.id, suggestion })
            })
            .collect())
    }

    fn accept_new(&self, id: i64) -> Result<Vec<ActivityId>> {
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            let Some(row) = conn.get(id)? else {
                return Ok(Vec::new());
            };
            if !decide(conn, id, SuggestionStatus::Accepted)? {
                return Ok(Vec::new());
            }
            let Some((_, TaxonomySuggestion::New(proposal))) = decode_row(&row) else {
                return Ok(Vec::new());
            };
            let open: HashSet<i64> = conn.get_unlabeled_ids()?.into_iter().collect();
            Ok(proposal
                .members
                .into_iter()
                .filter(|id| open.contains(&i64::from(*id)))
                .collect())
        })
        .map_err(|err: diesel::result::Error| err.into())
    }

    fn accept(&self, id: i64) -> Result<()> {
        decide(&mut self.db.write(), id, SuggestionStatus::Accepted)?;
        Ok(())
    }

    fn dismiss(&self, id: i64) -> Result<()> {
        decide(&mut self.db.write(), id, SuggestionStatus::Dismissed)?;
        Ok(())
    }

    fn count_unlabeled(&self) -> Result<usize> {
        let mut conn = self.db.read()?;
        Ok(usize::try_from(conn.count_unlabeled()?).unwrap_or(0))
    }

    fn get_unlabeled(&self, embedding_model: &str) -> Result<Vec<UnlabeledActivity>> {
        let mut conn = self.db.read()?;
        let rows = conn.get_unlabeled(embedding_model, &Window::until(Utc::now()))?;
        rows.into_iter()
            .map(|row| {
                Ok(UnlabeledActivity {
                    point: Point {
                        id: row.id.into(),
                        vector: decode(&row.vector)?,
                        seconds: row.total_seconds,
                    },
                    text: activity_text(row.app_name, row.url, row.title, row.source_kind, row.source_key, row.path),
                })
            })
            .collect()
    }

    fn get_for_generation(&self) -> Result<Vec<StoredSuggestion>> {
        let mut conn = self.db.read()?;
        let rows = conn.get_list(&[
            SuggestionStatus::Pending.as_str(),
            SuggestionStatus::Accepted.as_str(),
            SuggestionStatus::Dismissed.as_str(),
        ])?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                decode_row(row).map(|(status, suggestion)| StoredSuggestion {
                    id: row.id,
                    status,
                    at: time::parse_db(row.decided_at.as_deref().unwrap_or(&row.created_at)),
                    suggestion,
                })
            })
            .collect())
    }

    fn get_feedback(&self, since: DateTime<Utc>) -> Result<Vec<CategoryFeedback>> {
        let mut conn = self.db.read()?;
        let count = |value: i64| u32::try_from(value).unwrap_or(0);
        Ok(conn
            .get_feedback_by_category(&time::to_db(since))?
            .into_iter()
            .map(|row| CategoryFeedback {
                category_id: row.category_id.into(),
                confirmed: count(row.confirmed),
                corrected: count(row.corrected),
                rejected: count(row.rejected),
            })
            .collect())
    }

    fn get_describe_samples(&self, category: CategoryId, since: DateTime<Utc>) -> Result<(Vec<ActivityText>, Vec<MovedActivity>)> {
        let mut conn = self.db.read()?;
        let since = time::to_db(since);
        let (kept, moved) = (*conn).transaction(|conn| {
            let kept = conn.get_kept_samples(category.into(), DESCRIBE_SAMPLES)?;
            let moved = conn.get_moved_out_samples(category.into(), &since, DESCRIBE_SAMPLES)?;
            diesel::QueryResult::Ok((kept, moved))
        })?;
        let text = |row: SampleRow| activity_text(row.app_name, row.url, row.title, row.source_kind, row.source_key, row.path);
        Ok((
            kept.into_iter().map(text).collect(),
            moved
                .into_iter()
                .map(|row| {
                    let to = row.moved_to.map(CategoryId);
                    MovedActivity { text: text(row), to }
                })
                .collect(),
        ))
    }

    fn get_merge_signals(&self, embedding_model: &str, since: DateTime<Utc>) -> Result<MergeSignals> {
        let mut conn = self.db.read()?;
        let (filed, seconds, corrections) = (*conn).transaction(|conn| {
            let corrections = conn.get_correction_pairs(&time::to_db(since))?;
            if corrections.is_empty() {
                return diesel::QueryResult::Ok((Vec::new(), Vec::new(), corrections));
            }
            let filed = conn.get_categorized(embedding_model)?;
            let seconds = conn.get_filed_seconds(&Window::until(Utc::now()))?;
            Ok((filed, seconds, corrections))
        })?;
        Ok(MergeSignals {
            vectors: filed
                .into_iter()
                .map(|row| Ok((CategoryId(row.category_id), decode(&row.vector)?)))
                .collect::<Result<_>>()?,
            seconds: seconds.into_iter().map(|row| (CategoryId(row.category_id), row.seconds)).collect(),
            corrections: corrections
                .into_iter()
                .map(|row| {
                    let count = u32::try_from(row.count).unwrap_or(u32::MAX);
                    ((CategoryId(row.from_id), CategoryId(row.into_id)), count)
                })
                .collect(),
        })
    }

    fn save(&self, writes: &[SuggestionWrite], run_id: Option<i64>) -> Result<usize> {
        if writes.is_empty() {
            return Ok(0);
        }
        let created_at = time::to_db(Utc::now());
        let mut conn = self.db.write();
        conn.transaction(|conn| {
            let mut inserted = 0;
            for write in writes {
                match write {
                    SuggestionWrite::Insert(suggestion) => {
                        let payload = suggestion.payload().context("encoding a suggestion")?;
                        conn.insert(&NewTaxonomySuggestionRow {
                            kind: suggestion.kind().as_str(),
                            payload: &payload,
                            created_at: &created_at,
                            run_id,
                        })?;
                        inserted += 1;
                    }
                    SuggestionWrite::Update { id, suggestion } => {
                        let payload = suggestion.payload().context("encoding a suggestion")?;
                        conn.update_payload(*id, &payload)?;
                    }
                }
            }
            Ok(inserted)
        })
    }
}

fn activity_text(
    app_name: String,
    url: Option<String>,
    title: String,
    kind: crate::tracking::domain::source_key::KeyKind,
    key: String,
    path: String,
) -> ActivityText {
    ActivityText {
        app_name,
        url: url.as_deref().and_then(url_location),
        title,
        source: SourceRef { kind, key, path },
    }
}

/// Decides which suggestions to make, asking `llm` for names and descriptions: new categories
/// for the groups of activities the model found no category for, and sharper descriptions for
/// the categories the user keeps moving activities out of. `cancelled` is checked before each
/// request.
#[allow(clippy::too_many_arguments)]
pub fn generate(
    suggestions: &dyn TaxonomySuggestionSvc,
    suggester: &dyn Suggester,
    llm: &LlmSpec,
    embedding_model: &str,
    taxonomy: &[Category],
    meter: &UsageMeter,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<SuggestionWrite>> {
    let existing = suggestions.get_for_generation()?;
    let steps = [
        ("new categories", propose_new(suggestions, suggester, llm, embedding_model, taxonomy, &existing, meter, cancelled)),
        ("sharper descriptions", propose_descriptions(suggestions, suggester, llm, taxonomy, &existing, meter, cancelled)),
        ("merges", propose_merges(suggestions, embedding_model, taxonomy, &existing)),
    ];
    let mut writes = Vec::new();
    for (step, result) in steps {
        match result {
            Ok(step_writes) => writes.extend(step_writes),
            Err(err) => warn!(error = format!("{err:#}"), step, "could not look for taxonomy suggestions"),
        }
    }
    Ok(writes)
}

/// Clusters the activities the model found no category for and decides which groups to
/// propose, naming the new ones. A group the model cannot name is still proposed, unnamed.
#[allow(clippy::too_many_arguments)]
fn propose_new(
    suggestions: &dyn TaxonomySuggestionSvc,
    suggester: &dyn Suggester,
    llm: &LlmSpec,
    embedding_model: &str,
    taxonomy: &[Category],
    existing: &[StoredSuggestion],
    meter: &UsageMeter,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<SuggestionWrite>> {
    let unlabeled = suggestions.get_unlabeled(embedding_model)?;
    let points: Vec<Point> = unlabeled.iter().map(|activity| activity.point.clone()).collect();
    let clusters = cluster(&points, &ClusterThresholds::default());
    if clusters.is_empty() {
        return Ok(Vec::new());
    }
    let texts: HashMap<ActivityId, &ActivityText> =
        unlabeled.iter().map(|activity| (activity.point.id, &activity.text)).collect();

    let mut pending: Vec<(i64, &NewCategoryProposal)> = Vec::new();
    let mut decided: Vec<&NewCategoryProposal> = Vec::new();
    for stored in existing {
        if let TaxonomySuggestion::New(proposal) = &stored.suggestion {
            match stored.status {
                SuggestionStatus::Pending => pending.push((stored.id, proposal)),
                SuggestionStatus::Dismissed | SuggestionStatus::Accepted => decided.push(proposal),
            }
        }
    }
    let pending_by_id: HashMap<i64, &NewCategoryProposal> = pending.iter().copied().collect();

    let mut writes = Vec::new();
    let mut named = 0;
    for plan in reconcile_new(&clusters, &pending, &decided) {
        let group = &clusters[plan.cluster()];
        let sample_ids = samples(group, &points, NAMING_SAMPLES);
        let sample_texts: Vec<ActivityText> =
            sample_ids.iter().filter_map(|id| texts.get(id).map(|text| (*text).clone())).collect();
        let mut proposal = NewCategoryProposal {
            name: None,
            description: None,
            parent_id: None,
            members: group.members.clone(),
            seconds: group.seconds,
            sample_titles: sample_texts.iter().take(SHOWN_TITLES).map(|text| text.title.clone()).collect(),
        };
        let kept = match &plan {
            ClusterPlan::Update { id, .. } => pending_by_id.get(id).copied(),
            ClusterPlan::Insert { .. } => None,
        };
        if let Some(earlier) = kept.filter(|earlier| earlier.name.is_some()) {
            proposal.name = earlier.name.clone();
            proposal.description = earlier.description.clone();
            proposal.parent_id = earlier.parent_id;
        } else if plan.needs_name() && named < MAX_NAMED_PER_RUN && !cancelled() {
            named += 1;
            match suggester.name_cluster(llm, taxonomy, &sample_texts, meter) {
                Ok(Some(name)) => {
                    proposal.name = Some(name.name);
                    proposal.description = name.description;
                    proposal.parent_id = name.parent;
                }
                Ok(None) => info!(members = group.members.len(), "the model could not name a group of activities"),
                Err(err) => warn!(error = format!("{err:#}"), "naming a group of activities failed"),
            }
        }
        let suggestion = TaxonomySuggestion::New(proposal);
        writes.push(match plan {
            ClusterPlan::Update { id, .. } => {
                SuggestionWrite::Update { id, suggestion }
            }
            ClusterPlan::Insert { .. } => SuggestionWrite::Insert(suggestion),
        });
    }
    Ok(writes)
}

/// Finds pairs of categories to fold into one. Asks the model nothing: the signal is the data.
fn propose_merges(
    suggestions: &dyn TaxonomySuggestionSvc,
    embedding_model: &str,
    taxonomy: &[Category],
    existing: &[StoredSuggestion],
) -> Result<Vec<SuggestionWrite>> {
    let since = Utc::now() - Duration::days(FEEDBACK_DAYS);
    let signals = suggestions.get_merge_signals(embedding_model, since)?;
    if signals.corrections.is_empty() {
        return Ok(Vec::new());
    }
    let mut vectors: HashMap<CategoryId, Vec<&[f32]>> = HashMap::new();
    for (category, vector) in &signals.vectors {
        vectors.entry(*category).or_default().push(vector);
    }
    let categories: Vec<MergeCategory> = taxonomy
        .iter()
        .map(|category| MergeCategory {
            id: category.id,
            parent: category.parent_id,
            centroid: vectors.remove(&category.id).and_then(centroid),
            seconds: signals.seconds.get(&category.id).copied().unwrap_or(0.),
        })
        .collect();
    let known: HashSet<(CategoryId, CategoryId)> = existing
        .iter()
        .filter_map(|stored| match &stored.suggestion {
            TaxonomySuggestion::Merge(proposal) => Some(unordered(proposal.from, proposal.into)),
            _ => None,
        })
        .collect();
    Ok(find_merges(&categories, &signals.corrections, &known)
        .into_iter()
        .map(|proposal| SuggestionWrite::Insert(TaxonomySuggestion::Merge(proposal)))
        .collect())
}

/// Asks for a sharper description of each category whose AI labels the user moved out often
/// enough lately, unless one is pending or was accepted or dismissed lately: labels moved before a
/// new description still count for a while.
fn propose_descriptions(
    suggestions: &dyn TaxonomySuggestionSvc,
    suggester: &dyn Suggester,
    llm: &LlmSpec,
    taxonomy: &[Category],
    existing: &[StoredSuggestion],
    meter: &UsageMeter,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<SuggestionWrite>> {
    let now = Utc::now();
    let quiet_since = now - Duration::days(DESCRIBE_QUIET_DAYS);
    let quiet: HashSet<CategoryId> = existing
        .iter()
        .filter_map(|stored| match &stored.suggestion {
            TaxonomySuggestion::Describe(proposal) => {
                let blocks = stored.status == SuggestionStatus::Pending
                    || stored.at.is_some_and(|at| at >= quiet_since);
                blocks.then_some(proposal.category_id)
            }
            _ => None,
        })
        .collect();
    let since = now - Duration::days(FEEDBACK_DAYS);
    let mut writes = Vec::new();
    for feedback in pick_to_describe(&suggestions.get_feedback(since)?, &quiet) {
        if cancelled() {
            break;
        }
        let Some(category) = taxonomy.iter().find(|category| category.id == feedback.category_id) else {
            continue;
        };
        let (kept, moved) = suggestions.get_describe_samples(category.id, since)?;
        match suggester.describe_category(llm, taxonomy, category, &kept, &moved, meter) {
            Ok(Some(proposed)) if Some(&proposed) != category.description.as_ref() => {
                writes.push(SuggestionWrite::Insert(TaxonomySuggestion::Describe(DescribeProposal {
                    category_id: category.id,
                    current: category.description.clone(),
                    proposed,
                    corrected: feedback.moved(),
                    reviewed: feedback.reviewed(),
                })));
            }
            Ok(_) => info!(category = %category.id, "the model proposed no new description"),
            Err(err) => warn!(error = format!("{err:#}"), category = %category.id, "describing a category failed"),
        }
    }
    Ok(writes)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::categorization::ports::ClusterName;
    use crate::shared::sync::lock;
    use crate::tracking::domain::source_key::KeyKind;

    /// Suggestions held in memory.
    #[derive(Default)]
    struct Store {
        unlabeled: Vec<UnlabeledActivity>,
        existing: Vec<StoredSuggestion>,
        feedback: Vec<CategoryFeedback>,
        merge: MergeSignals,
    }

    impl TaxonomySuggestionSvc for Store {
        fn list(&self) -> Result<Vec<SuggestionView>> {
            unimplemented!()
        }
        fn accept_new(&self, _: i64) -> Result<Vec<ActivityId>> {
            unimplemented!()
        }
        fn accept(&self, _: i64) -> Result<()> {
            unimplemented!()
        }
        fn dismiss(&self, _: i64) -> Result<()> {
            unimplemented!()
        }
        fn count_unlabeled(&self) -> Result<usize> {
            Ok(self.unlabeled.len())
        }
        fn get_unlabeled(&self, _: &str) -> Result<Vec<UnlabeledActivity>> {
            Ok(self.unlabeled.clone())
        }
        fn get_for_generation(&self) -> Result<Vec<StoredSuggestion>> {
            Ok(self.existing.clone())
        }
        fn get_feedback(&self, _: DateTime<Utc>) -> Result<Vec<CategoryFeedback>> {
            Ok(self.feedback.clone())
        }
        fn get_describe_samples(&self, _: CategoryId, _: DateTime<Utc>) -> Result<(Vec<ActivityText>, Vec<MovedActivity>)> {
            Ok((vec![activity(1, 1.).text], vec![MovedActivity { text: activity(2, 1.).text, to: None }]))
        }
        fn get_merge_signals(&self, _: &str, _: DateTime<Utc>) -> Result<MergeSignals> {
            Ok(self.merge.clone())
        }
        fn save(&self, _: &[SuggestionWrite], _: Option<i64>) -> Result<usize> {
            unimplemented!()
        }
    }

    /// Names every group "Games" and describes every category as "Sharper.", and keeps the
    /// titles it was shown.
    #[derive(Default)]
    struct Namer(Mutex<Vec<Vec<String>>>);

    impl Suggester for Namer {
        fn name_cluster(&self, _: &LlmSpec, _: &[Category], samples: &[ActivityText], _: &UsageMeter) -> Result<Option<ClusterName>> {
            lock(&self.0).push(samples.iter().map(|sample| sample.title.clone()).collect());
            Ok(Some(ClusterName {
                name: "Games".into(),
                description: Some("Playing games.".into()),
                parent: None,
            }))
        }

        fn describe_category(
            &self,
            _: &LlmSpec,
            _: &[Category],
            _: &Category,
            kept: &[ActivityText],
            moved: &[MovedActivity],
            _: &UsageMeter,
        ) -> Result<Option<String>> {
            let titles = kept.iter().chain(moved.iter().map(|activity| &activity.text));
            lock(&self.0).push(titles.map(|sample| sample.title.clone()).collect());
            Ok(Some("Sharper.".into()))
        }
    }

    fn activity(id: i64, x: f32) -> UnlabeledActivity {
        let y = (1. - x * x).max(0.).sqrt();
        UnlabeledActivity {
            point: Point {
                id: ActivityId(id),
                vector: vec![x, y],
                seconds: 600.,
            },
            text: ActivityText {
                app_name: "Steam".into(),
                url: None,
                title: format!("game {id}"),
                source: SourceRef {
                    kind: KeyKind::Exe,
                    key: "steam".into(),
                    path: String::new(),
                },
            },
        }
    }

    fn llm() -> LlmSpec {
        LlmSpec {
            provider: None,
            model: "m".into(),
        }
    }

    fn run(store: &Store, namer: &Namer) -> Vec<SuggestionWrite> {
        generate(store, namer, &llm(), "e", &taxonomy(), &UsageMeter::default(), &|| false).unwrap()
    }

    fn taxonomy() -> Vec<Category> {
        [(1, "Work"), (2, "Fun")]
            .into_iter()
            .map(|(id, name)| Category {
                id: CategoryId(id),
                name: name.into(),
                color: String::new(),
                parent_id: None,
                description: Some("Vague.".into()),
            })
            .collect()
    }

    fn describe(category: i64, status: SuggestionStatus, days_ago: i64) -> StoredSuggestion {
        StoredSuggestion {
            id: category * 100,
            status,
            at: Some(Utc::now() - Duration::days(days_ago)),
            suggestion: TaxonomySuggestion::Describe(DescribeProposal {
                category_id: CategoryId(category),
                current: None,
                proposed: "Earlier.".into(),
                corrected: 5,
                reviewed: 5,
            }),
        }
    }

    fn moved_out(category: i64) -> CategoryFeedback {
        CategoryFeedback {
            category_id: CategoryId(category),
            confirmed: 2,
            corrected: 4,
            rejected: 1,
        }
    }

    #[test]
    fn a_large_enough_group_is_proposed_with_its_name() {
        let store = Store {
            unlabeled: (1..=5).map(|id| activity(id, 1.)).collect(),
            ..Store::default()
        };
        let namer = Namer::default();
        let writes = run(&store, &namer);
        let [SuggestionWrite::Insert(TaxonomySuggestion::New(proposal))] = writes.as_slice() else {
            panic!("expected one new suggestion: {writes:?}");
        };
        assert_eq!(proposal.name.as_deref(), Some("Games"));
        assert_eq!(proposal.members.len(), 5);
        assert_eq!(proposal.sample_titles.len(), SHOWN_TITLES);
        assert_eq!(lock(&namer.0)[0].len(), NAMING_SAMPLES);
    }

    #[test]
    fn too_small_a_group_is_not_proposed() {
        let store = Store {
            unlabeled: (1..=4).map(|id| activity(id, 1.)).collect(),
            ..Store::default()
        };
        assert!(run(&store, &Namer::default()).is_empty());
    }

    #[test]
    fn a_pending_named_group_is_updated_without_asking_again() {
        let earlier = NewCategoryProposal {
            name: Some("Play".into()),
            description: None,
            parent_id: None,
            members: (1..=5).map(ActivityId).collect(),
            seconds: 0.,
            sample_titles: Vec::new(),
        };
        let store = Store {
            unlabeled: (1..=6).map(|id| activity(id, 1.)).collect(),
            existing: vec![StoredSuggestion {
                id: 9,
                status: SuggestionStatus::Pending,
                at: None,
                suggestion: TaxonomySuggestion::New(earlier),
            }],
            ..Store::default()
        };
        let namer = Namer::default();
        let writes = run(&store, &namer);
        let [SuggestionWrite::Update { id: 9, suggestion: TaxonomySuggestion::New(proposal) }] = writes.as_slice() else {
            panic!("expected an update: {writes:?}");
        };
        assert_eq!(proposal.name.as_deref(), Some("Play"));
        assert_eq!(proposal.members.len(), 6);
        assert!(lock(&namer.0).is_empty());
    }

    #[test]
    fn a_category_the_user_keeps_correcting_gets_a_sharper_description() {
        let store = Store {
            feedback: vec![moved_out(1)],
            ..Store::default()
        };
        let namer = Namer::default();
        let writes = run(&store, &namer);
        let [SuggestionWrite::Insert(TaxonomySuggestion::Describe(proposal))] = writes.as_slice() else {
            panic!("expected one description: {writes:?}");
        };
        assert_eq!(proposal.category_id, CategoryId(1));
        assert_eq!((proposal.current.as_deref(), proposal.proposed.as_str()), (Some("Vague."), "Sharper."));
        assert_eq!((proposal.corrected, proposal.reviewed), (5, 7));
        assert_eq!(lock(&namer.0)[0], ["game 1", "game 2"]);
    }

    #[test]
    fn a_pending_or_lately_decided_description_holds_off_another() {
        let store = Store {
            feedback: vec![moved_out(1), moved_out(2)],
            existing: vec![
                describe(1, SuggestionStatus::Pending, 60),
                describe(2, SuggestionStatus::Accepted, 10),
            ],
            ..Store::default()
        };
        assert!(run(&store, &Namer::default()).is_empty());

        let store = Store {
            feedback: vec![moved_out(2)],
            existing: vec![describe(2, SuggestionStatus::Dismissed, 40)],
            ..Store::default()
        };
        assert_eq!(run(&store, &Namer::default()).len(), 1);
    }

    fn swapped() -> MergeSignals {
        MergeSignals {
            vectors: vec![(CategoryId(1), vec![1., 0.]), (CategoryId(2), vec![0.99, 0.141]), (CategoryId(2), vec![1., 0.])],
            seconds: HashMap::from([(CategoryId(1), 7200.), (CategoryId(2), 60.)]),
            corrections: HashMap::from([((CategoryId(1), CategoryId(2)), 3), ((CategoryId(2), CategoryId(1)), 1)]),
        }
    }

    #[test]
    fn categories_swapped_both_ways_are_proposed_for_a_merge_once() {
        let store = Store {
            merge: swapped(),
            ..Store::default()
        };
        let writes = run(&store, &Namer::default());
        let [SuggestionWrite::Insert(TaxonomySuggestion::Merge(proposal))] = writes.as_slice() else {
            panic!("expected one merge: {writes:?}");
        };
        assert_eq!((proposal.from, proposal.into), (CategoryId(2), CategoryId(1)));

        let store = Store {
            merge: swapped(),
            existing: vec![StoredSuggestion {
                id: 5,
                status: SuggestionStatus::Dismissed,
                at: None,
                suggestion: TaxonomySuggestion::Merge(proposal.clone()),
            }],
            ..Store::default()
        };
        assert!(run(&store, &Namer::default()).is_empty());
    }
}
