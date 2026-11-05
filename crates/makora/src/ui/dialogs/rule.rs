//! Adding rules, or editing one. The category comes first and every rule written below it files
//! there, so a batch of rules for one category goes in with a single pick. Editing changes one
//! rule, so it has one row. Each row previews what it would match, a moment after typing stops.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, EventEmitter, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window, div, px,
};
use makora_lib::categorization::application::rules::{
    CategoryRuleView, PathSuggestion, RuleDraft, RulePreview, SourceSuggestion,
};
use makora_lib::categorization::domain::rules::{PatternField, RuleKind};
use makora_lib::shared::ids::CategoryId;
use makora_lib::tracking::domain::source_key::KeyKind;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs::add_category::{AddCategoryData, AddCategoryDialog};
use crate::ui::dialogs::{self, AppDialog};
use crate::ui::error::UiError;
use crate::ui::format::duration::format_seconds;
use crate::ui::format::rules::{
    PATTERN_FIELDS, RULE_KINDS, parse_pattern_field, parse_rule_kind, pattern_field_label,
    rule_kind_label, rule_value_placeholder,
};
use crate::ui::widgets::category_combobox::{CategoryCombobox, CategoryComboboxEvent};
use crate::ui::widgets::select_item::{SelectItem, SelectItems, select_item_index};
use crate::ui::widgets::suggest_input::{SuggestInput, SuggestInputEvent, Suggestion};

const PREVIEW_DEBOUNCE: Duration = Duration::from_millis(300);

/// What one row says: the domain goes in `value` and the rest of a page's address in `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleFields {
    pub kind: RuleKind,
    pub field: PatternField,
    pub value: String,
    pub path: String,
}

impl RuleFields {
    /// A blank row, keeping the kind and field of `like`.
    pub fn empty(like: Option<&RuleFields>) -> Self {
        Self {
            kind: like.map_or(RuleKind::Site, |like| like.kind),
            field: like.map_or(PatternField::Url, |like| like.field),
            value: String::new(),
            path: String::new(),
        }
    }

    /// A saved rule split back into its fields: a page's value at its first `/`.
    pub fn of(rule: &CategoryRuleView) -> Self {
        let (value, path) = match rule.value.find('/') {
            Some(slash) if rule.kind == RuleKind::Page => (
                rule.value[..slash].to_string(),
                rule.value[slash..].to_string(),
            ),
            _ => (rule.value.clone(), String::new()),
        };
        Self {
            kind: rule.kind,
            field: rule.field.unwrap_or(PatternField::Url),
            value,
            path,
        }
    }

    /// What the rule matches: the trimmed value, or for a page its domain and path joined.
    pub fn match_value(&self) -> String {
        let value = self.value.trim();
        if self.kind != RuleKind::Page {
            return value.to_string();
        }
        let path = self.path.trim();
        if value.is_empty() || path.is_empty() {
            return value.to_string();
        }
        let slash = if path.starts_with('/') { "" } else { "/" };
        format!("{value}{slash}{path}")
    }

    pub fn pattern_field(&self) -> Option<PatternField> {
        (self.kind == RuleKind::Pattern).then_some(self.field)
    }

    fn previewed(&self) -> Previewed {
        (self.kind, self.pattern_field(), self.match_value())
    }
}

type Previewed = (RuleKind, Option<PatternField>, String);

fn source_hint(name: &str, key: &str, seconds: f64) -> String {
    let label = if !name.is_empty() && name != key {
        format!("{name} · ")
    } else {
        String::new()
    };
    format!("{label}{}", format_seconds(seconds))
}

fn source_suggestions(sources: &[SourceSuggestion], kind: RuleKind) -> Vec<Suggestion> {
    let wanted = if kind == RuleKind::App {
        KeyKind::Exe
    } else {
        KeyKind::Domain
    };
    sources
        .iter()
        .filter(|source| source.kind == wanted)
        .map(|source| {
            Suggestion::new(source.key.clone())
                .hint(source_hint(&source.name, &source.key, source.total_seconds))
                .search(source.name.clone())
        })
        .collect()
}

fn path_suggestions(paths: Vec<PathSuggestion>) -> Vec<Suggestion> {
    paths
        .into_iter()
        .map(|page| Suggestion::new(page.path).hint(format_seconds(page.total_seconds)))
        .collect()
}

