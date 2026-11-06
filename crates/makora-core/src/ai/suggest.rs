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

/// What a naming request tells a remote provider, for the network log.
pub(super) const NAMING_FIELDS: &str = "app name, URL domain and path (no query) and window title of a few \
    unlabeled activities, category names and descriptions, after PII redaction";

pub(super) fn naming_system_prompt(choices: &[Choice]) -> String {
    let mut prompt = String::from(
        "You organize the categories of a time tracker. The activities the user will show you fit \
         none of their categories. Propose one new category for them: a short name of one to three \
         words in the style of the existing ones, a one-sentence description of what belongs in \
         it, and the existing category it belongs under, or \"None\" for a top-level category.\n\n\
         Existing categories:\n",
    );
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
    prompt
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

pub(super) fn naming_user_prompt(lines: &[String]) -> String {
    format!("Activities that fit no category:\n{}", lines.join("\n"))
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
    fn the_system_prompt_lists_the_taxonomy() {
        let prompt = naming_system_prompt(&choices());
        assert!(prompt.contains("- Work: Paid work\n"));
        assert!(prompt.contains("- Work > \"Deep\" work\n"));
    }
}
