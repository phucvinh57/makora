//! Grouping the activities the model found no category for, so a group large enough can be
//! proposed as a new category. Greedy leader clustering over their embeddings: cheap, and the
//! same input always gives the same groups.

use std::collections::HashMap;

use crate::categorization::domain::retrieval::similarity;
use crate::categorization::ports::Embedding;
use crate::shared::ids::ActivityId;

#[derive(Debug, Clone)]
pub struct Point {
    pub id: ActivityId,
    pub vector: Embedding,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Ascending.
    pub members: Vec<ActivityId>,
    pub seconds: f64,
    /// The member the group formed around.
    pub leader: ActivityId,
}

#[derive(Debug, Clone, Copy)]
pub struct ClusterThresholds {
    /// How similar to a group's leader an activity must be to join it.
    pub join: f32,
    /// How similar two leaders must be for their groups to become one.
    pub merge: f32,
    pub min_members: usize,
    pub min_seconds: f64,
}

impl Default for ClusterThresholds {
    fn default() -> Self {
        Self {
            join: 0.80,
            merge: 0.85,
            min_members: 5,
            min_seconds: 30. * 60.,
        }
    }
}

/// The groups of `points` that pass `thresholds`, the most time first.
///
/// Points are taken in id order whatever order they come in. Each joins the leader it is most
/// similar to (the earliest on a tie) when that is at least `join`, or else leads a group of its
/// own; then groups whose leaders are at least `merge` alike become one.
pub fn cluster(points: &[Point], thresholds: &ClusterThresholds) -> Vec<Cluster> {
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by_key(|&position| points[position].id);

    let mut leaders: Vec<usize> = Vec::new();
    let mut group_of: Vec<usize> = vec![0; points.len()];
    for &position in &order {
        let vector = &points[position].vector;
        let mut best: Option<(usize, f32)> = None;
        for (group, &leader) in leaders.iter().enumerate() {
            let score = similarity(vector, &points[leader].vector);
            if score >= thresholds.join && best.is_none_or(|(_, top)| score > top) {
                best = Some((group, score));
            }
        }
        group_of[position] = match best {
            Some((group, _)) => group,
            None => {
                leaders.push(position);
                leaders.len() - 1
            }
        };
    }

    let mut root: Vec<usize> = (0..leaders.len()).collect();
    for later in 0..leaders.len() {
        for earlier in 0..later {
            if similarity(&points[leaders[earlier]].vector, &points[leaders[later]].vector) >= thresholds.merge {
                let (a, b) = (find(&mut root, earlier), find(&mut root, later));
                if a != b {
                    root[a.max(b)] = a.min(b);
                }
            }
        }
    }

    let mut groups: HashMap<usize, Cluster> = HashMap::new();
    for &position in &order {
        let group = find(&mut root, group_of[position]);
        let point = &points[position];
        let cluster = groups.entry(group).or_insert_with(|| Cluster {
            members: Vec::new(),
            seconds: 0.,
            leader: points[leaders[group]].id,
        });
        cluster.members.push(point.id);
        cluster.seconds += point.seconds;
    }

    let mut clusters: Vec<Cluster> = groups
        .into_values()
        .filter(|cluster| cluster.members.len() >= thresholds.min_members && cluster.seconds >= thresholds.min_seconds)
        .collect();
    clusters.sort_by(|a, b| b.seconds.total_cmp(&a.seconds).then(a.leader.cmp(&b.leader)));
    clusters
}

/// Up to `n` members of `cluster`, those most like its leader first: what stands for the group
/// when it is named or shown.
pub fn samples(cluster: &Cluster, points: &[Point], n: usize) -> Vec<ActivityId> {
    let by_id: HashMap<ActivityId, &Point> = points.iter().map(|point| (point.id, point)).collect();
    let Some(leader) = by_id.get(&cluster.leader) else {
        return cluster.members.iter().copied().take(n).collect();
    };
    let mut scored: Vec<(f32, ActivityId)> = cluster
        .members
        .iter()
        .filter_map(|id| by_id.get(id).map(|point| (similarity(&leader.vector, &point.vector), *id)))
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(n).map(|(_, id)| id).collect()
}

fn find(root: &mut [usize], mut group: usize) -> usize {
    while root[group] != group {
        root[group] = root[root[group]];
        group = root[group];
    }
    group
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit vector at `degrees` in the plane: cos(angle between two) is their similarity.
    fn at(id: i64, degrees: f32, seconds: f64) -> Point {
        let radians = degrees.to_radians();
        Point {
            id: ActivityId(id),
            vector: vec![radians.cos(), radians.sin()],
            seconds,
        }
    }

    fn small() -> ClusterThresholds {
        ClusterThresholds {
            min_members: 2,
            min_seconds: 60.,
            ..ClusterThresholds::default()
        }
    }

    fn ids(cluster: &Cluster) -> Vec<i64> {
        cluster.members.iter().map(|id| id.0).collect()
    }

    #[test]
    fn close_points_group_and_far_ones_do_not() {
        // cos 30° ≈ 0.87 joins, cos 40° ≈ 0.77 does not.
        let points = [at(1, 0., 60.), at(2, 30., 60.), at(3, 70., 60.), at(4, 100., 60.)];
        let clusters = cluster(&points, &small());
        assert_eq!(clusters.iter().map(ids).collect::<Vec<_>>(), [vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn the_same_points_in_any_order_give_the_same_groups() {
        let points = vec![at(1, 0., 60.), at(2, 20., 90.), at(3, 35., 60.), at(4, 90., 60.), at(5, 95., 30.)];
        let mut reversed = points.clone();
        reversed.reverse();
        assert_eq!(cluster(&points, &small()), cluster(&reversed, &small()));
    }

    #[test]
    fn leaders_close_enough_merge_their_groups() {
        // 1 leads; 3 is 40° from 1 so leads its own group; 2 sits between and joins 1.
        let points = [at(1, 0., 60.), at(2, 20., 60.), at(3, 40., 60.), at(4, 60., 60.)];
        let apart = ClusterThresholds { merge: 0.95, ..small() };
        assert_eq!(cluster(&points, &apart).len(), 2);
        let together = ClusterThresholds { merge: 0.70, ..small() };
        let merged = cluster(&points, &together);
        assert_eq!(merged.iter().map(ids).collect::<Vec<_>>(), [vec![1, 2, 3, 4]]);
        assert_eq!(merged[0].leader, ActivityId(1));
    }

    #[test]
    fn small_or_short_groups_are_left_out_and_the_rest_ordered_by_time() {
        let points = [
            at(1, 0., 100.),
            at(2, 5., 100.),
            at(3, 90., 10.),
            at(4, 95., 10.),
            at(5, 180., 500.),
            at(6, 185., 500.),
            at(7, 270., 900.),
        ];
        let clusters = cluster(&points, &small());
        assert_eq!(clusters.iter().map(ids).collect::<Vec<_>>(), [vec![5, 6], vec![1, 2]]);
    }

    #[test]
    fn samples_are_the_members_most_like_the_leader() {
        let points = [at(1, 0., 60.), at(2, 30., 60.), at(3, 10., 60.), at(4, 20., 60.)];
        let clusters = cluster(&points, &small());
        let picked: Vec<i64> = samples(&clusters[0], &points, 3).iter().map(|id| id.0).collect();
        assert_eq!(picked, [1, 3, 4]);
    }
}
