use makora_lib::ai::privacy::{Kind, UrlPolicy};

pub fn redaction_kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Email => "Email addresses",
        Kind::Phone => "Phone numbers",
        Kind::Ip => "IP addresses",
        Kind::Card => "Card numbers",
        Kind::Iban => "Bank accounts (IBAN)",
        Kind::Secret => "Keys and tokens",
        Kind::Uuid => "UUIDs",
        Kind::LongNumber => "Numbers of 8+ digits",
    }
}

/// A kind as the network log names it (`email`, `term`, `never_send`…), for people.
pub fn masked_kind_label(name: &str) -> String {
    match name {
        "term" => "Words to hide".to_string(),
        "account" => "Account name".to_string(),
        "never_send" => "Never-send activities".to_string(),
        other => Kind::parse(other).map_or_else(
            || other.to_string(),
            |kind| redaction_kind_label(kind).to_string(),
        ),
    }
}

pub fn url_policy_label(policy: UrlPolicy) -> &'static str {
    match policy {
        UrlPolicy::Full => "Full path",
        UrlPolicy::StripIds => "Path without ids",
        UrlPolicy::DomainOnly => "Domain only",
    }
}

pub fn url_policy_example(policy: UrlPolicy) -> &'static str {
    match policy {
        UrlPolicy::Full => "github.com/acme/api/pull/42 is sent as it is.",
        UrlPolicy::StripIds => {
            "github.com/acme/api/pull/42 is sent as github.com/acme/api/pull/{id}."
        }
        UrlPolicy::DomainOnly => "github.com/acme/api/pull/42 is sent as github.com.",
    }
}

/// `Hides jane, Jane Doe.` for the account names that would be redacted.
pub fn account_names_hint(names: &[String]) -> String {
    if names.is_empty() {
        "No account name was found on this system.".to_string()
    } else {
        format!("Hides {}.", names.join(", "))
    }
}

/// `3 of 20 recent activities change.`
pub fn preview_summary(changed: usize, total: usize) -> String {
    match (changed, total) {
        (_, 0) => "No activity recorded yet.".to_string(),
        (0, _) => format!("None of the {total} most recent activities change."),
        _ => format!("{changed} of the {total} most recent activities change."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_and_policy_has_a_label() {
        assert!(
            Kind::ALL
                .iter()
                .all(|kind| !redaction_kind_label(*kind).is_empty())
        );
        assert!(
            UrlPolicy::ALL
                .iter()
                .all(|policy| !url_policy_label(*policy).is_empty())
        );
    }

    #[test]
    fn masked_kinds_read_well() {
        assert_eq!(masked_kind_label("email"), "Email addresses");
        assert_eq!(masked_kind_label("long_number"), "Numbers of 8+ digits");
        assert_eq!(masked_kind_label("term"), "Words to hide");
        assert_eq!(masked_kind_label("never_send"), "Never-send activities");
        assert_eq!(masked_kind_label("future_kind"), "future_kind");
    }

    #[test]
    fn hints_read_well() {
        assert_eq!(
            account_names_hint(&["jane".into(), "Jane Doe".into()]),
            "Hides jane, Jane Doe."
        );
        assert_eq!(
            preview_summary(3, 20),
            "3 of the 20 most recent activities change."
        );
        assert_eq!(
            preview_summary(0, 20),
            "None of the 20 most recent activities change."
        );
        assert_eq!(preview_summary(0, 0), "No activity recorded yet.");
    }
}
