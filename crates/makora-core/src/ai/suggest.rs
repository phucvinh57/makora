//! What the model is asked when it proposes a change to the taxonomy, and how its answer is
//! read: one JSON object, held to a schema by a remote provider and to a grammar by the local
//! model, and checked again here either way.

use serde_json::{json, Value};

use crate::categorization::ports::{ActivityText, ClusterName};
use crate::shared::text::{clip, single_line};

use super::choices::Choice;
use super::prompt::NONE_ANSWER;

const MAX_NAME_CHARS: usize = 40;
const MAX_DESCRIPTION_CHARS: usize = 200;
const MAX_SAMPLE_CHARS: usize = 120;
/// Room for the JSON answer: a name, a sentence and a parent path.
pub(super) const NAMING_MAX_TOKENS: u32 = 200;
/// Room for the JSON answer: one sentence.
pub(super) const DESCRIBE_MAX_TOKENS: u32 = 120;

/// What a description request tells a remote provider, for the network log.
pub(super) const DESCRIBE_FIELDS: &str = "app name, URL domain and path (no query) and window title of a few \
    activities the user kept in or moved out of a category, category names and descriptions, after PII redaction";

/// What a naming request tells a remote provider, for the network log.
pub(super) const NAMING_FIELDS: &str = "app name, URL domain and path (no query) and window title of a few \
    unlabeled activities, category names and descriptions, after PII redaction";

pub(super) fn naming_system_prompt(choices: &[Choice]) -> String {
    let mut prompt = String::from(
        "You organize the categories of a time tracker. The activities the user will show you fit \
         none of their categories. Propose one new category for them: a short name of one to three \
         words in the style of the existing ones, a one-sentence description of what belongs in \
         it, and the existing category it belongs under, or \"None\" for a top-level category.\n\n",
    );
    push_taxonomy(&mut prompt, choices);
    prompt
}

/// Asks for a sharper description of the category at `path`, which the model keeps getting
/// wrong.
pub(super) fn describe_system_prompt(choices: &[Choice], path: &str, current: Option<&str>) -> String {
    let current = current.map(single_line).filter(|text| !text.is_empty());
    let mut prompt = format!(
        "You organize the categories of a time tracker. An AI files the user's activities under \
         these categories by their names and descriptions. It keeps filing activities under \
         \"{path}\" that the user then moves elsewhere. Rewrite the description of \"{path}\" as \
         one sentence of at most {MAX_DESCRIPTION_CHARS} characters that covers what the user keeps \
         there and rules out what they move away. Describe kinds of activity; name an app or site \
         only when it alone decides.\n\nCurrent description of \"{path}\": {}\n\n",
        current.as_deref().unwrap_or("none"),
    );
    push_taxonomy(&mut prompt, choices);
    prompt
}

/// The user message of a description request: what stayed in `path`, then what left it and
/// where to. `moved` pairs each line with the destination's path.
pub(super) fn describe_lines(path: &str, kept: &[String], moved: &[(String, String)]) -> Vec<String> {
    let mut lines = vec![format!("The user keeps these in \"{path}\":")];
    if kept.is_empty() {
        lines.push("- (none yet)".to_string());
    }
    lines.extend(kept.iter().cloned());
    lines.push(format!("The user moved these out of \"{path}\":"));
    lines.extend(moved.iter().map(|(line, to)| format!("{line} -> {to}")));
    lines
}

pub(super) fn describe_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "description": { "type": "string" } },
        "required": ["description"],
        "additionalProperties": false,
    })
}

pub(super) fn describe_grammar() -> String {
    format!(
        "root ::= \"{{\" ws \"\\\"description\\\":\" ws description ws \"}}\"\n\
         description ::= \"\\\"\" char{{1,{MAX_DESCRIPTION_CHARS}}} \"\\\"\"\n\
         char ::= [^\"\\\\\\x00-\\x1F]\n\
         ws ::= [ ]?\n"
    )
}

/// The description in `answer`, on one line and within bounds; `None` when empty.
pub(super) fn parse_description(answer: &Value) -> Option<String> {
    answer
        .get("description")
        .and_then(Value::as_str)
        .map(|value| clip(&single_line(value), MAX_DESCRIPTION_CHARS).trim().to_string())
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case(NONE_ANSWER))
}

fn push_taxonomy(prompt: &mut String, choices: &[Choice]) {
    prompt.push_str("Existing categories:\n");
    for choice in choices {
        prompt.push_str("- ");
        prompt.push_str(&choice.path);
        if let Some(description) = choice.description.as_deref().map(single_line) {
            if !description.is_empty() {
                prompt.push_str(": ");
                prompt.push_str(&description);
            }
        }
        prompt.push('\n');
    }
}

/// One line per sample: `- App | domain/path | Title`.
pub(super) fn sample_lines(samples: &[ActivityText]) -> Vec<String> {
    samples
        .iter()
        .map(|sample| {
            let mut line = format!("- {}", clip(&sample.app_name, MAX_SAMPLE_CHARS));
            if let Some(url) = &sample.url {
                line.push_str(&format!(" | {}", clip(url, MAX_SAMPLE_CHARS)));
            }
            line.push_str(&format!(" | {}", clip(&sample.title, MAX_SAMPLE_CHARS)));
            line
        })
        .collect()
}

pub(super) fn naming_lines(samples: &[String]) -> Vec<String> {
    std::iter::once("Activities that fit no category:".to_string())
        .chain(samples.iter().cloned())
        .collect()
}

fn parent_options(choices: &[Choice]) -> Vec<&str> {
    std::iter::once(NONE_ANSWER)
        .chain(choices.iter().map(|choice| choice.path.as_str()))
        .collect()
}

