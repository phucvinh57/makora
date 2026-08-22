//! Deterministic, thresholded average-link clustering for activity documents.
//!
//! This module deliberately has no database or classifier dependencies.  The caller owns the
//! distinction between geometry (centered vectors) and classification (the uncentered centroid).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub struct ClusterInput<'a> {
    pub id: &'a str,
    pub vector: &'a [f32],
    pub seconds: f64,
    pub text: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub struct ClusterParams {
    pub sigma_multiplier: f32,
    pub min_similarity: f32,
    pub max_similarity: f32,
    pub min_members: usize,
    pub max_members: usize,
    pub max_clusters: usize,
    pub max_inputs: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    /// Indices into the original `inputs` slice. They are ordered by canonical input id.
    pub members: Vec<usize>,
    /// Normalized centroid of the original, uncentered vectors.
    pub centroid: Vec<f32>,
    pub cohesion: f32,
    pub seconds: f64,
}

#[derive(Debug)]
struct WorkCluster {
    members: Vec<usize>,
    sum: Vec<f32>,
    seconds: f64,
}

fn pair_index(left: usize, right: usize, count: usize) -> usize {
    let (left, right) = if left < right {
        (left, right)
    } else {
        (right, left)
    };
    left * (2 * count - left - 1) / 2 + right - left - 1
}

fn better_similarity(left: (usize, f32), right: (usize, f32)) -> bool {
    left.1.total_cmp(&right.1).is_gt() || (left.1.total_cmp(&right.1).is_eq() && left.0 < right.0)
}

fn choose_best(
    row: usize,
    active: &[bool],
    clusters: &[Option<WorkCluster>],
    similarities: &[f32],
    count: usize,
    max_members: usize,
) -> Option<(usize, f32)> {
    let row_size = clusters[row].as_ref()?.members.len();
    let mut best = None;
    for (other, is_active) in active.iter().copied().enumerate() {
        if !is_active || other == row {
            continue;
        }
        let other_size = clusters[other].as_ref()?.members.len();
        if row_size.saturating_add(other_size) > max_members {
            continue;
        }
        let candidate = (other, similarities[pair_index(row, other, count)]);
        if best.is_none_or(|current| better_similarity(candidate, current)) {
            best = Some(candidate);
        }
    }
    best
}

