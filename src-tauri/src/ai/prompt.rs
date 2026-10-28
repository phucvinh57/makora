//! The prompt every language model is given, local or remote: the taxonomy as the system prompt,
//! identical for every activity so it can be cached, and the activity with the filed ones most like
//! it as the user turn.

use std::collections::HashMap;

use crate::categorization::domain::retrieval::RelatedActivity;
use crate::categorization::ports::ActivityText;
use crate::shared::ids::CategoryId;
use crate::shared::text::{clip, single_line};

use super::choices::Choice;

const MAX_TITLE_CHARS: usize = 200;
const MAX_URL_CHARS: usize = 200;
const MAX_RELATED_CHARS: usize = 80;
pub(super) const NONE_ANSWER: &str = "None";

/// `- App | domain/path | Title => Category`. `None` when the category has since been deleted.
pub(super) fn related_line(related: &RelatedActivity, paths: &HashMap<CategoryId, &str>) -> Option<String> {
    let path = paths.get(&related.category)?;
    let mut line = format!("- {}", clip(&related.app_name, MAX_RELATED_CHARS));
    if let Some(url) = &related.url {
        line.push_str(&format!(" | {}", clip(url, MAX_RELATED_CHARS)));
    }
    line.push_str(&format!(
        " | {} => {path}",
        clip(&related.title, MAX_RELATED_CHARS)
    ));
    Some(line)
}

pub(super) fn system_prompt(choices: &[Choice]) -> String {
    let mut prompt = String::from(
        "You label computer activity for a time tracker. Given the app, page URL and window \
         title, weigh the URL and the title together and reply with exactly one category \
         from the list below, using the most specific one that fits. How the user filed similar \
         activities, when shown, takes precedence over the descriptions. Reply \"None\" \
         if no category fits.\n\nCategories:\n",
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

pub(super) fn user_prompt(activity: &ActivityText, related: &[String]) -> String {
    let mut prompt = String::new();
    if !related.is_empty() {
        prompt.push_str("Activities the user filed that are most like this one:\n");
        for line in related {
            prompt.push_str(line);
            prompt.push('\n');
        }
        prompt.push_str("\nLabel this activity:\n");
    }
    prompt.push_str(&format!("App: {}\n", single_line(&activity.app_name)));
    if let Some(url) = &activity.url {
        prompt.push_str(&format!("URL: {}\n", clip(url, MAX_URL_CHARS)));
    }
    prompt.push_str(&format!(
        "Title: {}",
        clip(&activity.title, MAX_TITLE_CHARS)
    ));
    prompt
}
