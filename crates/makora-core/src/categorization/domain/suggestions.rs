//! Taxonomy suggestions: changes to the categories proposed from the data, which the user accepts
//! or dismisses. Each kind keeps what it needs as a JSON payload in `taxonomy_suggestions`.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::categorization::domain::clustering::Cluster;
use crate::shared::ids::{ActivityId, CategoryId};

/// How much a fresh cluster must share with an earlier suggestion to count as the same group.
pub const SAME_GROUP_OVERLAP: f64 = 0.5;
/// At most this many clusters are named per generation, to bound what it costs.
pub const MAX_NAMED_PER_RUN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    New,
    Describe,
    Merge,
}

impl SuggestionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Describe => "describe",
            Self::Merge => "merge",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "new" => Some(Self::New),
            "describe" => Some(Self::Describe),
            "merge" => Some(Self::Merge),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Dismissed,
}

impl SuggestionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(Self::Pending),
            "accepted" => Some(Self::Accepted),
            "dismissed" => Some(Self::Dismissed),
            _ => None,
        }
    }
}

/// A group of activities the model found no category for, proposed as a new category. `name` is
/// `None` when the model could not name it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewCategoryProposal {
    pub name: Option<String>,
    pub description: Option<String>,
    pub parent_id: Option<CategoryId>,
    pub members: Vec<ActivityId>,
    pub seconds: f64,
    pub sample_titles: Vec<String>,
}

/// A sharper description for a category the model keeps getting wrong.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DescribeProposal {
    pub category_id: CategoryId,
    pub current: Option<String>,
    pub proposed: String,
    pub corrected: u32,
    pub reviewed: u32,
}

/// Two categories that look alike and that the user moves activities between, to fold `from`
/// into `into`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MergeProposal {
    pub from: CategoryId,
    pub into: CategoryId,
    pub similarity: f32,
    /// Corrections from `from` to `into` and back.
    pub moved_out: u32,
    pub moved_in: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TaxonomySuggestion {
    New(NewCategoryProposal),
    Describe(DescribeProposal),
    Merge(MergeProposal),
}

impl TaxonomySuggestion {
    pub fn kind(&self) -> SuggestionKind {
        match self {
            Self::New(_) => SuggestionKind::New,
            Self::Describe(_) => SuggestionKind::Describe,
            Self::Merge(_) => SuggestionKind::Merge,
        }
    }

    pub fn payload(&self) -> serde_json::Result<String> {
        match self {
            Self::New(proposal) => serde_json::to_string(proposal),
            Self::Describe(proposal) => serde_json::to_string(proposal),
            Self::Merge(proposal) => serde_json::to_string(proposal),
        }
    }

    pub fn decode(kind: SuggestionKind, payload: &str) -> serde_json::Result<Self> {
        Ok(match kind {
            SuggestionKind::New => Self::New(serde_json::from_str(payload)?),
            SuggestionKind::Describe => Self::Describe(serde_json::from_str(payload)?),
            SuggestionKind::Merge => Self::Merge(serde_json::from_str(payload)?),
        })
    }
}

/// The share of the smaller of `a` and `b` that is in both; 0 when either is empty.
pub fn overlap(a: &[ActivityId], b: &[ActivityId]) -> f64 {
    let smaller = a.len().min(b.len());
    if smaller == 0 {
        return 0.;
    }
    let a: HashSet<&ActivityId> = a.iter().collect();
    let shared = b.iter().filter(|id| a.contains(id)).count();
    shared as f64 / smaller as f64
}

/// What to do with one fresh cluster.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClusterPlan {
    /// A new suggestion, named first.
    Insert { cluster: usize },
    /// The pending suggestion `id` is the same group: its members change. It is named again only
    /// when it has no name yet.
    Update { cluster: usize, id: i64, name: bool },
}

impl ClusterPlan {
    pub fn cluster(&self) -> usize {
        match self {
            Self::Insert { cluster } | Self::Update { cluster, .. } => *cluster,
        }
    }

    pub fn needs_name(&self) -> bool {
        match self {
            Self::Insert { .. } => true,
            Self::Update { name, .. } => *name,
        }
    }
}

