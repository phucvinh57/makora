//! Taxonomy suggestions: generating them at the end of an AI run, and listing, accepting and
//! dismissing them from the UI. Generation clusters the activities the model found no category
//! for and asks the model to name each new group.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::Utc;
use diesel::Connection;
use tracing::{info, warn};

use crate::categorization::domain::clustering::{cluster, samples, ClusterThresholds, Point};
use crate::categorization::domain::retrieval::SourceRef;
use crate::categorization::domain::suggestions::{
    reconcile_new, ClusterPlan, NewCategoryProposal, SuggestionKind, SuggestionStatus, TaxonomySuggestion, MAX_NAMED_PER_RUN,
};
use crate::categorization::ports::{ActivityText, LlmSpec, Suggester, UsageMeter};
use crate::db::models::{NewTaxonomySuggestionRow, TaxonomySuggestionRow};
use crate::db::taxonomy_suggestions::TaxonomySuggestionsRepo;
use crate::db::Db;
use crate::domain::types::Category;
use crate::reporting::suggestions::SuggestionsReportRepo;
use crate::reporting::Window;
use crate::shared::ids::{ActivityId, CategoryId};
use crate::shared::time;
use crate::tracking::domain::source_key::url_location;

use super::embeddings::decode;

/// How many members stand for a group when the model names it.
const NAMING_SAMPLES: usize = 5;
/// How many titles a suggestion shows.
const SHOWN_TITLES: usize = 3;

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
    fn dismiss(&self, id: i64) -> Result<()>;
    /// How many still-unfiled activities the model found no category for.
    fn count_unlabeled(&self) -> Result<usize>;

    /// The still-unfiled activities the model found no category for, with an embedding from
    /// `embedding_model`.
    fn get_unlabeled(&self, embedding_model: &str) -> Result<Vec<UnlabeledActivity>>;
    /// The pending and the dismissed suggestions, for generation to match against.
    fn get_for_generation(&self) -> Result<Vec<(i64, SuggestionStatus, TaxonomySuggestion)>>;
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
            conn.set_status(id, SuggestionStatus::Pending.as_str(), SuggestionStatus::Accepted.as_str())?;
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

    fn dismiss(&self, id: i64) -> Result<()> {
        self.db.write().set_status(
            id,
            SuggestionStatus::Pending.as_str(),
            SuggestionStatus::Dismissed.as_str(),
        )?;
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
                    text: ActivityText {
                        app_name: row.app_name,
                        url: row.url.as_deref().and_then(url_location),
                        title: row.title,
                        source: SourceRef {
                            kind: row.source_kind,
                            key: row.source_key,
                            path: row.path,
                        },
                    },
                })
            })
            .collect()
    }

    fn get_for_generation(&self) -> Result<Vec<(i64, SuggestionStatus, TaxonomySuggestion)>> {
        let mut conn = self.db.read()?;
        let rows = conn.get_list(&[SuggestionStatus::Pending.as_str(), SuggestionStatus::Dismissed.as_str()])?;
        Ok(rows
            .iter()
            .filter_map(|row| decode_row(row).map(|(status, suggestion)| (row.id, status, suggestion)))
            .collect())
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

/// Clusters the activities the model found no category for and decides which groups to
/// propose, naming the new ones with `llm`. A group the model cannot name is still proposed,
/// unnamed. `cancelled` is checked before each request.
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
    let unlabeled = suggestions.get_unlabeled(embedding_model)?;
    let points: Vec<Point> = unlabeled.iter().map(|activity| activity.point.clone()).collect();
    let clusters = cluster(&points, &ClusterThresholds::default());
    if clusters.is_empty() {
        return Ok(Vec::new());
    }
    let texts: HashMap<ActivityId, &ActivityText> =
        unlabeled.iter().map(|activity| (activity.point.id, &activity.text)).collect();

    let existing = suggestions.get_for_generation()?;
    let mut pending: Vec<(i64, &NewCategoryProposal)> = Vec::new();
    let mut dismissed: Vec<&NewCategoryProposal> = Vec::new();
    for (id, status, suggestion) in &existing {
        if let TaxonomySuggestion::New(proposal) = suggestion {
            match status {
                SuggestionStatus::Pending => pending.push((*id, proposal)),
                SuggestionStatus::Dismissed => dismissed.push(proposal),
                SuggestionStatus::Accepted => {}
            }
        }
    }
    let pending_by_id: HashMap<i64, &NewCategoryProposal> = pending.iter().copied().collect();

    let mut writes = Vec::new();
    let mut named = 0;
    for plan in reconcile_new(&clusters, &pending, &dismissed) {
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
        existing: Vec<(i64, SuggestionStatus, TaxonomySuggestion)>,
    }

    impl TaxonomySuggestionSvc for Store {
        fn list(&self) -> Result<Vec<SuggestionView>> {
            unimplemented!()
        }
        fn accept_new(&self, _: i64) -> Result<Vec<ActivityId>> {
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
        fn get_for_generation(&self) -> Result<Vec<(i64, SuggestionStatus, TaxonomySuggestion)>> {
            Ok(self.existing.clone())
        }
        fn save(&self, _: &[SuggestionWrite], _: Option<i64>) -> Result<usize> {
            unimplemented!()
        }
    }

    /// Names every group "Games", and keeps the titles it was shown.
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
        generate(store, namer, &llm(), "e", &[], &UsageMeter::default(), &|| false).unwrap()
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
            existing: vec![(9, SuggestionStatus::Pending, TaxonomySuggestion::New(earlier))],
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
}
