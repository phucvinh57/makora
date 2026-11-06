use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Enter, Input, InputEvent, InputState};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, IndexPath, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window, div, px,
};
use makora_lib::categorization::application::rules::SourceSuggestion;
use makora_lib::categorization::domain::rules::{PatternField, RuleKind};
use makora_lib::settings::SettingsSvc;
use makora_lib::settings::{Settings, SettingsResult};
use makora_lib::tracking::domain::exclusion::Exclusion;
use makora_lib::tracking::domain::source_key::KeyKind;
use tracing::warn;

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::error::UiError;
use crate::ui::format::duration::format_seconds;
use crate::ui::format::rules::{
    PATTERN_FIELDS, RULE_KINDS, describe_rule, parse_pattern_field, parse_rule_kind,
    pattern_field_label, rule_kind_label, rule_value_placeholder,
};
use crate::ui::widgets::card::Card;
use crate::ui::widgets::select_item::SelectItem;
use crate::ui::widgets::suggest_input::{SuggestInput, SuggestInputEvent, Suggestion};

const SOURCES_STALE_AFTER: Duration = Duration::from_secs(60);

/// Which list of apps, sites, pages and patterns the section edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionList {
    /// Never recorded.
    Tracking,
    /// Recorded, but sent to a remote AI provider masked: no title or address.
    NeverSend,
}

impl ExclusionList {
    fn title(self) -> &'static str {
        match self {
            Self::Tracking => "Excluded from tracking",
            Self::NeverSend => "Never sent to AI providers",
        }
    }

    fn entries(self, settings: &Settings) -> Vec<Exclusion> {
        match self {
            Self::Tracking => settings.tracking.excluded.clone(),
            Self::NeverSend => settings.ai.privacy.never_send.clone(),
        }
    }

    fn hint(self, kind: RuleKind) -> &'static str {
        match self {
            Self::Tracking => exclusion_hint(kind),
            Self::NeverSend => never_send_hint(kind),
        }
    }

    fn footer(self) -> &'static str {
        match self {
            Self::Tracking => {
                "Matching activity is not recorded from the next time it starts. Already \
                 recorded history is kept."
            }
            Self::NeverSend => {
                "Matching activity is still tracked and can be labeled by the local model. A \
                 remote provider gets it with the title, address, site and listed app hidden, and \
                 never as an example for other activities."
            }
        }
    }

    /// An element id unique to this list, as both lists are on one screen.
    fn id(self, name: &str) -> SharedString {
        let list = match self {
            Self::Tracking => "excluded",
            Self::NeverSend => "never-send",
        };
        format!("{list}-{name}").into()
    }

    fn add_label(self) -> &'static str {
        match self {
            Self::Tracking => "Exclude",
            Self::NeverSend => "Add",
        }
    }

    fn remove_label(self) -> &'static str {
        match self {
            Self::Tracking => "Stop excluding",
            Self::NeverSend => "Allow sending",
        }
    }

    fn add(
        self,
        settings: &dyn SettingsSvc,
        kind: RuleKind,
        field: Option<PatternField>,
        value: &str,
    ) -> SettingsResult {
        match self {
            Self::Tracking => settings.add_exclusion(kind, field, value),
            Self::NeverSend => settings.add_never_send(kind, field, value),
        }
    }

    fn remove(self, settings: &dyn SettingsSvc, exclusion: &Exclusion) -> SettingsResult {
        let (kind, field, value) = (exclusion.kind, exclusion.field, &exclusion.value);
        match self {
            Self::Tracking => settings.remove_exclusion(kind, field, value),
            Self::NeverSend => settings.remove_never_send(kind, field, value),
        }
    }
}

pub fn never_send_hint(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::App => {
            "Activity in this app goes out as [app] with its title hidden. Its browser tabs are matched by site."
        }
        RuleKind::Site => "Every page of this website goes out as [site] with its title hidden.",
        RuleKind::Page => {
            "This page and everything below it goes out as [site] with its title hidden."
        }
        RuleKind::Pattern => "* matches any text and ? matches one character, ignoring case.",
    }
}