fn normalize(vector: &mut [f32]) {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > f32::EPSILON {
        for value in vector {
            *value /= norm;
        }
    }
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn compare_pair(left: (usize, usize, f32), right: (usize, usize, f32)) -> Ordering {
    left.2
        .total_cmp(&right.2)
        .then_with(|| right.0.cmp(&left.0))
        .then_with(|| right.1.cmp(&left.1))
}

/// Performs deterministic average-link clustering with a threshold derived from this pool.
pub fn cluster(inputs: &[ClusterInput<'_>], params: ClusterParams) -> Vec<Cluster> {
    if inputs.is_empty()
        || params.max_inputs == 0
        || params.min_members == 0
        || params.max_members < params.min_members
        || params.max_clusters == 0
    {
        return Vec::new();
    }

    // A cap covers a minority of titles but the majority of tracked time. This keeps the
    // condensed matrix bounded while retaining the activity that matters most to the user.
    let mut selected = (0..inputs.len()).collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        inputs[*right]
            .seconds
            .total_cmp(&inputs[*left].seconds)
            .then_with(|| inputs[*left].id.cmp(inputs[*right].id))
    });
    selected.truncate(params.max_inputs);
    selected.sort_by(|left, right| inputs[*left].id.cmp(inputs[*right].id));
    let count = selected.len();
    if count < params.min_members {
        return Vec::new();
    }

    let dimension = selected
        .iter()
        .map(|index| inputs[*index].vector.len())
        .min()
        .unwrap_or_default();
    if dimension == 0 {
        return Vec::new();
    }

    let original = selected
        .iter()
        .map(|index| {
            let mut vector = inputs[*index].vector[..dimension].to_vec();
            normalize(&mut vector);
            vector
        })
        .collect::<Vec<_>>();
    let mut pool_centroid = vec![0.0; dimension];
    for vector in &original {
        for (target, value) in pool_centroid.iter_mut().zip(vector) {
            *target += *value;
        }
    }
    for value in &mut pool_centroid {
        *value /= count as f32;
    }
    let centered = original
        .iter()
        .map(|vector| {
            let mut centered = vector
                .iter()
                .zip(&pool_centroid)
                .map(|(value, mean)| value - mean)
                .collect::<Vec<_>>();
            normalize(&mut centered);
            centered
        })
        .collect::<Vec<_>>();

    let condensed_len = count.saturating_mul(count.saturating_sub(1)) / 2;
    let mut similarities = vec![0.0; condensed_len];
    for left in 0..count {
        for right in (left + 1)..count {
            let left_is_zero = centered[left]
                .iter()
                .all(|value| value.abs() <= f32::EPSILON);
            let right_is_zero = centered[right]
                .iter()
                .all(|value| value.abs() <= f32::EPSILON);
            similarities[pair_index(left, right, count)] = if left_is_zero && right_is_zero {
                // A pool of exact duplicates has no residual direction after centering, but
                // those documents are still the safest possible merge.
                1.0
            } else {
                dot(&centered[left], &centered[right]).clamp(-1.0, 1.0)
            };
        }
    }
    let samples = similarities
        .iter()
        .step_by((similarities.len() / 4096).max(1))
        .copied()
        .collect::<Vec<_>>();
    if samples.is_empty() {
        return Vec::new();
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    let variance = samples
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f32>()
        / samples.len() as f32;
    let upper = params.max_similarity.min(1.0).max(params.min_similarity);
    let threshold =
        (mean + params.sigma_multiplier * variance.sqrt()).clamp(params.min_similarity, upper);

    let mut clusters = (0..count)
        .map(|index| {
            Some(WorkCluster {
                members: vec![index],
                sum: original[index].clone(),
                seconds: inputs[selected[index]].seconds.max(0.0),
            })
        })
        .collect::<Vec<_>>();
    let mut active = vec![true; count];
    let mut best = (0..count)
        .map(|row| {
            choose_best(
                row,
                &active,
                &clusters,
                &similarities,
                count,
                params.max_members,
            )
        })
        .collect::<Vec<_>>();

    loop {
        let mut pair = None;
        for left in 0..count {
            if !active[left] {
                continue;
            }
            let Some((right, similarity)) = best[left] else {
                continue;
            };
            let candidate = (left.min(right), left.max(right), similarity);
            if pair.is_none_or(|current| compare_pair(candidate, current).is_gt()) {
                pair = Some(candidate);
            }
        }
        let Some((left, right, similarity)) = pair else {
            break;
        };
        if similarity < threshold {
            break;
        }

        let left_size = clusters[left].as_ref().unwrap().members.len();
        let right_size = clusters[right].as_ref().unwrap().members.len();
        let total_size = left_size + right_size;
        let right_cluster = clusters[right].take().unwrap();
        let left_cluster = clusters[left].as_mut().unwrap();
        left_cluster.members.extend(right_cluster.members);
        left_cluster
            .sum
            .iter_mut()
            .zip(right_cluster.sum)
            .for_each(|(a, b)| *a += b);
        left_cluster.seconds += right_cluster.seconds;
        active[right] = false;

        for other in 0..count {
            if !active[other] || other == left {
                continue;
            }
            let left_similarity = similarities[pair_index(left, other, count)];
            let right_similarity = similarities[pair_index(right, other, count)];
            similarities[pair_index(left, other, count)] = ((left_size as f32 * left_similarity)
                + (right_size as f32 * right_similarity))
                / total_size as f32;
        }

        best[right] = None;
        for row in 0..count {
            if !active[row] {
                best[row] = None;
                continue;
            }
            if row == left
                || best[row].is_none_or(|(partner, _)| partner == left || partner == right)
            {
                best[row] = choose_best(
                    row,
                    &active,
                    &clusters,
                    &similarities,
                    count,
                    params.max_members,
                );
                continue;
            }
            let candidate = (left, similarities[pair_index(row, left, count)]);
            if best[row].is_none_or(|current| better_similarity(candidate, current)) {
                best[row] = Some(candidate);
            }
        }
    }

    let mut output = clusters
        .into_iter()
        .enumerate()
        .filter_map(|(canonical_index, cluster)| {
            let cluster = cluster?;
            if cluster.members.len() < params.min_members {
                return None;
            }
            let mut centroid = cluster.sum.clone();
            normalize(&mut centroid);
            let sum_norm_squared = dot(&cluster.sum, &cluster.sum);
            let member_count = cluster.members.len() as f32;
            let cohesion = if cluster.members.len() > 1 {
                ((sum_norm_squared - member_count) / (member_count * (member_count - 1.0)))
                    .clamp(-1.0, 1.0)
            } else {
                0.0
            };
            let mut members = cluster
                .members
                .iter()
                .map(|position| selected[*position])
                .collect::<Vec<_>>();
            members.sort_by(|left, right| inputs[*left].id.cmp(inputs[*right].id));
            Some((
                Cluster {
                    members,
                    centroid,
                    cohesion,
                    seconds: cluster.seconds,
                },
                canonical_index,
            ))
        })
        .collect::<Vec<_>>();
    output.sort_by(|(left, left_index), (right, right_index)| {
        right
            .seconds
            .total_cmp(&left.seconds)
            .then_with(|| left_index.cmp(right_index))
    });
    output
        .into_iter()
        .take(params.max_clusters)
        .map(|(cluster, _)| cluster)
        .collect()
}

const STOPWORDS: &[&str] = &[
    "about", "after", "again", "against", "also", "and", "are", "because", "been", "before",
    "being", "between", "but", "can", "could", "from", "have", "into", "just", "more", "most",
    "not", "only", "other", "our", "out", "over", "same", "should", "some", "such", "than", "that",
    "their", "there", "these", "they", "this", "those", "through", "under", "very", "was", "were",
    "what", "when", "where", "which", "while", "with", "would", "your",
];

fn tokens(text: &str) -> HashSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .map(str::to_ascii_lowercase)
        .filter(|token| {
            token.len() >= 3 && !token.chars().all(|character| character.is_ascii_digit())
        })
        .filter(|token| STOPWORDS.binary_search(&token.as_str()).is_err())
        .collect()
}

