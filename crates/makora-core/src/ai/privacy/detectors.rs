//! One finder per kind of personal data. Each returns the byte ranges it found in a piece of text;
//! deciding which of several overlapping finds wins is [`super::apply`]'s job.

use std::net::Ipv6Addr;
use std::ops::Range;
use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};
use tracing::warn;

pub type Spans = Vec<Range<usize>>;

macro_rules! regex {
    ($pattern:expr) => {{
        static RE: OnceLock<Regex> = OnceLock::new();
        RE.get_or_init(|| Regex::new($pattern).expect("a valid pattern"))
    }};
}

fn char_before(text: &str, at: usize) -> Option<char> {
    text[..at].chars().next_back()
}

fn char_after(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Not glued to a letter, digit or underscore on either side.
fn stands_alone(text: &str, range: &Range<usize>) -> bool {
    !char_before(text, range.start).is_some_and(is_word) && !char_after(text, range.end).is_some_and(is_word)
}

/// Every part of `range` that `valid` accepts, cut only at the `splits` characters, earliest
/// start first and then longest: a greedy pattern can take in numbers before or after the one it
/// is looking for, or several of them in a row.
fn valid_windows(
    text: &str,
    range: Range<usize>,
    splits: &[char],
    valid: impl Fn(&str, Option<char>) -> bool,
) -> Spans {
    let mut found = Vec::new();
    let mut from = range.start;
    while let Some(window) = valid_window(text, from..range.end, splits, &valid) {
        from = window.end;
        found.push(window);
    }
    found
}

fn valid_window(
    text: &str,
    range: Range<usize>,
    splits: &[char],
    valid: &impl Fn(&str, Option<char>) -> bool,
) -> Option<Range<usize>> {
    let candidate = &text[range.clone()];
    let cuts: Vec<usize> = candidate.match_indices(splits).map(|(at, _)| at).collect();
    let starts = std::iter::once(0).chain(cuts.iter().map(|at| at + 1));
    for start in starts {
        let ends = std::iter::once(candidate.len()).chain(cuts.iter().rev().copied().filter(|at| *at > start));
        for end in ends {
            let window = range.start + start..range.start + end;
            let before = char_before(text, window.start);
            if !window.is_empty() && !char_after(text, window.end).is_some_and(is_word) && valid(&text[window.clone()], before) {
                return Some(window);
            }
        }
    }
    None
}

pub fn emails(text: &str) -> Spans {
    regex!(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?)*\.[A-Za-z]{2,}")
        .find_iter(text)
        .map(|found| found.range())
        .collect()
}

/// International numbers (`+84 912 345 678`) and local ones written with separators
/// (`(555) 123-4567`, `0912.345.678`), not dates, versions or bare digit runs.
pub fn phones(text: &str) -> Spans {
    regex!(r"\+?\(?\d[\d .()\-]{5,}\d")
        .find_iter(text)
        .flat_map(|found| {
            valid_windows(text, found.range(), &[' '], |candidate, before| {
                let glued = before.is_some_and(|c| {
                    is_word(c) || (!candidate.starts_with('+') && matches!(c, '.' | '-' | '/' | ':'))
                });
                !glued && is_phone(candidate)
            })
        })
        .filter(|range| !char_after(text, range.end).is_some_and(|c| matches!(c, '/' | ':')))
        .collect()
}

fn is_phone(candidate: &str) -> bool {
    let digits = candidate.chars().filter(char::is_ascii_digit).count();
    if !(7..=15).contains(&digits) {
        return false;
    }
    if ["  ", "..", "--", " - ", "((", "))"].iter().any(|run| candidate.contains(run)) {
        return false;
    }
    if regex!(r"\d{4}[-./]\d{1,2}[-./]\d{1,2}|\d{1,2}[-./]\d{1,2}[-./]\d{4}").is_match(candidate) {
        return false;
    }
    let international = candidate.starts_with('+');
    let groups: Vec<&str> = candidate
        .split(|c: char| !c.is_ascii_digit())
        .filter(|group| !group.is_empty())
        .collect();
    if !international && groups.len() < 2 {
        return false;
    }
    let separators: Vec<char> = candidate.chars().filter(|c| !c.is_ascii_digit() && *c != '+').collect();
    if !separators.is_empty() && separators.iter().all(|c| *c == '.') {
        return (3..=4).contains(&groups.len()) && groups.iter().all(|group| (2..=4).contains(&group.len()));
    }
    if !international && separators.iter().all(|c| *c == ' ') && groups.len() < 3 {
        return false;
    }
    true
}

pub fn ip_addresses(text: &str) -> Spans {
    let v4 = regex!(r"\d{1,3}(?:\.\d{1,3}){3}")
        .find_iter(text)
        .map(|found| found.range())
        .filter(|range| {
            let dotted_before = char_before(text, range.start).is_some_and(|c| is_word(c) || c == '.');
            let rest = &text[range.end..];
            let dotted_after = rest.starts_with('.') && rest[1..].starts_with(|c: char| c.is_ascii_digit());
            !dotted_before
                && !dotted_after
                && !char_after(text, range.end).is_some_and(is_word)
                && text[range.clone()].split('.').all(|octet| octet.parse::<u16>().is_ok_and(|n| n <= 255))
        });
    let v6 = regex!(r"[0-9A-Fa-f]{0,4}(?::[0-9A-Fa-f]{0,4}){2,7}")
        .find_iter(text)
        .map(|found| found.range())
        .filter(|range| {
            let candidate = &text[range.clone()];
            !char_before(text, range.start).is_some_and(|c| is_word(c) || c == ':')
                && !char_after(text, range.end).is_some_and(|c| is_word(c) || c == ':')
                && candidate.chars().filter(char::is_ascii_hexdigit).count() >= 4
                && candidate.parse::<Ipv6Addr>().is_ok()
        });
    v4.chain(v6).collect()
}

/// 13 to 19 digits, grouped by spaces or dashes or not, that pass the Luhn check.
pub fn card_numbers(text: &str) -> Spans {
    regex!(r"\d(?:[ -]?\d){12,79}")
        .find_iter(text)
        .flat_map(|found| {
            valid_windows(text, found.range(), &[' ', '-'], |candidate, before| {
                !before.is_some_and(is_word) && is_card(candidate)
            })
        })
        .collect()
}

fn is_card(candidate: &str) -> bool {
    let digits: Vec<u32> = candidate.chars().filter_map(|c| c.to_digit(10)).collect();
    (13..=19).contains(&digits.len()) && luhn(&digits)
}

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(position, &digit)| match position % 2 {
            0 => digit,
            _ if digit * 2 > 9 => digit * 2 - 9,
            _ => digit * 2,
        })
        .sum();
    sum.is_multiple_of(10)
}