fn preview_summary(preview: &RulePreview) -> String {
    let noun = if preview.activity_count == 1 {
        "activity"
    } else {
        "activities"
    };
    let pinned = if preview.pinned_count > 0 {
        format!(" ({} pinned by you will not change)", preview.pinned_count)
    } else {
        String::new()
    };
    format!(
        "Matches {} {noun} · {}{pinned}",
        preview.activity_count,
        format_seconds(preview.total_seconds)
    )
}

fn value_hint(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::Pattern => "* matches any text and ? matches one character, ignoring case.",
        RuleKind::Page => "Covers this page and everything below it.",
        RuleKind::App | RuleKind::Site => "Pick from what you have tracked, or type your own.",
    }
}

/// The add button's label for `filled` rows waiting to be saved.
pub fn save_label(editing: bool, filled: usize) -> String {
    if editing {
        "Save rule".into()
    } else if filled > 1 {
        format!("Add {filled} rules")
    } else {
        "Add rule".into()
    }
}

enum Preview {
    Idle,
    Checking,
    Failed(String),
    Ready(RulePreview),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowEvent {
    Changed,
    Remove,
}

struct RuleRow {
    id: usize,
    fields: RuleFields,
    removable: bool,
    error: Option<String>,
    sources: Rc<Vec<SourceSuggestion>>,
    kind_select: Entity<SelectState<SelectItems>>,
    field_select: Entity<SelectState<SelectItems>>,
    value: Entity<SuggestInput>,
    path: Entity<SuggestInput>,
    previewed: Previewed,
    preview: Preview,
    schedule: Task<()>,
    preview_load: Task<()>,
    paths_load: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<RowEvent> for RuleRow {}

fn kind_choices() -> Vec<SelectItem> {
    RULE_KINDS
        .into_iter()
        .map(|kind| SelectItem::new(kind.as_str(), rule_kind_label(kind)))
        .collect()
}

fn field_choices() -> Vec<SelectItem> {
    PATTERN_FIELDS
        .into_iter()
        .map(|field| SelectItem::new(field.as_str(), pattern_field_label(field)))
        .collect()
}

impl RuleRow {
    fn new(
        id: usize,
        fields: RuleFields,
        sources: Rc<Vec<SourceSuggestion>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let kinds = kind_choices();
        let kind_ix = select_item_index(&kinds, fields.kind.as_str());
        let kind_select =
            cx.new(|cx| SelectState::new(SelectItems::from(kinds), kind_ix, window, cx));
        let field_list = field_choices();
        let field_ix = select_item_index(&field_list, fields.field.as_str());
        let field_select =
            cx.new(|cx| SelectState::new(SelectItems::from(field_list), field_ix, window, cx));
        let value = cx.new(|cx| {
            let mut input = SuggestInput::new(rule_value_placeholder(fields.kind), window, cx);
            input.set_value(fields.value.clone(), window, cx);
            input
        });
        let path = cx.new(|cx| {
            let mut input = SuggestInput::new("/your-org/your-repo", window, cx);
            input.set_value(fields.path.clone(), window, cx);
            input
        });
        let subscriptions = vec![
            cx.subscribe_in(&kind_select, window, |this, _, event, window, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                if let Some(kind) = parse_rule_kind(value) {
                    this.fields.kind = kind;
                    this.value.update(cx, |input, cx| {
                        input.set_placeholder(rule_value_placeholder(kind), window, cx)
                    });
                    this.refresh_sources(cx);
                    this.load_paths(cx);
                    this.changed(cx);
                }
            }),
            cx.subscribe(&field_select, |this, _, event, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                if let Some(field) = parse_pattern_field(value) {
                    this.fields.field = field;
                    this.changed(cx);
                }
            }),
            cx.subscribe(&value, |this, _, SuggestInputEvent::Change(value), cx| {
                this.fields.value = value.to_string();
                this.load_paths(cx);
                this.changed(cx);
            }),
            cx.subscribe(&path, |this, _, SuggestInputEvent::Change(path), cx| {
                this.fields.path = path.to_string();
                this.changed(cx);
            }),
        ];
        let mut row = Self {
            id,
            previewed: (fields.kind, None, String::new()),
            fields,
            removable: false,
            error: None,
            sources,
            kind_select,
            field_select,
            value,
            path,
            preview: Preview::Idle,
            schedule: Task::ready(()),
            preview_load: Task::ready(()),
            paths_load: Task::ready(()),
            _subscriptions: subscriptions,
        };
        row.refresh_sources(cx);
        row.load_paths(cx);
        row.preview(row.fields.previewed(), cx);
        row
    }