pub fn exclusion_hint(kind: RuleKind) -> &'static str {
    match kind {
        RuleKind::App => {
            "Nothing done in this app is recorded, including its browser tabs if it is a browser."
        }
        RuleKind::Site => "Every page of this website is skipped.",
        RuleKind::Page => "This page and everything below it is skipped.",
        RuleKind::Pattern => {
            "* matches any text and ? matches one character, ignoring case. Tracked as the title \
             or address when it starts."
        }
    }
}

/// Apps for an app exclusion, websites for a site one, each with the time recorded for it.
pub fn exclusion_suggestions(kind: RuleKind, sources: &[SourceSuggestion]) -> Vec<Suggestion> {
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
                .hint(format_seconds(source.total_seconds))
                .search(source.name.clone())
        })
        .collect()
}

fn suggests(kind: RuleKind) -> bool {
    matches!(kind, RuleKind::App | RuleKind::Site)
}

pub struct ExclusionsSection {
    list: ExclusionList,
    kind_select: Entity<SelectState<Vec<SelectItem>>>,
    field_select: Entity<SelectState<Vec<SelectItem>>>,
    suggest: Entity<SuggestInput>,
    plain: Entity<InputState>,
    kind: RuleKind,
    field: PatternField,
    value: SharedString,
    sources: Vec<SourceSuggestion>,
    sources_loaded: Option<Instant>,
    adding: bool,
    add_error: Option<String>,
    removing: bool,
    remove_error: Option<String>,
    sources_load: Task<()>,
    add: Task<()>,
    remove: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl ExclusionsSection {
    pub fn new(list: ExclusionList, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let kind = RuleKind::App;
        let field = PatternField::Url;
        let kinds: Vec<SelectItem> = RULE_KINDS
            .into_iter()
            .map(|kind| SelectItem::new(kind.as_str(), rule_kind_label(kind)))
            .collect();
        let fields: Vec<SelectItem> = PATTERN_FIELDS
            .into_iter()
            .map(|field| SelectItem::new(field.as_str(), pattern_field_label(field)))
            .collect();
        let kind_select =
            cx.new(|cx| SelectState::new(kinds, Some(IndexPath::default()), window, cx));
        let field_select =
            cx.new(|cx| SelectState::new(fields, Some(IndexPath::default()), window, cx));
        let suggest = cx.new(|cx| SuggestInput::new(rule_value_placeholder(kind), window, cx));
        let plain = cx.new(|cx| InputState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(&kind_select, window, Self::on_kind_selected),
            cx.subscribe_in(&field_select, window, Self::on_field_selected),
            cx.subscribe(&suggest, |this, _, SuggestInputEvent::Change(value), cx| {
                this.set_value(value.clone(), cx)
            }),
            cx.subscribe_in(&plain, window, |this, input, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let value = input.read(cx).value();
                    this.set_value(value, cx);
                }
            }),
            cx.observe(&AppState::entity(cx), |_, _, cx| cx.notify()),
        ];
        let mut this = Self {
            list,
            kind_select,
            field_select,
            suggest,
            plain,
            kind,
            field,
            value: SharedString::default(),
            sources: Vec::new(),
            sources_loaded: None,
            adding: false,
            add_error: None,
            removing: false,
            remove_error: None,
            sources_load: Task::ready(()),
            add: Task::ready(()),
            remove: Task::ready(()),
            _subscriptions: subscriptions,
        };
        this.load_sources(cx);
        this
    }

    fn load_sources(&mut self, cx: &mut Context<Self>) {
        self.sources_loaded = Some(Instant::now());
        self.sources_load = data::fetch(
            cx,
            |core| core.rules.source_suggestions().map_err(UiError::from),
            |this, result, cx| match result {
                Ok(sources) => {
                    this.sources = sources;
                    this.refresh_suggestions(cx);
                }
                Err(err) => {
                    this.sources_loaded = None;
                    warn!(error = %err, "failed to load the source suggestions");
                }
            },
        );
    }

    fn refresh_suggestions(&mut self, cx: &mut Context<Self>) {
        let suggestions = exclusion_suggestions(self.kind, &self.sources);
        self.suggest
            .update(cx, |input, cx| input.set_suggestions(suggestions, cx));
    }

    fn set_value(&mut self, value: SharedString, cx: &mut Context<Self>) {
        self.value = value;
        self.add_error = None;
        cx.notify();
    }

    fn on_kind_selected(
        &mut self,
        _: &Entity<SelectState<Vec<SelectItem>>>,
        event: &SelectEvent<Vec<SelectItem>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let SelectEvent::Confirm(Some(value)) = event else {
            return;
        };
        let Some(kind) = parse_rule_kind(value) else {
            return;
        };
        if kind == self.kind {
            return;
        }
        let switched_input = suggests(kind) != suggests(self.kind);
        self.kind = kind;
        self.add_error = None;
        let value = self.value.clone();
        let placeholder = rule_value_placeholder(kind);
        if suggests(kind) {
            self.suggest.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
                if switched_input {
                    input.set_value(value, window, cx);
                }
            });
            self.refresh_suggestions(cx);
            if self
                .sources_loaded
                .is_none_or(|loaded| loaded.elapsed() > SOURCES_STALE_AFTER)
            {
                self.load_sources(cx);
            }
        } else {
            self.plain.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx);
                if switched_input {
                    input.set_value(value, window, cx);
                }
            });
        }
        cx.notify();
    }

    fn on_field_selected(
        &mut self,
        _: &Entity<SelectState<Vec<SelectItem>>>,
        event: &SelectEvent<Vec<SelectItem>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let SelectEvent::Confirm(Some(value)) = event
            && let Some(field) = parse_pattern_field(value)
        {
            self.field = field;
            cx.notify();
        }
    }

    fn can_add(&self) -> bool {
        !self.value.trim().is_empty() && !self.adding
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_add() {
            return;
        }
        let kind = self.kind;
        let field = (kind == RuleKind::Pattern).then_some(self.field);
        let value = self.value.to_string();
        let list = self.list;
        self.adding = true;
        cx.notify();
        self.add = data::fetch_in(
            window,
            cx,
            move |core| {
                list.add(&*core.settings, kind, field, &value)
                    .map_err(UiError::from)
            },
            |this, result, window, cx| {
                this.adding = false;
                match result {
                    Ok(settings) => {
                        AppState::update(cx, |state, cx| state.set_settings(settings, cx));
                        this.clear_value(window, cx);
                    }
                    Err(err) => this.add_error = Some(err.to_string()),
                }
            },
        );
    }

    fn clear_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.value = SharedString::default();
        self.suggest
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.plain
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn remove(&mut self, exclusion: Exclusion, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.list;
        self.removing = true;
        self.remove_error = None;
        cx.notify();
        self.remove = data::fetch_in(
            window,
            cx,
            move |core| {
                list.remove(&*core.settings, &exclusion)
                    .map_err(UiError::from)
            },
            |this, result, _, cx| {
                this.removing = false;
                match result {
                    Ok(settings) => {
                        AppState::update(cx, |state, cx| state.set_settings(settings, cx))
                    }
                    Err(err) => this.remove_error = Some(err.to_string()),
                }
            },
        );
    }

    fn render_list(&self, excluded: &[Exclusion], cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
        let list = self.list;
        v_flex()
            .rounded_md()
            .border_1()
            .border_color(border)
            .children(excluded.iter().enumerate().map(|(ix, exclusion)| {
                let target = exclusion.clone();
                h_flex()
                    .justify_between()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .text_sm()
                    .when(ix > 0, |this| this.border_t_1().border_color(border))
                    .child(div().min_w_0().truncate().child(describe_rule(
                        exclusion.kind,
                        exclusion.field,
                        &exclusion.value,
                    )))
                    .child(
                        Button::new((list.id("remove"), ix))
                            .ghost()
                            .xsmall()
                            .icon(Icon::new(IconName::X))
                            .tooltip(self.list.remove_label())
                            .accessibility_label(format!(
                                "{} {}",
                                self.list.remove_label(),
                                exclusion.value
                            ))
                            .disabled(self.removing)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.remove(target.clone(), window, cx)
                            })),
                    )
            }))
    }
}

