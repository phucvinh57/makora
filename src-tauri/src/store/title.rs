use crate::category::{exe_file_name, normalized_domain, url_key};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TitleQuality {
    Empty = 0,
    Placeholder = 1,
    Real = 2,
}

pub fn title_quality(title: &str, app_name: &str, url: Option<&str>) -> TitleQuality {
    let title = title.trim();
    if title.is_empty() {
        return TitleQuality::Empty;
    }

    let folded = title.to_ascii_lowercase();
    let mut placeholders = Vec::new();
    if let Some(url) = url.map(str::trim).filter(|url| !url.is_empty()) {
        placeholders.push(url.to_ascii_lowercase());
        let without_scheme = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .unwrap_or(url);
        placeholders.push(without_scheme.to_ascii_lowercase());
        if let Some(key) = url_key(url) {
            placeholders.push(key.to_ascii_lowercase());
        }
        if let Some(domain) = normalized_domain(url) {
            placeholders.push(domain.to_ascii_lowercase());
            if let Some(first_label) = domain.split('.').next() {
                placeholders.push(first_label.to_ascii_lowercase());
            }
        }
    }
    let app_name = app_name.trim();
    if !app_name.is_empty() {
        placeholders.push(app_name.to_ascii_lowercase());
        placeholders.push(exe_file_name(app_name).to_ascii_lowercase());
    }

    if placeholders.iter().any(|candidate| candidate == &folded) {
        TitleQuality::Placeholder
    } else {
        TitleQuality::Real
    }
}

pub fn canonical_title(title: &str, app_name: &str, url: Option<&str>) -> String {
    let trimmed = title.trim();
    match title_quality(trimmed, app_name, url) {
        TitleQuality::Empty | TitleQuality::Placeholder => String::new(),
        TitleQuality::Real => trimmed.to_string(),
    }
}