pub(super) fn naming_schema(choices: &[Choice]) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "description": { "type": "string" },
            "parent": { "type": "string", "enum": parent_options(choices) },
        },
        "required": ["name", "description", "parent"],
        "additionalProperties": false,
    })
}

/// The same object as [`naming_schema`] as a GBNF grammar, with the strings' lengths bounded so a
/// small model cannot ramble.
pub(super) fn naming_grammar(choices: &[Choice]) -> String {
    let parents = parent_options(choices)
        .into_iter()
        .map(|path| gbnf_literal(&json!(path).to_string()))
        .collect::<Vec<_>>()
        .join(" | ");
    format!(
        "root ::= \"{{\" ws \"\\\"name\\\":\" ws name \",\" ws \"\\\"description\\\":\" ws description \",\" ws \"\\\"parent\\\":\" ws parent ws \"}}\"\n\
         name ::= \"\\\"\" char{{1,{MAX_NAME_CHARS}}} \"\\\"\"\n\
         description ::= \"\\\"\" char{{1,{MAX_DESCRIPTION_CHARS}}} \"\\\"\"\n\
         parent ::= {parents}\n\
         char ::= [^\"\\\\\\x00-\\x1F]\n\
         ws ::= [ ]?\n"
    )
}

/// `text` as a GBNF string literal.
fn gbnf_literal(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The name in `answer`, or `None` when it has none worth showing. An unknown parent counts as
/// none: the category is proposed at the top level.
pub(super) fn parse_name(answer: &Value, choices: &[Choice]) -> Option<ClusterName> {
    let text = |field: &str, max: usize| {
        answer
            .get(field)
            .and_then(Value::as_str)
            .map(|value| clip(&single_line(value), max).trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let name = text("name", MAX_NAME_CHARS)?;
    if name.eq_ignore_ascii_case(NONE_ANSWER) {
        return None;
    }
    let parent = answer
        .get("parent")
        .and_then(Value::as_str)
        .map(str::trim)
        .and_then(|path| choices.iter().find(|choice| choice.path == path))
        .map(|choice| choice.id);
    Some(ClusterName {
        name,
        description: text("description", MAX_DESCRIPTION_CHARS),
        parent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::ids::CategoryId;

    fn choices() -> Vec<Choice> {
        vec![
            Choice {
                path: "Work".into(),
                id: CategoryId(1),
                description: Some("Paid work".into()),
            },
            Choice {
                path: "Work > \"Deep\" work".into(),
                id: CategoryId(2),
                description: None,
            },
        ]
    }

    #[test]
    fn an_answer_names_a_category_under_a_known_parent() {
        let answer = json!({ "name": " Code review\n", "description": "Reviewing pull requests.", "parent": "Work" });
        assert_eq!(
            parse_name(&answer, &choices()),
            Some(ClusterName {
                name: "Code review".into(),
                description: Some("Reviewing pull requests.".into()),
                parent: Some(CategoryId(1)),
            })
        );
    }

    #[test]
    fn an_unknown_parent_means_top_level_and_an_empty_name_means_nothing() {
        let answer = json!({ "name": "Games", "description": "", "parent": "Fun" });
        let name = parse_name(&answer, &choices()).unwrap();
        assert_eq!((name.parent, name.description), (None, None));
        assert!(parse_name(&json!({ "name": "  ", "parent": "None" }), &choices()).is_none());
        assert!(parse_name(&json!({ "name": "None" }), &choices()).is_none());
        assert!(parse_name(&json!({}), &choices()).is_none());
    }

    #[test]
    fn the_schema_and_grammar_offer_none_and_every_path() {
        let schema = naming_schema(&choices());
        assert_eq!(schema["properties"]["parent"]["enum"], json!(["None", "Work", "Work > \"Deep\" work"]));
        let grammar = naming_grammar(&choices());
        assert!(grammar.contains(r#"parent ::= "\"None\"" | "\"Work\"" | "\"Work > \\\"Deep\\\" work\"""#), "{grammar}");
        assert!(grammar.contains("char{1,40}"));
    }

    #[test]
    fn a_description_answer_is_one_bounded_line() {
        let long = "x".repeat(300);
        assert_eq!(parse_description(&json!({ "description": long })).unwrap().chars().count(), MAX_DESCRIPTION_CHARS);
        assert_eq!(parse_description(&json!({ "description": " Code\nreview " })).as_deref(), Some("Code review"));
        assert!(parse_description(&json!({ "description": " " })).is_none());
        assert!(parse_description(&json!({})).is_none());
    }

    #[test]
    fn a_description_request_shows_what_stayed_and_where_the_rest_went() {
        let prompt = describe_system_prompt(&choices(), "Work", Some("Paid work"));
        assert!(prompt.contains("Current description of \"Work\": Paid work"));
        assert!(prompt.contains("- Work: Paid work\n"));
        let lines = describe_lines("Work", &["- Code | main.rs".into()], &[("- Steam | Hades".into(), "Fun".into())]);
        assert_eq!(
            lines,
            [
                "The user keeps these in \"Work\":",
                "- Code | main.rs",
                "The user moved these out of \"Work\":",
                "- Steam | Hades -> Fun",
            ]
        );
        let grammar = describe_grammar();
        assert!(grammar.contains("char{1,200}"), "{grammar}");
        assert!(grammar.starts_with(r#"root ::= "{" ws "\"description\":" ws description ws "}""#), "{grammar}");
    }

    #[test]
    fn the_system_prompt_lists_the_taxonomy() {
        let prompt = naming_system_prompt(&choices());
        assert!(prompt.contains("- Work: Paid work\n"));
        assert!(prompt.contains("- Work > \"Deep\" work\n"));
    }
}