impl Render for ExclusionsSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let danger = cx.theme().danger;
        let list = self.list;
        let excluded = list.entries(&AppState::read(cx).settings);
        let kind = self.kind;

        Card::new().small().title(list.title()).child(
            v_flex()
                .gap_3()
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            h_flex()
                                .gap_2()
                                .on_action(cx.listener(|this, _: &Enter, window, cx| {
                                    this.submit(window, cx)
                                }))
                                .child(
                                    div().w(px(128.)).flex_shrink_0().child(
                                        Select::new(&self.kind_select)
                                            .accessibility_label("What to exclude"),
                                    ),
                                )
                                .when(kind == RuleKind::Pattern, |this| {
                                    this.child(
                                        div().w(px(160.)).flex_shrink_0().child(
                                            Select::new(&self.field_select)
                                                .accessibility_label("Match against"),
                                        ),
                                    )
                                })
                                .child(div().flex_1().min_w_0().map(|this| {
                                    if suggests(kind) {
                                        this.child(self.suggest.clone())
                                    } else {
                                        this.child(Input::new(&self.plain).aria_label(
                                            if kind == RuleKind::Page {
                                                "Page"
                                            } else {
                                                "Pattern"
                                            },
                                        ))
                                    }
                                }))
                                .child(
                                    Button::new(list.id("add"))
                                        .small()
                                        .primary()
                                        .icon(Icon::new(IconName::Plus))
                                        .label(list.add_label())
                                        .disabled(!self.can_add())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.submit(window, cx)
                                        })),
                                ),
                        )
                        .child(div().text_xs().text_color(muted).child(list.hint(kind)))
                        .when_some(self.add_error.clone(), |this, error| {
                            this.child(div().text_xs().text_color(danger).child(error))
                        }),
                )
                .when(!excluded.is_empty(), |this| {
                    this.child(self.render_list(&excluded, cx))
                })
                .child(div().text_xs().text_color(muted).child(list.footer()))
                .when_some(self.remove_error.clone(), |this, error| {
                    this.child(div().text_xs().text_color(danger).child(error))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(kind: KeyKind, key: &str, name: &str, total_seconds: f64) -> SourceSuggestion {
        SourceSuggestion {
            kind,
            key: key.into(),
            name: name.into(),
            total_seconds,
        }
    }

    #[test]
    fn suggestions_follow_the_kind() {
        let sources = vec![
            source(KeyKind::Exe, "code", "Visual Studio Code", 7500.0),
            source(KeyKind::Domain, "github.com", "GitHub", 65.0),
        ];
        let apps = exclusion_suggestions(RuleKind::App, &sources);
        assert_eq!(
            apps,
            [Suggestion::new("code")
                .hint("2h 5m")
                .search("Visual Studio Code")]
        );
        let sites = exclusion_suggestions(RuleKind::Site, &sources);
        assert_eq!(
            sites,
            [Suggestion::new("github.com").hint("1m 5s").search("GitHub")]
        );
    }

    #[test]
    fn only_apps_and_sites_suggest() {
        let suggesting: Vec<bool> = RULE_KINDS.into_iter().map(suggests).collect();
        assert_eq!(suggesting, [true, true, false, false]);
    }

    #[test]
    fn every_kind_has_a_hint() {
        assert!(exclusion_hint(RuleKind::Pattern).starts_with("* matches any text"));
        assert_eq!(
            exclusion_hint(RuleKind::Site),
            "Every page of this website is skipped."
        );
    }
}
