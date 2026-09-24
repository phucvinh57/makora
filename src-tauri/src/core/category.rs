use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyKind {
    Exe,
    Domain,
}

impl KeyKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exe => "exe",
            Self::Domain => "domain",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CategoryTree {
    ids: HashSet<i64>,
    children: HashMap<i64, Vec<i64>>,
}

impl CategoryTree {
    pub(crate) fn new<I>(categories: I) -> Self
    where
        I: IntoIterator<Item = (i64, Option<i64>)>,
    {
        let mut ids = HashSet::new();
        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        for (id, parent_id) in categories {
            ids.insert(id);
            if let Some(parent_id) = parent_id {
                children.entry(parent_id).or_default().push(id);
            }
        }
        for child_ids in children.values_mut() {
            child_ids.sort_unstable();
        }
        Self { ids, children }
    }

    pub(crate) fn contains(&self, id: i64) -> bool {
        self.ids.contains(&id)
    }

    pub(crate) fn descendants(&self, root: i64) -> Vec<i64> {
        if !self.contains(root) {
            return Vec::new();
        }
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            result.push(id);
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().rev().copied());
            }
        }
        result
    }

    pub(crate) fn deletion_order(&self, root: i64) -> Vec<i64> {
        let descendants = self.descendants(root);
        let descendant_set: HashSet<_> = descendants.iter().copied().collect();
        let mut depth = HashMap::new();
        depth.insert(root, 0usize);
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            let next_depth = depth.get(&id).copied().unwrap_or(0) + 1;
            if let Some(children) = self.children.get(&id) {
                for child in children {
                    if descendant_set.contains(child) && !depth.contains_key(child) {
                        depth.insert(*child, next_depth);
                        stack.push(*child);
                    }
                }
            }
        }
        let mut ordered = descendants;
        ordered.sort_by_key(|id| (Reverse(depth.get(id).copied().unwrap_or(0)), *id));
        ordered
    }
}

pub fn category_key(exe_path: &str, url: Option<&str>) -> (KeyKind, String) {
    match url.and_then(normalized_domain) {
        Some(domain) => (KeyKind::Domain, domain),
        None => (KeyKind::Exe, exe_file_name(exe_path).to_ascii_lowercase()),
    }
}

pub fn normalized_domain(url: &str) -> Option<String> {
    let host = domain_of(url)?.to_ascii_lowercase();
    Some(
        host.strip_prefix("www.")
            .map(str::to_string)
            .unwrap_or(host),
    )
}

pub fn url_key(url: &str) -> Option<String> {
    let domain = normalized_domain(url)?;
    let parsed = parse_lenient(url);
    let path = parsed
        .as_ref()
        .map(|p| p.path())
        .unwrap_or("/")
        .trim_start_matches('/')
        .trim_end_matches('/');
    let query = parsed.as_ref().and_then(|p| p.query()).unwrap_or("");

    let mut key = domain;
    if !path.is_empty() {
        key.push('/');
        key.push_str(path);
    }
    if !query.is_empty() {
        key.push('?');
        key.push_str(query);
    }
    Some(key)
}

pub fn filter_query_params(url: &str, allowed: &[String]) -> String {
    let Some(mut parsed) = parse_lenient(url) else {
        return url.split('#').next().unwrap_or(url).to_string();
    };
    parsed.set_fragment(None);
    let kept = parsed.query().map(|query| {
        let mut kept: Vec<&str> = query
            .split('&')
            .filter(|param| {
                let name = param.split('=').next().unwrap_or("");
                allowed.iter().any(|a| a == name)
            })
            .collect();
        kept.sort_unstable();
        kept.join("&")
    });
    parsed.set_query(kept.as_deref().filter(|q| !q.is_empty()));
    parsed.to_string()
}

pub(crate) fn exe_file_name(exe_path: &str) -> &str {
    exe_path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(exe_path)
}

fn parse_lenient(input: &str) -> Option<Url> {
    Url::parse(input)
        .or_else(|_| Url::parse(&format!("http://{input}")))
        .ok()
}

pub(crate) fn domain_of(url: &str) -> Option<String> {
    let host = parse_lenient(url)?.host_str()?.to_string();
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}