    fn set_sources(&mut self, sources: Rc<Vec<SourceSuggestion>>, cx: &mut Context<Self>) {
        self.sources = sources;
        self.refresh_sources(cx);
    }

    fn refresh_sources(&mut self, cx: &mut Context<Self>) {
        let suggestions = if self.fields.kind == RuleKind::Pattern {
            Vec::new()
        } else {
            source_suggestions(&self.sources, self.fields.kind)
        };
        self.value
            .update(cx, |input, cx| input.set_suggestions(suggestions, cx));
    }

    fn load_paths(&mut self, cx: &mut Context<Self>) {
        let domain = self.fields.value.trim().to_lowercase();
        if self.fields.kind != RuleKind::Page || domain.is_empty() {
            self.paths_load = Task::ready(());
            return;
        }
        self.paths_load = data::fetch(
            cx,
            move |core| core.rules.path_suggestions(&domain),
            |this, result, cx| {
                let suggestions = path_suggestions(result.unwrap_or_default());
                this.path
                    .update(cx, |input, cx| input.set_suggestions(suggestions, cx));
            },
        );
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        let next = self.fields.previewed();
        self.schedule = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PREVIEW_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| this.preview(next, cx));
        });
        cx.emit(RowEvent::Changed);
        cx.notify();
    }

    fn preview(&mut self, next: Previewed, cx: &mut Context<Self>) {
        if next == self.previewed && !matches!(self.preview, Preview::Idle) {
            return;
        }
        self.previewed = next.clone();
        let (kind, field, value) = next;
        if value.is_empty() {
            self.preview = Preview::Idle;
            self.preview_load = Task::ready(());
            cx.notify();
            return;
        }
        self.preview = Preview::Checking;
        self.preview_load = data::fetch(
            cx,
            move |core| {
                core.rules
                    .preview(kind, field, &value)
                    .map_err(UiError::from)
            },
            |this, result, _| {
                this.preview = match result {
                    Ok(preview) => Preview::Ready(preview),
                    Err(err) => Preview::Failed(err.to_string()),
                };
            },
        );
        cx.notify();
    }

    fn value_input(&self) -> Entity<SuggestInput> {
        self.value.clone()
    }

    fn render_preview(&self, cx: &App) -> impl IntoElement + use<> {
        let muted = cx.theme().muted_foreground;
        let body = match &self.preview {
            _ if self.previewed.2.is_empty() => div()
                .text_color(muted)
                .child("Type a value to see what it would match.")
                .into_any_element(),
            Preview::Failed(error) => div()
                .text_color(muted)
                .child(error.clone())
                .into_any_element(),
            Preview::Idle | Preview::Checking => div()
                .text_color(muted)
                .child("Checking your history…")
                .into_any_element(),
            Preview::Ready(preview) if preview.activity_count == 0 => div()
                .text_color(muted)
                .child("Nothing tracked so far matches. It will apply to new activity.")
                .into_any_element(),
            Preview::Ready(preview) => v_flex()
                .gap_1()
                .child(preview_summary(preview))
                .children(
                    preview
                        .samples
                        .iter()
                        .map(|sample| div().truncate().text_color(muted).child(sample.clone())),
                )
                .into_any_element(),
        };
        div()
            .min_h_10()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().muted.opacity(0.3))
            .px_3()
            .py_2()
            .text_xs()
            .child(body)
    }
}