fn humanize_key(key: &str) -> HashSet<String> {
    tokens(&key.replace(['.', '-', '_'], " "))
}

/// Produces a short, lift-weighted label for a cluster. `member_texts` should be ordered by
/// descending tracked time so the fallback is the most useful observed document.
pub fn label_cluster(
    member_texts: &[&str],
    pool_texts: &[&str],
    parent_key: Option<&str>,
) -> String {
    if member_texts.is_empty() {
        return "Activity group".into();
    }
    let parent_tokens = parent_key.map(humanize_key).unwrap_or_default();
    let member_sets = member_texts
        .iter()
        .map(|text| tokens(text))
        .collect::<Vec<_>>();
    let pool_sets = pool_texts
        .iter()
        .map(|text| tokens(text))
        .collect::<Vec<_>>();
    let mut member_df = HashMap::<String, usize>::new();
    let mut pool_df = HashMap::<String, usize>::new();
    for set in &member_sets {
        for token in set {
            *member_df.entry(token.clone()).or_default() += 1;
        }
    }
    for set in &pool_sets {
        for token in set {
            *pool_df.entry(token.clone()).or_default() += 1;
        }
    }
    let member_count = member_sets.len() as f32;
    let pool_count = pool_sets.len().max(1) as f32;
    let mut scored = member_df
        .into_iter()
        .filter(|(token, _)| !parent_tokens.contains(token))
        .map(|(token, count)| {
            let lift = count as f32 / member_count
                - pool_df.get(&token).copied().unwrap_or_default() as f32 / pool_count;
            (token, lift)
        })
        .filter(|(_, score)| *score > 0.0)
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });
    if !scored.is_empty() {
        return scored
            .into_iter()
            .take(3)
            .map(|(token, _)| token)
            .collect::<Vec<_>>()
            .join(" · ");
    }
    member_texts[0].chars().take(60).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ClusterParams {
        ClusterParams {
            sigma_multiplier: 1.0,
            min_similarity: 0.2,
            max_similarity: 1.0,
            min_members: 3,
            max_members: 200,
            max_clusters: 40,
            max_inputs: 1500,
        }
    }

    #[test]
    fn average_link_similarity_matches_mean_of_pairwise_dots() {
        let inputs = [
            ClusterInput {
                id: "a",
                vector: &[1.0, 0.0, 0.0],
                seconds: 1.0,
                text: "a",
            },
            ClusterInput {
                id: "b",
                vector: &[0.0, 1.0, 0.0],
                seconds: 1.0,
                text: "b",
            },
            ClusterInput {
                id: "c",
                vector: &[0.0, 0.0, 1.0],
                seconds: 1.0,
                text: "c",
            },
            ClusterInput {
                id: "d",
                vector: &[0.70710677, 0.70710677, 0.0],
                seconds: 1.0,
                text: "d",
            },
        ];
        let left = [0usize, 1];
        let right = [2usize, 3];
        let expected = left
            .iter()
            .flat_map(|left| {
                right
                    .iter()
                    .map(move |right| dot(inputs[*left].vector, inputs[*right].vector))
            })
            .sum::<f32>()
            / 4.0;
        let left_mean = [0.5, 0.5, 0.0];
        let right_mean = [0.35355338, 0.35355338, 0.5];
        assert!((dot(&left_mean, &right_mean) - expected).abs() < 1e-6);
    }

    #[test]
    fn labels_use_lift_and_drop_parent_boilerplate() {
        assert_eq!(
            label_cluster(
                &["youtube.com lecture rust", "youtube.com lecture async"],
                &["youtube.com lecture rust", "youtube.com game stream"],
                Some("youtube.com"),
            ),
            "async · lecture"
        );
    }

    #[test]
    fn cap_prefers_longest_documents() {
        let inputs = [
            ClusterInput {
                id: "short",
                vector: &[0.0, 1.0],
                seconds: 1.0,
                text: "short",
            },
            ClusterInput {
                id: "long",
                vector: &[1.0, 0.0],
                seconds: 3.0,
                text: "long",
            },
            ClusterInput {
                id: "middle",
                vector: &[1.0, 0.0],
                seconds: 2.0,
                text: "middle",
            },
            ClusterInput {
                id: "longest",
                vector: &[1.0, 0.0],
                seconds: 4.0,
                text: "longest",
            },
        ];
        let mut parameters = params();
        parameters.max_inputs = 3;
        parameters.min_similarity = -1.0;
        parameters.sigma_multiplier = -1.0;
        let results = cluster(&inputs, parameters);
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0]
                .members
                .iter()
                .map(|index| inputs[*index].id)
                .collect::<Vec<_>>(),
            vec!["long", "longest", "middle"]
        );
    }

    #[test]
    fn similar_documents_merge_and_singletons_are_dropped() {
        let inputs = [
            ClusterInput {
                id: "a",
                vector: &[1.0, 0.0],
                seconds: 1.0,
                text: "a",
            },
            ClusterInput {
                id: "b",
                vector: &[1.0, 0.0],
                seconds: 1.0,
                text: "b",
            },
            ClusterInput {
                id: "c",
                vector: &[1.0, 0.0],
                seconds: 1.0,
                text: "c",
            },
            ClusterInput {
                id: "d",
                vector: &[0.0, 1.0],
                seconds: 1.0,
                text: "d",
            },
            ClusterInput {
                id: "e",
                vector: &[0.0, 1.0],
                seconds: 1.0,
                text: "e",
            },
            ClusterInput {
                id: "f",
                vector: &[0.0, 1.0],
                seconds: 1.0,
                text: "f",
            },
        ];
        let results = cluster(&inputs, params());
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.members.len() == 3));
    }

    #[test]
    fn mean_centering_can_separate_a_shared_prefix() {
        let inputs = [
            ClusterInput {
                id: "a1",
                vector: &[0.97, 0.24, 0.0],
                seconds: 1.0,
                text: "a1",
            },
            ClusterInput {
                id: "a2",
                vector: &[0.97, 0.22, 0.03],
                seconds: 1.0,
                text: "a2",
            },
            ClusterInput {
                id: "a3",
                vector: &[0.97, 0.20, -0.02],
                seconds: 1.0,
                text: "a3",
            },
            ClusterInput {
                id: "b1",
                vector: &[0.97, 0.0, 0.24],
                seconds: 1.0,
                text: "b1",
            },
            ClusterInput {
                id: "b2",
                vector: &[0.97, 0.03, 0.22],
                seconds: 1.0,
                text: "b2",
            },
            ClusterInput {
                id: "b3",
                vector: &[0.97, -0.02, 0.20],
                seconds: 1.0,
                text: "b3",
            },
        ];
        let mut parameters = params();
        parameters.sigma_multiplier = 0.5;
        parameters.min_similarity = 0.2;
        let results = cluster(&inputs, parameters);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.members.len() == 3));
    }
}