/// Matches fresh clusters against the earlier "new category" suggestions. A cluster that is
/// mostly a dismissed suggestion is dropped, so a dismissed group stays dismissed; one that is
/// mostly a pending suggestion updates it, keeping its name; the rest are new.
pub fn reconcile_new(
    clusters: &[Cluster],
    pending: &[(i64, &NewCategoryProposal)],
    dismissed: &[&NewCategoryProposal],
) -> Vec<ClusterPlan> {
    let mut taken: HashSet<i64> = HashSet::new();
    let mut plans = Vec::new();
    for (position, cluster) in clusters.iter().enumerate() {
        if dismissed
            .iter()
            .any(|proposal| overlap(&cluster.members, &proposal.members) >= SAME_GROUP_OVERLAP)
        {
            continue;
        }
        let same = pending
            .iter()
            .filter(|(id, _)| !taken.contains(id))
            .map(|(id, proposal)| (*id, *proposal, overlap(&cluster.members, &proposal.members)))
            .filter(|(_, _, shared)| *shared >= SAME_GROUP_OVERLAP)
            .max_by(|a, b| a.2.total_cmp(&b.2).then(b.0.cmp(&a.0)));
        match same {
            Some((id, proposal, _)) => {
                taken.insert(id);
                plans.push(ClusterPlan::Update {
                    cluster: position,
                    id,
                    name: proposal.name.is_none(),
                });
            }
            None => plans.push(ClusterPlan::Insert { cluster: position }),
        }
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[i64]) -> Vec<ActivityId> {
        list.iter().copied().map(ActivityId).collect()
    }

    fn cluster(members: &[i64]) -> Cluster {
        Cluster {
            members: ids(members),
            seconds: 3600.,
            leader: ActivityId(members[0]),
        }
    }

    fn proposal(members: &[i64], name: Option<&str>) -> NewCategoryProposal {
        NewCategoryProposal {
            name: name.map(str::to_string),
            description: None,
            parent_id: None,
            members: ids(members),
            seconds: 0.,
            sample_titles: Vec::new(),
        }
    }

    #[test]
    fn overlap_is_measured_against_the_smaller_set() {
        assert_eq!(overlap(&ids(&[1, 2]), &ids(&[1, 2, 3, 4])), 1.0);
        assert_eq!(overlap(&ids(&[1, 2, 3, 4]), &ids(&[3, 4, 5, 6])), 0.5);
        assert_eq!(overlap(&ids(&[]), &ids(&[1])), 0.0);
    }

    #[test]
    fn a_dismissed_group_is_not_proposed_again() {
        let dismissed = proposal(&[1, 2, 3, 4], Some("Games"));
        let plans = reconcile_new(&[cluster(&[1, 2, 3, 9, 10])], &[], &[&dismissed]);
        assert!(plans.is_empty());
    }

    #[test]
    fn a_pending_group_is_updated_and_keeps_its_name() {
        let named = proposal(&[1, 2, 3, 4], Some("Games"));
        let unnamed = proposal(&[20, 21, 22], None);
        let plans = reconcile_new(
            &[cluster(&[1, 2, 3, 5]), cluster(&[20, 21, 23]), cluster(&[40, 41])],
            &[(7, &named), (8, &unnamed)],
            &[],
        );
        assert_eq!(
            plans,
            [
                ClusterPlan::Update { cluster: 0, id: 7, name: false },
                ClusterPlan::Update { cluster: 1, id: 8, name: true },
                ClusterPlan::Insert { cluster: 2 },
            ]
        );
    }

    #[test]
    fn one_pending_suggestion_is_updated_by_one_cluster_only() {
        let pending = proposal(&[1, 2, 3, 4], Some("Games"));
        let plans = reconcile_new(&[cluster(&[1, 2]), cluster(&[3, 4])], &[(7, &pending)], &[]);
        assert_eq!(plans[0], ClusterPlan::Update { cluster: 0, id: 7, name: false });
        assert_eq!(plans[1], ClusterPlan::Insert { cluster: 1 });
    }

    #[test]
    fn payloads_round_trip() {
        let suggestion = TaxonomySuggestion::New(proposal(&[1, 2], Some("Games")));
        let payload = suggestion.payload().unwrap();
        assert_eq!(TaxonomySuggestion::decode(SuggestionKind::New, &payload).unwrap(), suggestion);
    }
}
