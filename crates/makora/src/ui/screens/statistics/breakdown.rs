//! The numbers the statistics cards show, worked out from the range's stats.

use std::collections::HashMap;

use makora_lib::domain::types::{Category, CategoryStat, DailyStat};
use makora_lib::shared::ids::CategoryId;

use crate::ui::format::categories::{
    CategoryNode, UNCATEGORIZED, UNCATEGORIZED_COLOR, UNCATEGORIZED_LABEL, build_category_tree,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    pub key: String,
    pub name: String,
    pub value: f64,
    pub color: String,
    pub depth: usize,
}

/// One row per category in tree order, each carrying its own time *plus* everything its
/// subcategories spent, so a parent only ever used through its children still shows where that
/// time went. Returns the rows and the time they add up to.
pub fn roll_up(
    nodes: &[CategoryNode],
    seconds_by_id: &HashMap<CategoryId, f64>,
) -> (Vec<Slice>, f64) {
    let mut rows = Vec::new();
    let mut total = 0.0;
    for node in nodes {
        let (nested, nested_total) = roll_up(&node.children, seconds_by_id);
        let value = seconds_by_id.get(&node.category.id).copied().unwrap_or(0.0) + nested_total;
        total += value;
        if value == 0.0 {
            continue;
        }
        rows.push(Slice {
            key: node.category.id.0.to_string(),
            name: node.category.name.clone(),
            value,
            color: node.category.color.clone(),
            depth: node.depth,
        });
        rows.extend(nested);
    }
    (rows, total)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CategoryBreakdown {
    /// Every category with time, for the legend.
    pub rows: Vec<Slice>,
    /// The roots only: the donut splits the range once, and a subcategory's time is already in
    /// its parent's slice.
    pub top_level: Vec<Slice>,
}

pub fn category_breakdown(stats: &[CategoryStat], categories: &[Category]) -> CategoryBreakdown {
    let seconds_by_id: HashMap<CategoryId, f64> = stats
        .iter()
        .filter_map(|c| c.category_id.map(|id| (id, c.total_seconds)))
        .collect();
    let (rows, _) = roll_up(&build_category_tree(categories), &seconds_by_id);
    let untracked = stats
        .iter()
        .find(|c| c.category_id.is_none())
        .map(|c| Slice {
            key: UNCATEGORIZED.into(),
            name: UNCATEGORIZED_LABEL.into(),
            value: c.total_seconds,
            color: UNCATEGORIZED_COLOR.into(),
            depth: 0,
        });
    let top_level = rows
        .iter()
        .filter(|row| row.depth == 0)
        .cloned()
        .chain(untracked.clone())
        .collect();
    CategoryBreakdown {
        rows: rows.into_iter().chain(untracked).collect(),
        top_level,
    }
}

/// The first day with the most tracked time.
pub fn busiest_day(daily: &[DailyStat]) -> Option<&DailyStat> {
    daily
        .iter()
        .fold(None, |max: Option<&DailyStat>, day| match max {
            Some(max) if day.total_seconds <= max.total_seconds => Some(max),
            _ => Some(day),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat(id: i64, name: &str, parent: Option<i64>) -> Category {
        Category {
            id: CategoryId(id),
            name: name.into(),
            color: format!("#00000{id}"),
            parent_id: parent.map(CategoryId),
            description: None,
        }
    }

    fn stat(id: Option<i64>, seconds: f64) -> CategoryStat {
        CategoryStat {
            category_id: id.map(CategoryId),
            name: None,
            color: None,
            total_seconds: seconds,
            session_count: 1,
        }
    }

    fn day(day: &str, seconds: f64) -> DailyStat {
        DailyStat {
            day: day.into(),
            total_seconds: seconds,
        }
    }

    fn sample() -> Vec<Category> {
        vec![
            cat(1, "Work", None),
            cat(2, "Coding", Some(1)),
            cat(3, "Rust", Some(2)),
            cat(4, "Meetings", Some(1)),
            cat(5, "Fun", None),
            cat(6, "Idle", None),
        ]
    }

    fn rows(slices: &[Slice]) -> Vec<(&str, f64, usize)> {
        slices
            .iter()
            .map(|s| (s.key.as_str(), s.value, s.depth))
            .collect()
    }

    #[test]
    fn parents_add_up_their_subcategories() {
        let seconds = HashMap::from([
            (CategoryId(1), 10.0),
            (CategoryId(3), 30.0),
            (CategoryId(4), 5.0),
            (CategoryId(5), 7.0),
        ]);
        let (slices, total) = roll_up(&build_category_tree(&sample()), &seconds);
        assert_eq!(
            rows(&slices),
            vec![
                ("1", 45.0, 0),
                ("2", 30.0, 1),
                ("3", 30.0, 2),
                ("4", 5.0, 1),
                ("5", 7.0, 0)
            ]
        );
        assert_eq!(total, 52.0);
    }

    #[test]
    fn a_parent_used_only_through_children_still_shows() {
        let seconds = HashMap::from([(CategoryId(3), 12.0)]);
        let (slices, total) = roll_up(&build_category_tree(&sample()), &seconds);
        assert_eq!(
            rows(&slices),
            vec![("1", 12.0, 0), ("2", 12.0, 1), ("3", 12.0, 2)]
        );
        assert_eq!(total, 12.0);
        assert_eq!(slices[0].name, "Work");
        assert_eq!(slices[0].color, "#000001");
    }

    #[test]
    fn breakdown_puts_uncategorized_last_and_only_roots_in_the_donut() {
        let stats = vec![stat(Some(3), 20.0), stat(Some(5), 4.0), stat(None, 9.0)];
        let breakdown = category_breakdown(&stats, &sample());
        assert_eq!(
            rows(&breakdown.rows),
            vec![
                ("1", 20.0, 0),
                ("2", 20.0, 1),
                ("3", 20.0, 2),
                ("5", 4.0, 0),
                (UNCATEGORIZED, 9.0, 0)
            ]
        );
        assert_eq!(
            rows(&breakdown.top_level),
            vec![("1", 20.0, 0), ("5", 4.0, 0), (UNCATEGORIZED, 9.0, 0)]
        );
        let uncategorized = breakdown.top_level.last().unwrap();
        assert_eq!(uncategorized.name, UNCATEGORIZED_LABEL);
        assert_eq!(uncategorized.color, UNCATEGORIZED_COLOR);
    }

    #[test]
    fn stats_for_unknown_categories_are_left_out() {
        let breakdown = category_breakdown(&[stat(Some(99), 5.0)], &sample());
        assert_eq!(breakdown, CategoryBreakdown::default());
    }

    #[test]
    fn busiest_day_keeps_the_first_of_a_tie() {
        let days = vec![
            day("2026-11-01", 5.0),
            day("2026-11-02", 9.0),
            day("2026-11-03", 9.0),
        ];
        assert_eq!(busiest_day(&days).unwrap().day, "2026-11-02");
        assert!(busiest_day(&[]).is_none());
    }
}
