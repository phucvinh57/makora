use super::source_key::{exe_file_name, normalized_domain, url_key};

/// Whether `title` is just the app name, the URL, or the domain repeated back — a placeholder a
/// window shows before its real title loads, not a title worth keeping.
fn is_placeholder(title: &str, app_name: &str, url: Option<&str>) -> bool {
    let folded = placeholder_form(title).to_ascii_lowercase();
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
    placeholders.iter().any(|candidate| candidate == &folded)
}

/// A browser's placeholder title is the URL as loaded, query included, while the tracked URL has
/// it stripped. Only a single token is cut, so a real title like "What is Rust? - Blog" stays whole.
fn placeholder_form(title: &str) -> &str {
    if title.contains(char::is_whitespace) {
        return title;
    }
    title.split(['?', '#']).next().unwrap_or(title)
}

pub fn canonical_title(title: &str, app_name: &str, url: Option<&str>) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() || is_placeholder(trimmed, app_name, url) {
        String::new()
    } else {
        trimmed.to_string()
    }
}