impl Render for RuleRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.fields.kind;
        let theme = cx.theme();
        v_flex()
            .min_w_0()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div().flex_1().min_w_0().child(
                            Select::new(&self.kind_select).accessibility_label("What to match"),
                        ),
                    )
                    .when(kind == RuleKind::Pattern, |this| {
                        this.child(div().flex_1().min_w_0().child(
                            Select::new(&self.field_select).accessibility_label("Match against"),
                        ))
                    })
                    .when(self.removable, |this| {
                        this.child(
                            Button::new("remove-rule")
                                .ghost()
                                .xsmall()
                                .icon(IconName::X)
                                .tooltip("Remove rule")
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(RowEvent::Remove))),
                        )
                    }),
            )
            .child(
                v_flex()
                    .gap_1p5()
                    .child(self.value.clone())
                    .when(kind == RuleKind::Page, |this| this.child(self.path.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(value_hint(kind)),
                    ),
            )
            .child(self.render_preview(cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
    }
}

/// What saving the rows did: the rows saved, and the row that stopped it with why.
struct SaveResult {
    saved: Vec<usize>,
    failed: Option<(usize, String)>,
}

struct RuleForm {
    rule: Option<CategoryRuleView>,
    category: Entity<CategoryCombobox>,
    category_id: Option<CategoryId>,
    rows: Vec<Entity<RuleRow>>,
    next_id: usize,
    sources: Rc<Vec<SourceSuggestion>>,
    pending: bool,
    error: Option<String>,
    sources_load: Task<()>,
    save: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl RuleForm {
    fn new(rule: Option<CategoryRuleView>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let category_id = rule.as_ref().map(|rule| rule.category_id);
        let category = cx.new(|cx| {
            CategoryCombobox::new(
                category_id.map(|id| SharedString::from(id.0.to_string())),
                Vec::new(),
                window,
                cx,
            )
            .creatable()
        });
        let subscriptions = vec![cx.subscribe_in(
            &category,
            window,
            |this, combobox, event, window, cx| match event {
                CategoryComboboxEvent::Change(value) => {
                    this.category_id = value.parse().ok().map(CategoryId);
                    cx.notify();
                }
                CategoryComboboxEvent::Create => {
                    let this = cx.entity().downgrade();
                    let combobox = combobox.downgrade();
                    dialogs::open::<AddCategoryDialog>(
                        AddCategoryData::new(None).on_created(move |created, window, cx| {
                            let value = SharedString::from(created.id.0.to_string());
                            let _ = combobox.update(cx, |combobox, cx| {
                                combobox.set_value(Some(value), window, cx)
                            });
                            let _ = this.update(cx, |this, cx| {
                                this.category_id = Some(created.id);
                                cx.notify();
                            });
                        }),
                        window,
                        cx,
                    );
                }
            },
        )];
        let first = rule
            .as_ref()
            .map_or_else(|| RuleFields::empty(None), RuleFields::of);
        let mut form = Self {
            rule,
            category,
            category_id,
            rows: Vec::new(),
            next_id: 0,
            sources: Rc::new(Vec::new()),
            pending: false,
            error: None,
            sources_load: Task::ready(()),
            save: Task::ready(()),
            _subscriptions: subscriptions,
        };
        form.push_row(first, window, cx);
        form.sources_load = data::fetch(
            cx,
            |core| core.rules.source_suggestions(),
            |this, result, cx| {
                this.sources = Rc::new(result.unwrap_or_default());
                let sources = this.sources.clone();
                for row in &this.rows {
                    row.update(cx, |row, cx| row.set_sources(sources.clone(), cx));
                }
            },
        );
        form
    }

    fn push_row(
        &mut self,
        fields: RuleFields,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<RuleRow> {
        let id = self.next_id;
        self.next_id += 1;
        let sources = self.sources.clone();
        let row = cx.new(|cx| RuleRow::new(id, fields, sources, window, cx));
        self._subscriptions
            .push(cx.subscribe(&row, move |this, _, event, cx| match event {
                RowEvent::Changed => cx.notify(),
                RowEvent::Remove => {
                    this.rows.retain(|row| row.read(cx).id != id);
                    this.sync_removable(cx);
                }
            }));
        self.rows.push(row.clone());
        self.sync_removable(cx);
        row
    }

    fn add_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let like = self.rows.last().map(|row| row.read(cx).fields.clone());
        let row = self.push_row(RuleFields::empty(like.as_ref()), window, cx);
        let input = row.read(cx).value_input();
        window.defer(cx, move |window, cx| {
            input.update(cx, |input, cx| input.focus(window, cx))
        });
    }

    fn sync_removable(&mut self, cx: &mut Context<Self>) {
        let removable = self.rule.is_none() && self.rows.len() > 1;
        for row in &self.rows {
            row.update(cx, |row, cx| {
                row.removable = removable;
                cx.notify();
            });
        }
        cx.notify();
    }

    /// The rows with something to save, by id; a blank one is skipped rather than blocking.
    fn filled(&self, cx: &App) -> Vec<(usize, RuleFields)> {
        self.rows
            .iter()
            .map(|row| row.read(cx))
            .filter(|row| !row.fields.match_value().is_empty())
            .map(|row| (row.id, row.fields.clone()))
            .collect()
    }

    fn can_save(&self, cx: &App) -> bool {
        !self.filled(cx).is_empty() && self.category_id.is_some() && !self.pending
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(category_id) = self.category_id else {
            return;
        };
        if !self.can_save(cx) {
            return;
        }
        let drafts: Vec<(usize, RuleDraft)> = self
            .filled(cx)
            .into_iter()
            .map(|(id, fields)| {
                let draft = RuleDraft {
                    kind: fields.kind,
                    field: fields.pattern_field(),
                    value: fields.match_value(),
                    category_id,
                };
                (id, draft)
            })
            .collect();
        let editing = self.rule.as_ref().map(|rule| rule.id);
        self.pending = true;
        self.error = None;
        for row in &self.rows {
            row.update(cx, |row, _| row.error = None);
        }
        self.save = data::fetch_in(
            window,
            cx,
            move |core| -> Result<SaveResult, UiError> {
                if let Some(id) = editing {
                    let (row, draft) = &drafts[0];
                    core.rules.update(id, draft)?;
                    return Ok(SaveResult {
                        saved: vec![*row],
                        failed: None,
                    });
                }
                let mut saved = Vec::new();
                for (row, draft) in &drafts {
                    match core.rules.create(draft) {
                        Ok(_) => saved.push(*row),
                        Err(err) => {
                            return Ok(SaveResult {
                                saved,
                                failed: Some((*row, UiError::from(err).to_string())),
                            });
                        }
                    }
                }
                Ok(SaveResult {
                    saved,
                    failed: None,
                })
            },
            |this, result, window, cx| {
                this.pending = false;
                match result {
                    Ok(SaveResult { failed: None, .. }) => window.close_dialog(cx),
                    // Rules saved before a failure stay saved, so they leave the form: saving
                    // again must not add them twice.
                    Ok(SaveResult {
                        saved,
                        failed: Some((failed, message)),
                    }) => {
                        this.rows.retain(|row| !saved.contains(&row.read(cx).id));
                        if let Some(row) = this.rows.iter().find(|row| row.read(cx).id == failed) {
                            row.update(cx, |row, cx| {
                                row.error = Some(message);
                                cx.notify();
                            });
                        }
                        this.sync_removable(cx);
                    }
                    Err(err) => this.error = Some(err.to_string()),
                }
                AppState::update(cx, |state, cx| state.invalidate_categorization(cx));
            },
        );
        cx.notify();
    }
}

impl Render for RuleForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let editing = self.rule.is_some();
        let description = if editing {
            "Every activity that matches goes to the category you choose, including what you have \
             already tracked."
        } else {
            "Choose a category, then add every rule that belongs in it. Every activity that \
             matches goes there, including what you have already tracked."
        };
        let filled = self.filled(cx).len();
        let can_save = self.can_save(cx);
        let border = theme.border;

        v_flex()
            .min_w_0()
            .gap_4()
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_lg()
                            .font_semibold()
                            .pr_6()
                            .child(if editing { "Edit rule" } else { "Add rules" }),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(format!(
                        "{description} Avoid multi-purpose sites or apps: every matching activity \
                         goes to the category."
                    ))),
            )
            .child(self.category.clone())
            .child(
                v_flex()
                    .min_w_0()
                    .gap_3()
                    .children(self.rows.iter().map(|row| {
                        if editing {
                            div().child(row.clone())
                        } else {
                            div()
                                .rounded_lg()
                                .border_1()
                                .border_color(border)
                                .p_3()
                                .child(row.clone())
                        }
                    })),
            )
            .when(!editing, |this| {
                this.child(
                    div().child(
                        Button::new("add-row")
                            .outline()
                            .small()
                            .icon(IconName::Plus)
                            .label("Add another rule")
                            .on_click(cx.listener(|this, _, window, cx| this.add_row(window, cx))),
                    ),
                )
            })
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("save")
                            .primary()
                            .label(save_label(editing, filled))
                            .disabled(!can_save)
                            .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                    ),
            )
    }
}

