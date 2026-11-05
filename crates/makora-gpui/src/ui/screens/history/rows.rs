//! What a history row says about itself: its category's hover text, its title, and the toast
//! after its category changes.

use makora_lib::domain::types::{CategorySource, SessionRow};
use makora_lib::shared::ids::CategoryId;

use crate::ui::format::categories::{UNCATEGORIZED, UNCATEGORIZED_LABEL};
use crate::ui::widgets::toast::ToastKind;

/// The row's category as the picker values it.
pub fn category_value(row: &SessionRow) -> String {
    row.category_id
        .map_or_else(|| UNCATEGORIZED.to_owned(), |id| id.0.to_string())
}

/// A picked value as a category, `None` for "Uncategorized"; `Err` for a value that is neither.
pub fn parse_category_value(value: &str) -> Result<Option<CategoryId>, ()> {
    if value == UNCATEGORIZED {
        return Ok(None);
    }
    value.parse().map(|id| Some(CategoryId(id))).map_err(|_| ())
}

/// The category's path and where the label came from.
pub fn category_title(row: &SessionRow, path: Option<&str>) -> String {
    let path = path.unwrap_or(UNCATEGORIZED_LABEL);
    if row.category_is_pinned {
        format!("{path} — pinned to this activity")
    } else {
        match row.category_source {
            Some(CategorySource::Rule) => format!("{path} — matched by a rule"),
            Some(CategorySource::Ai) => format!("{path} — suggested by AI"),
            _ => path.to_owned(),
        }
    }
}

fn non_empty(value: &str) -> Option<&str> {
    (!value.is_empty()).then_some(value)
}

/// The title, else the page address, else the app.
pub fn display_title(row: &SessionRow) -> &str {
    non_empty(&row.title)
        .or_else(|| row.url.as_deref().and_then(non_empty))
        .unwrap_or(&row.app_name)
}

/// The full title and the page address, one per line.
pub fn full_title(row: &SessionRow) -> String {
    let lines: Vec<&str> = [Some(row.title.as_str()), row.url.as_deref()]
        .into_iter()
        .flatten()
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        row.app_name.clone()
    } else {
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowToast {
    pub kind: ToastKind,
    pub message: String,
    pub description: Option<String>,
}

/// The toast after setting a row's category: `path` is the new category's path, `None` when it
/// was cleared. Clearing a rule's category leaves the rule in force, which the toast says.
pub fn row_category_toast(row: &SessionRow, path: Option<&str>) -> RowToast {
    match path {
        Some(path) => RowToast {
            kind: ToastKind::Success,
            message: format!("Set to {path}"),
            description: Some(display_title(row).to_owned()),
        },
        None if row.category_source == Some(CategorySource::Rule) => RowToast {
            kind: ToastKind::Info,
            message: "This activity matches a rule".into(),
            description: Some(
                "Pin another category to override it, or edit the rule on the Categories page."
                    .into(),
            ),
        },
        None => RowToast {
            kind: ToastKind::Success,
            message: if row.category_source == Some(CategorySource::Ai) {
                "Rejected the AI suggestion"
            } else {
                "Cleared this activity's category"
            }
            .into(),
            description: None,
        },
    }
}

pub fn bulk_toast(count: usize) -> String {
    format!("Categorized {count} activities")
}

/// Whether the system can be asked to open `url`: a web address with a host.
pub fn check_site_url(url: &str) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    match rest {
        Some(rest) if !rest.is_empty() && !rest.starts_with('/') => Ok(()),
        _ => Err(format!("Not a web address: {url}")),
    }
}

#[cfg(test)]
mod tests {
    use makora_lib::shared::ids::{ActivityId, SessionId};
    use makora_lib::tracking::domain::source_key::KeyKind;

    use super::*;

    fn row(source: Option<CategorySource>) -> SessionRow {
        SessionRow {
            id: SessionId(1),
            activity_id: ActivityId(2),
            app_name: "Firefox".into(),
            source_kind: KeyKind::Exe,
            source_key: "firefox".into(),
            title: String::new(),
            url: None,
            start_time: "2026-11-05T10:00:00.000Z".into(),
            end_time: None,
            category_id: source.map(|_| CategoryId(7)),
            category_name: None,
            category_color: None,
            category_is_pinned: false,
            category_source: source,
        }
    }

    #[test]
    fn values_round_trip_through_the_picker() {
        assert_eq!(category_value(&row(None)), UNCATEGORIZED);
        assert_eq!(category_value(&row(Some(CategorySource::Manual))), "7");
        assert_eq!(parse_category_value("7"), Ok(Some(CategoryId(7))));
        assert_eq!(parse_category_value(UNCATEGORIZED), Ok(None));
        assert!(parse_category_value("all").is_err());
    }

    #[test]
    fn the_title_says_where_the_label_came_from() {
        let mut pinned = row(Some(CategorySource::Manual));
        pinned.category_is_pinned = true;
        assert_eq!(
            category_title(&pinned, Some("Work / Code")),
            "Work / Code — pinned to this activity"
        );
        assert_eq!(
            category_title(&row(Some(CategorySource::Rule)), Some("Work")),
            "Work — matched by a rule"
        );
        assert_eq!(
            category_title(&row(Some(CategorySource::Ai)), Some("Work")),
            "Work — suggested by AI"
        );
        assert_eq!(category_title(&row(None), None), "Uncategorized");
    }

    #[test]
    fn the_title_falls_back_to_the_address_then_the_app() {
        let mut r = row(None);
        assert_eq!(display_title(&r), "Firefox");
        assert_eq!(full_title(&r), "Firefox");
        r.url = Some("https://github.com".into());
        assert_eq!(display_title(&r), "https://github.com");
        r.title = "Issues".into();
        assert_eq!(display_title(&r), "Issues");
        assert_eq!(full_title(&r), "Issues\nhttps://github.com");
    }

    #[test]
    fn clearing_a_category_depends_on_its_source() {
        let rule = row_category_toast(&row(Some(CategorySource::Rule)), None);
        assert_eq!(rule.kind, ToastKind::Info);
        assert_eq!(rule.message, "This activity matches a rule");
        assert!(rule.description.is_some());

        let ai = row_category_toast(&row(Some(CategorySource::Ai)), None);
        assert_eq!(
            (ai.kind, ai.message.as_str()),
            (ToastKind::Success, "Rejected the AI suggestion")
        );

        let manual = row_category_toast(&row(Some(CategorySource::Manual)), None);
        assert_eq!(manual.message, "Cleared this activity's category");
        assert_eq!(manual.description, None);
    }

    #[test]
    fn setting_a_category_names_it_and_the_activity() {
        let mut r = row(None);
        r.title = "Inbox".into();
        let toast = row_category_toast(&r, Some("Work / Mail"));
        assert_eq!(toast.message, "Set to Work / Mail");
        assert_eq!(toast.description.as_deref(), Some("Inbox"));
        assert_eq!(bulk_toast(3), "Categorized 3 activities");
    }

    #[test]
    fn only_web_addresses_open() {
        assert!(check_site_url("https://github.com/a").is_ok());
        assert!(check_site_url("HTTP://example.com").is_ok());
        assert!(check_site_url("https://").is_err());
        assert!(check_site_url("chrome://newtab").is_err());
        assert!(check_site_url("github.com").is_err());
    }
}