pub fn ibans(text: &str) -> Spans {
    regex!(r"[A-Z]{2}\d{2}(?: ?[A-Z0-9]){11,30}")
        .find_iter(text)
        .filter(|found| !char_before(text, found.start()).is_some_and(is_word))
        .flat_map(|found| {
            let start = found.start();
            valid_windows(text, found.range(), &[' '], |candidate, _| {
                text[start..].starts_with(candidate) && is_iban(candidate)
            })
        })
        .collect()
}

fn is_iban(candidate: &str) -> bool {
    let compact: String = candidate.chars().filter(|c| *c != ' ').collect();
    if !(15..=34).contains(&compact.len()) {
        return false;
    }
    let (head, tail) = compact.split_at(4);
    let mut remainder = 0u32;
    for c in tail.chars().chain(head.chars()) {
        let Some(value) = c.to_digit(36) else {
            return false;
        };
        remainder = if value >= 10 {
            (remainder * 100 + value) % 97
        } else {
            (remainder * 10 + value) % 97
        };
    }
    remainder == 1
}

/// API keys and tokens: JWTs, well-known key prefixes, long hex strings and long base64-looking
/// runs that mix cases and digits.
pub fn secrets(text: &str) -> Spans {
    let prefixed = regex!(
        r"eyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}|(?:sk|pk|rk)-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|AKIA[0-9A-Z]{16}|xox[abpors]-[A-Za-z0-9-]{10,}"
    )
    .find_iter(text)
    .map(|found| found.range())
    .filter(|range| !char_before(text, range.start).is_some_and(is_word));
    let hex = regex!(r"[0-9A-Fa-f]{32,}")
        .find_iter(text)
        .map(|found| found.range())
        .filter(|range| stands_alone(text, range));
    let base64 = regex!(r"[A-Za-z0-9+_]{40,}={0,2}")
        .find_iter(text)
        .map(|found| found.range())
        .filter(|range| {
            let candidate = &text[range.clone()];
            stands_alone(text, range)
                && candidate.chars().any(|c| c.is_ascii_uppercase())
                && candidate.chars().any(|c| c.is_ascii_lowercase())
                && candidate.chars().filter(char::is_ascii_digit).count() >= 3
        });
    prefixed.chain(hex).chain(base64).collect()
}