pub struct RuleDialog;

impl AppDialog for RuleDialog {
    /// The rule to edit, or `None` to add rules.
    type Data = Option<CategoryRuleView>;

    fn open(rule: Option<CategoryRuleView>, window: &mut Window, cx: &mut App) {
        let form = cx.new(|cx| RuleForm::new(rule, window, cx));
        window.open_dialog(cx, move |dialog, _, _| {
            dialog.w(px(512.)).child(form.clone())
        });
    }
}

#[cfg(test)]
mod tests {
    use makora_lib::shared::ids::RuleId;

    use super::*;

    fn fields(kind: RuleKind, value: &str, path: &str) -> RuleFields {
        RuleFields {
            kind,
            field: PatternField::Url,
            value: value.into(),
            path: path.into(),
        }
    }

    fn rule(kind: RuleKind, value: &str) -> CategoryRuleView {
        CategoryRuleView {
            id: RuleId(1),
            kind,
            field: None,
            value: value.into(),
            category_id: CategoryId(2),
            activity_count: 0,
            total_seconds: 0.,
        }
    }

    #[test]
    fn a_page_joins_domain_and_path() {
        let page = |value, path| fields(RuleKind::Page, value, path).match_value();
        assert_eq!(page(" github.com ", "/acme"), "github.com/acme");
        assert_eq!(page("github.com", "acme/api"), "github.com/acme/api");
        assert_eq!(page("github.com", "  "), "github.com");
        assert_eq!(page("", "/acme"), "");
        assert_eq!(
            fields(RuleKind::Site, " x.com ", "/a").match_value(),
            "x.com"
        );
    }

