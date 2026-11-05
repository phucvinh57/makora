use makora_lib::categorization::domain::rules::{PatternField, RuleKind};

pub const RULE_KINDS: [RuleKind; 4] = [
    RuleKind::App,
    RuleKind::Site,
    RuleKind::Page,
    RuleKind::Pattern,
];
pub const PATTERN_FIELDS: [PatternField; 3] =
    [PatternField::Url, PatternField::Title, PatternField::App];

pub fn parse_rule_kind(value: &str) -> Option<RuleKind> {
    RULE_KINDS.into_iter().find(|kind| kind.as_str() == value)
}

pub fn parse_pattern_field(value: &str) -> Option<PatternField> {
    PATTERN_FIELDS
        .into_iter()
        .find(|field| field.as_str() == value)
}

pub fn rule_kind_label(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::App => "App",
        RuleKind::Site => "Website",
        RuleKind::Page => "Page",
        RuleKind::Pattern => "Pattern",
    }
}

pub fn pattern_field_label(field: PatternField) -> &'static str {
    match field {
        PatternField::Url => "URL",
        PatternField::Title => "Window title",
        PatternField::App => "App or site name",
    }
}

pub fn rule_value_placeholder(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::App => "code",
        RuleKind::Site => "github.com",
        RuleKind::Page => "github.com/your-org",
        RuleKind::Pattern => "*standup*",
    }
}

/// How a rule reads in a list: `Window title matches *standup*`, `Website github.com`.
pub fn describe_rule(kind: RuleKind, field: Option<PatternField>, value: &str) -> String {
    match kind {
        RuleKind::Pattern => format!(
            "{} matches {value}",
            pattern_field_label(field.unwrap_or(PatternField::Url))
        ),
        _ => format!("{} {value}", rule_kind_label(kind)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wire_names() {
        assert_eq!(parse_rule_kind("site"), Some(RuleKind::Site));
        assert_eq!(parse_rule_kind("pattern"), Some(RuleKind::Pattern));
        assert_eq!(parse_rule_kind("Site"), None);
        assert_eq!(parse_pattern_field("title"), Some(PatternField::Title));
        assert_eq!(parse_pattern_field("body"), None);
        for kind in RULE_KINDS {
            assert_eq!(parse_rule_kind(kind.as_str()), Some(kind));
        }
        for field in PATTERN_FIELDS {
            assert_eq!(parse_pattern_field(field.as_str()), Some(field));
        }
    }

    #[test]
    fn labels() {
        let kinds: Vec<&str> = RULE_KINDS.into_iter().map(rule_kind_label).collect();
        assert_eq!(kinds, ["App", "Website", "Page", "Pattern"]);
        let fields: Vec<&str> = PATTERN_FIELDS
            .into_iter()
            .map(pattern_field_label)
            .collect();
        assert_eq!(fields, ["URL", "Window title", "App or site name"]);
        let placeholders: Vec<&str> = RULE_KINDS.into_iter().map(rule_value_placeholder).collect();
        assert_eq!(
            placeholders,
            ["code", "github.com", "github.com/your-org", "*standup*"]
        );
    }

    #[test]
    fn describes_rules() {
        assert_eq!(
            describe_rule(RuleKind::Site, None, "github.com"),
            "Website github.com"
        );
        assert_eq!(
            describe_rule(RuleKind::App, Some(PatternField::Title), "code"),
            "App code"
        );
        assert_eq!(
            describe_rule(RuleKind::Pattern, Some(PatternField::Title), "*standup*"),
            "Window title matches *standup*"
        );
        assert_eq!(
            describe_rule(RuleKind::Pattern, None, "*jira*"),
            "URL matches *jira*"
        );
    }
}
