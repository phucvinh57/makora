//! Turning what the finders found into the text that is sent: one find per stretch of text, each
//! replaced by the placeholder of its kind.

use std::ops::Range;
use std::sync::OnceLock;

use regex::Regex;

use crate::shared::text::single_line;

use super::Found;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub range: Range<usize>,
    pub found: Found,
}

/// Placeholders already in a text, which nothing may match inside; this is what makes redacting
/// twice the same as redacting once.
fn placeholders(text: &str) -> Vec<Range<usize>> {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[(?:email|phone|ip|card|iban|secret|id|name|term|app|site|hidden)\]|\{id\}").expect("a valid pattern")
    })
    .find_iter(text)
    .map(|found| found.range())
    .collect()
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

/// The hits to replace, in text order: the longest wins an overlap, then the earliest, then the
/// kind with the higher priority. Hits touching a placeholder already in `text` are dropped.
pub fn resolve(text: &str, mut hits: Vec<Hit>) -> Vec<Hit> {
    let protected = placeholders(text);
    hits.retain(|hit| !hit.range.is_empty() && !protected.iter().any(|range| overlaps(range, &hit.range)));
    hits.sort_by_key(|hit| (std::cmp::Reverse(hit.range.len()), hit.range.start, hit.found.priority()));
    let mut kept: Vec<Hit> = Vec::with_capacity(hits.len());
    for hit in hits {
        if !kept.iter().any(|other| overlaps(&other.range, &hit.range)) {
            kept.push(hit);
        }
    }
    kept.sort_by_key(|hit| hit.range.start);
    kept
}

/// `text` with every hit replaced, a placeholder that repeats with only spaces between written
/// once, on one line.
pub fn replace(text: &str, hits: &[Hit]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut last: Option<Found> = None;
    for hit in hits {
        let gap = &text[at..hit.range.start];
        let repeats = last.is_some_and(|found| found.placeholder() == hit.found.placeholder()) && gap.trim().is_empty();
        if !repeats {
            out.push_str(gap);
            out.push_str(hit.found.placeholder());
        }
        last = Some(hit.found);
        at = hit.range.end;
    }
    out.push_str(&text[at..]);
    single_line(&out)
}

#[cfg(test)]
mod tests {
    use super::super::Kind;
    use super::*;

    fn hit(range: Range<usize>, found: Found) -> Hit {
        Hit { range, found }
    }

    #[test]
    fn the_longest_hit_wins_then_the_earliest() {
        let text = "aaaaaaaaaa";
        let hits = vec![
            hit(0..4, Found::Kind(Kind::Phone)),
            hit(2..9, Found::Kind(Kind::LongNumber)),
            hit(8..10, Found::Kind(Kind::Email)),
        ];
        assert_eq!(resolve(text, hits), [hit(2..9, Found::Kind(Kind::LongNumber))]);
        let ties = vec![hit(0..3, Found::Kind(Kind::LongNumber)), hit(0..3, Found::Kind(Kind::Card))];
        assert_eq!(resolve(text, ties), [hit(0..3, Found::Kind(Kind::Card))]);
    }

    #[test]
    fn repeated_placeholders_collapse() {
        let text = "call 111 222 or 333";
        let hits = vec![
            hit(5..8, Found::Kind(Kind::Phone)),
            hit(9..12, Found::Kind(Kind::Phone)),
            hit(16..19, Found::Kind(Kind::Phone)),
        ];
        assert_eq!(replace(text, &hits), "call [phone] or [phone]");
    }

    #[test]
    fn placeholders_are_never_matched_again() {
        let text = "mail [email] about [id]";
        let hits = vec![hit(6..11, Found::Term), hit(20..22, Found::Term)];
        assert!(resolve(text, hits).is_empty());
    }
}