    #[test]
    fn a_saved_page_splits_at_its_first_slash() {
        let page = RuleFields::of(&rule(RuleKind::Page, "github.com/acme/api"));
        assert_eq!(
            (page.value.as_str(), page.path.as_str()),
            ("github.com", "/acme/api")
        );
        assert_eq!(page.match_value(), "github.com/acme/api");
        let bare = RuleFields::of(&rule(RuleKind::Page, "github.com"));
        assert_eq!(
            (bare.value.as_str(), bare.path.as_str()),
            ("github.com", "")
        );
        let site = RuleFields::of(&rule(RuleKind::Site, "a/b"));
        assert_eq!(site.value, "a/b");
        assert_eq!(site.field, PatternField::Url);
    }

    #[test]
    fn a_new_row_keeps_the_last_kind() {
        let mut last = fields(RuleKind::Pattern, "*x*", "");
        last.field = PatternField::Title;
        let next = RuleFields::empty(Some(&last));
        assert_eq!(
            (next.kind, next.field),
            (RuleKind::Pattern, PatternField::Title)
        );
        assert!(next.value.is_empty());
        assert_eq!(RuleFields::empty(None).kind, RuleKind::Site);
    }

    #[test]
    fn only_a_pattern_has_a_field() {
        assert_eq!(fields(RuleKind::Site, "a", "").pattern_field(), None);
        assert_eq!(
            fields(RuleKind::Pattern, "a", "").pattern_field(),
            Some(PatternField::Url)
        );
    }

    #[test]
    fn hints_name_the_source_when_it_differs() {
        assert_eq!(source_hint("Code", "code", 125.), "Code · 2m 5s");
        assert_eq!(source_hint("code", "code", 5.), "5s");
        assert_eq!(source_hint("", "code", 5.), "5s");
    }

    #[test]
    fn suggestions_follow_the_kind() {
        let sources = vec![
            SourceSuggestion {
                kind: KeyKind::Exe,
                key: "code".into(),
                name: "Code".into(),
                total_seconds: 60.,
            },
            SourceSuggestion {
                kind: KeyKind::Domain,
                key: "github.com".into(),
                name: "github.com".into(),
                total_seconds: 5.,
            },
        ];
        let values = |kind| -> Vec<String> {
            source_suggestions(&sources, kind)
                .into_iter()
                .map(|s| s.value.to_string())
                .collect()
        };
        assert_eq!(values(RuleKind::App), ["code"]);
        assert_eq!(values(RuleKind::Site), ["github.com"]);
        assert_eq!(values(RuleKind::Page), ["github.com"]);
    }

    #[test]
    fn the_summary_counts_pins() {
        let preview = RulePreview {
            activity_count: 1,
            total_seconds: 3600.,
            pinned_count: 0,
            samples: vec![],
        };
        assert_eq!(preview_summary(&preview), "Matches 1 activity · 1h 0m");
        let preview = RulePreview {
            activity_count: 4,
            total_seconds: 30.,
            pinned_count: 2,
            samples: vec![],
        };
        assert_eq!(
            preview_summary(&preview),
            "Matches 4 activities · 30s (2 pinned by you will not change)"
        );
    }

    #[test]
    fn the_save_button_counts_rows() {
        assert_eq!(save_label(true, 1), "Save rule");
        assert_eq!(save_label(false, 0), "Add rule");
        assert_eq!(save_label(false, 1), "Add rule");
        assert_eq!(save_label(false, 3), "Add 3 rules");
    }
}