pub fn uuids(text: &str) -> Spans {
    regex!(r"[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}")
        .find_iter(text)
        .map(|found| found.range())
        .filter(|range| stands_alone(text, range))
        .collect()
}

/// Eight digits or more in a row: order, ticket and account numbers.
pub fn long_numbers(text: &str) -> Spans {
    regex!(r"\d{8,}").find_iter(text).map(|found| found.range()).collect()
}

/// Case-insensitive literal words, the longest first; a word that starts or ends with a letter
/// or digit only matches whole there, so `Jane` does not match inside `Janet`.
#[derive(Debug, Default)]
pub struct Literals(Option<Regex>, Vec<Regex>);

/// Room for a few thousand words to hide; the default limit fits far fewer case-insensitive ones.
const LITERALS_SIZE_LIMIT: usize = 64 << 20;

/// `word` escaped, matching only whole where it starts or ends with a letter or digit.
fn whole_word(word: &str) -> String {
    let start = if word.starts_with(is_word) { r"\b" } else { "" };
    let end = if word.ends_with(is_word) { r"\b" } else { "" };
    format!("{start}{}{end}", regex::escape(word))
}

fn literal_regex(alternatives: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(&format!("(?i)(?:{alternatives})")).size_limit(LITERALS_SIZE_LIMIT).build()
}

impl Literals {
    pub fn new<S: AsRef<str>>(words: &[S]) -> Self {
        let mut words: Vec<&str> = words.iter().map(AsRef::as_ref).map(str::trim).filter(|word| !word.is_empty()).collect();
        if words.is_empty() {
            return Self::default();
        }
        words.sort_by_key(|word| std::cmp::Reverse(word.chars().count()));
        let alternatives: Vec<String> = words.iter().map(|word| whole_word(word)).collect();
        match literal_regex(&alternatives.join("|")) {
            Ok(re) => Self(Some(re), Vec::new()),
            Err(err) => {
                warn!(words = words.len(), error = %err, "the words to hide could not be compiled together; matching them one by one");
                Self(None, alternatives.iter().filter_map(|alternative| literal_regex(alternative).ok()).collect())
            }
        }
    }

    pub fn find(&self, text: &str) -> Spans {
        self.0
            .iter()
            .chain(self.1.iter())
            .flat_map(|re| re.find_iter(text).map(|found| found.range()))
            .collect()
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(spans: Spans, text: &str) -> Vec<String> {
        spans.into_iter().map(|range| text[range].to_string()).collect()
    }

    #[test]
    fn emails_are_found() {
        let text = "Inbox (3) - jane.doe+work@mail.acme.co.uk - Gmail";
        assert_eq!(found(emails(text), text), ["jane.doe+work@mail.acme.co.uk"]);
        assert!(emails("@mention in #general, user@localhost").is_empty());
    }

    #[test]
    fn phones_are_found() {
        for text in [
            "Call +84 912 345 678 now",
            "(555) 123-4567",
            "Ring 0912.345.678",
            "+442071838750",
            "tel:+84912345678",
            "Order 12 0912 345 678",
        ] {
            assert_eq!(phones(text).len(), 1, "{text}");
        }
        let two = "Call 0912 345 678 or 0987 654 321";
        assert_eq!(found(phones(two), two), ["0912 345 678", "0987 654 321"]);
        let two_cards = "4111 1111 1111 1111 5500 0055 5555 5559";
        assert_eq!(card_numbers(two_cards).len(), 2);
    }

    #[test]
    fn phone_near_misses_are_left() {
        for text in [
            "Release 2026-11-06",
            "06.11.2026 notes",
            "Chrome 120.0.6099.109",
            "v2.10.3",
            "Windows 10.0.19045",
            "Score 12 - 345678",
            "Page 1234567",
            "12:30:45",
            "ext 1234",
        ] {
            assert!(phones(text).is_empty(), "{text}: {:?}", found(phones(text), text));
        }
    }

    #[test]
    fn ip_addresses_are_found() {
        let text = "ssh 192.168.1.20:22 and fe80::1ff:fe23:4567:890a done";
        assert_eq!(found(ip_addresses(text), text), ["192.168.1.20", "fe80::1ff:fe23:4567:890a"]);
    }

    #[test]
    fn ip_near_misses_are_left() {
        for text in ["Chrome 120.0.6099.109", "1.2.3.4.5", "std::fmt::Display", "12:30:45", "aa:bb:cc:dd:ee:ff", "v1.2.3.4"] {
            assert!(ip_addresses(text).is_empty(), "{text}: {:?}", found(ip_addresses(text), text));
        }
    }

    #[test]
    fn card_numbers_need_the_luhn_check() {
        let text = "Pay 4111 1111 1111 1111 2026 and 4111-1111-1111-1112";
        assert_eq!(found(card_numbers(text), text), ["4111 1111 1111 1111"]);
        assert_eq!(card_numbers("5500005555555559").len(), 1);
        let after_a_number = "Checkout step 2 4111 1111 1111 1111";
        assert_eq!(found(card_numbers(after_a_number), after_a_number), ["4111 1111 1111 1111"]);
    }

    #[test]
    fn ibans_need_the_checksum() {
        let text = "Transfer to DE89 3704 0044 0532 0130 00 TODAY";
        assert_eq!(found(ibans(text), text), ["DE89 3704 0044 0532 0130 00"]);
        assert_eq!(ibans("GB82WEST12345698765432").len(), 1);
        assert!(ibans("GB82WEST12345698765433").is_empty());
    }

    #[test]
    fn secrets_are_found() {
        for text in [
            "key sk-proj-abcdefghijklmnop1234",
            "ghp_abcdefghijklmnopqrstuvwxyz0123",
            "AKIAIOSFODNN7EXAMPLE",
            "xoxb-1234567890-abcdef",
            "eyJhbGciOi.eyJzdWIiOi.SflKxwRJSM",
            "commit 3f786850e387550fdab836ed7e6dc881de23001b",
            "token aB3dE5fG7hJ9kL1mN2pQ4rS6tU8vW0xY2zA4bC6d",
        ] {
            assert_eq!(secrets(text).len(), 1, "{text}");
        }
        assert!(secrets("a_very_long_snake_case_function_name_without_numbers").is_empty());
        assert!(secrets("task-list-for-the-sk-team").is_empty());
    }

    #[test]
    fn uuids_and_long_numbers_are_found() {
        let text = "Ticket 123e4567-e89b-12d3-a456-426614174000 order 98765432 room 1234567";
        assert_eq!(found(uuids(text), text), ["123e4567-e89b-12d3-a456-426614174000"]);
        assert_eq!(found(long_numbers(text), text), ["426614174000", "98765432"]);
    }

    #[test]
    fn literals_match_whole_words_ignoring_case() {
        let literals = Literals::new(&["Jane", "Acme Corp", " "]);
        let text = "acme corp review with JANE, not Janet";
        assert_eq!(found(literals.find(text), text), ["acme corp", "JANE"]);
        let unicode = Literals::new(&["Nguyễn"]);
        let text = "Họp với nguyễn — Zoom";
        assert_eq!(found(unicode.find(text), text), ["nguyễn"]);
        assert!(Literals::new::<&str>(&[]).find("anything").is_empty());
    }
}
