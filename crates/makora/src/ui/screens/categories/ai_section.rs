//! The AI's configuration and its run controls: which models it uses and where the language model
//! runs, how often it runs on its own, and running, stopping and re-checking it.

use std::time::Duration;

use chrono::{DateTime, Utc};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::select::{Select, SelectEvent, SelectState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, Task, Window, div, px,
};
use makora_lib::ai::llm::ModelRole;
use makora_lib::categorization::application::ai_run::{AiRunOutcome, RunMode, RunRequest};
use makora_lib::settings::{AUTO_CATEGORIZE_HOURS_OPTIONS, AiSettings, ProviderConfig, Settings};

use crate::ui::app_state::AppState;
use crate::ui::data;
use crate::ui::dialogs::add_model::{AddModelData, AddModelDialog};
use crate::ui::dialogs::reset_categorization::ResetCategorizationDialog;
use crate::ui::dialogs::{self};
use crate::ui::error::UiError;
use crate::ui::format::ai_usage::describe_run_usage;
use crate::ui::format::date_range::to_iso_string;
use crate::ui::format::duration::format_timestamp;
use crate::ui::widgets::card::Card;
use crate::ui::widgets::select_item::{SelectItem, SelectItems, select_item_index};
use crate::ui::widgets::suggest_input::{SuggestInput, Suggestion};
use crate::ui::widgets::toast::Toast;

const ADD_MODEL: &str = "__add_model__";
const LOCAL_PROVIDER: &str = "__local__";
const DEFAULT_HOURS: u32 = 24;
const NEXT_RUN_POLL: Duration = Duration::from_secs(60);

pub fn period_label(hours: u32) -> String {
    match hours {
        24 => "day".into(),
        72 => "3 days".into(),
        168 => "week".into(),
        hours => format!("{hours} hours"),
    }
}

/// date-fns' `formatDistance` in English, with its "in …" / "… ago" suffix.
pub fn distance_to_now(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    const DAY: i64 = 1440;
    const MONTH: i64 = 43_200;
    let seconds = (at - now).num_seconds();
    let minutes = (seconds.abs() as f64 / 60.).round() as i64;
    let plural = |count: i64, one: &str, many: &str| {
        if count == 1 {
            one.to_string()
        } else {
            many.replace("{}", &count.to_string())
        }
    };
    let words = if minutes < 2 {
        if minutes == 0 {
            "less than a minute".to_string()
        } else {
            "1 minute".to_string()
        }
    } else if minutes < 45 {
        format!("{minutes} minutes")
    } else if minutes < 90 {
        "about 1 hour".to_string()
    } else if minutes < DAY {
        plural(
            (minutes as f64 / 60.).round() as i64,
            "about 1 hour",
            "about {} hours",
        )
    } else if minutes < 2520 {
        "1 day".to_string()
    } else if minutes < MONTH {
        plural(
            (minutes as f64 / DAY as f64).round() as i64,
            "1 day",
            "{} days",
        )
    } else {
        plural(
            (minutes as f64 / MONTH as f64).round() as i64,
            "about 1 month",
            "about {} months",
        )
    };
    if seconds >= 0 {
        format!("in {words}")
    } else {
        format!("{words} ago")
    }
}

pub fn describe_next_run(next: DateTime<Utc>, now: DateTime<Utc>, running: bool) -> String {
    if running {
        return "A run is in progress.".into();
    }
    if (next - now).num_milliseconds() < 60_000 {
        return "Next run: any moment now.".into();
    }
    format!(
        "Next run: {} ({}).",
        format_timestamp(&to_iso_string(next)),
        distance_to_now(next, now)
    )
}

/// The toast after a run: its title and what it did.
pub fn run_summary(outcome: &AiRunOutcome, mode: RunMode) -> (&'static str, String) {
    let title = match mode {
        _ if outcome.stopped => "AI categorization stopped",
        RunMode::Recheck => "AI labels re-checked",
        RunMode::Label => "AI categorization finished",
        RunMode::FindCategories => "New categories",
    };
    let mut counts = match mode {
        RunMode::Recheck => format!(
            "{} labels changed, {} cleared as uncertain.",
            outcome.changed, outcome.skipped
        ),
        RunMode::Label => format!(
            "{} activities labeled, {} left uncategorized.",
            outcome.labeled, outcome.skipped
        ),
        RunMode::FindCategories => String::new(),
    };
    let found = match outcome.suggested {
        0 if mode == RunMode::FindCategories => Some("No new groups of activities found.".to_string()),
        0 => None,
        1 => Some("1 new category suggested.".to_string()),
        n => Some(format!("{n} new categories suggested.")),
    };
    if let Some(found) = found {
        if !counts.is_empty() {
            counts.push(' ');
        }
        counts.push_str(&found);
    }
    let description = match describe_run_usage(&outcome.usage) {
        Some(usage) => format!("{counts} {usage}"),
        None => counts,
    };
    (title, description)
}

/// Whether the models a run needs are there, and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    pub ready: bool,
    pub blocker: Option<String>,
}

/// `installed` is `None` while the list is loading.
pub fn readiness(settings: &Settings, installed: Option<&[(String, ModelRole)]>) -> Readiness {
    let ai = &settings.ai;
    let installed_as = |role: ModelRole, name: &str| {
        installed.is_some_and(|models| {
            models
                .iter()
                .any(|(model, model_role)| *model_role == role && model == name)
        })
    };
    let saved_provider = ai.llm_provider.as_deref().and_then(|id| ai.provider(id));
    let llm_ready = match &ai.llm_provider {
        Some(_) => saved_provider.is_some(),
        None => installed_as(ModelRole::Llm, &ai.llm_model),
    };
    let embedding_ready = installed_as(ModelRole::Embedding, &ai.embedding_model);
    let blocker = if installed.is_none() {
        None
    } else if ai.llm_provider.is_some() && saved_provider.is_none() {
        Some("The selected provider no longer exists. Choose another.".into())
    } else if !llm_ready {
        Some(format!(
            "Language model {} is not installed. Add it from the menu above.",
            ai.llm_model
        ))
    } else if !embedding_ready {
        Some(format!(
            "Embedding model {} is not installed. Add it from the menu above.",
            ai.embedding_model
        ))
    } else {
        None
    };
    Readiness {
        ready: llm_ready && embedding_ready,
        blocker,
    }
}

/// The installed names for a model select, with the current one first when it is missing.
pub fn model_choices(value: &str, installed: &[String]) -> Vec<SelectItem> {
    let mut names: Vec<&str> = installed.iter().map(String::as_str).collect();
    if !installed.iter().any(|name| name == value) {
        names.insert(0, value);
    }
    names
        .into_iter()
        .map(|name| {
            let label = if installed.iter().any(|model| model == name) {
                name.to_string()
            } else {
                format!("{name} (not installed)")
            };
            SelectItem::new(name.to_string(), label)
        })
        .chain([SelectItem::new(ADD_MODEL, "Add model").icon(IconName::Plus)])
        .collect()
}

fn provider_choices(providers: &[ProviderConfig]) -> Vec<SelectItem> {
    [SelectItem::new(LOCAL_PROVIDER, "Local (on this machine)")]
        .into_iter()
        .chain(
            providers
                .iter()
                .map(|provider| SelectItem::new(provider.id.clone(), provider.name.clone())),
        )
        .collect()
}

fn period_choices() -> Vec<SelectItem> {
    AUTO_CATEGORIZE_HOURS_OPTIONS
        .into_iter()
        .map(|hours| SelectItem::new(hours.to_string(), period_label(hours)))
        .collect()
}

type Select_ = Entity<SelectState<SelectItems>>;

/// What the selects show; they are rebuilt only when it changes.
#[derive(PartialEq)]
struct SyncKey {
    embedding_model: String,
    llm_model: String,
    llm_provider: Option<String>,
    hours: u32,
    providers: Vec<ProviderConfig>,
    installed: Option<Vec<(String, ModelRole)>>,
    pending_provider: Option<String>,
}

impl SyncKey {
    fn of(
        ai: &AiSettings,
        installed: &Option<Vec<(String, ModelRole)>>,
        pending_provider: &Option<String>,
    ) -> Self {
        Self {
            embedding_model: ai.embedding_model.clone(),
            llm_model: ai.llm_model.clone(),
            llm_provider: ai.llm_provider.clone(),
            hours: ai.auto_categorize_hours,
            providers: ai.providers.clone(),
            installed: installed.clone(),
            pending_provider: pending_provider.clone(),
        }
    }
}

pub struct AiSection {
    installed: Option<Vec<(String, ModelRole)>>,
    pending_provider: Option<String>,
    pending_review: Option<i64>,
    setting_model: bool,
    /// While the run call is out: whether it is a re-check.
    stopping: bool,
    saving_period: bool,
    period_error: Option<String>,
    next_run: Option<DateTime<Utc>>,
    next_run_inputs: Option<(u32, bool)>,
    /// What the selects last showed; they are only rebuilt when it changes, so an open menu
    /// survives the progress updates.
    synced: Option<SyncKey>,
    was_downloading: bool,
    installed_revision: Option<u64>,
    remote_models: Option<Result<Vec<String>, String>>,
    remote_provider: Option<String>,
    remote_typed: String,
    embedding: Select_,
    provider: Select_,
    llm: Select_,
    period: Select_,
    remote: Entity<SuggestInput>,
    model_save: Task<()>,
    period_save: Task<()>,
    stop_task: Task<()>,
    installed_load: Task<()>,
    next_run_load: Task<()>,
    models_load: Task<()>,
    _polls: Vec<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl AiSection {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let select = |items: Vec<SelectItem>, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| SelectState::new(SelectItems::from(items), None, window, cx))
        };
        let embedding = select(Vec::new(), window, cx);
        let provider = select(Vec::new(), window, cx);
        let llm = select(Vec::new(), window, cx);
        let period = select(period_choices(), window, cx);
        let remote = cx.new(|cx| SuggestInput::new("Model id", window, cx));
        let subscriptions = vec![
            cx.observe_in(&AppState::entity(cx), window, |this, _, window, cx| {
                this.on_app_state(window, cx)
            }),
            cx.subscribe_in(&embedding, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.pick_model(ModelRole::Embedding, value.to_string(), window, cx);
                }
            }),
            cx.subscribe_in(&llm, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    this.pick_model(ModelRole::Llm, value.to_string(), window, cx);
                }
            }),
            cx.subscribe_in(&provider, window, |this, _, event, window, cx| {
                if let SelectEvent::Confirm(Some(value)) = event {
                    let provider = (value.as_ref() != LOCAL_PROVIDER).then(|| value.to_string());
                    this.choose_provider(provider, window, cx);
                }
            }),
            cx.subscribe(&period, |this, _, event, cx| {
                if let SelectEvent::Confirm(Some(value)) = event
                    && let Ok(hours) = value.parse()
                {
                    this.save_period(hours, cx);
                }
            }),
            cx.subscribe(&remote, |this, _, event, cx| {
                let crate::ui::widgets::suggest_input::SuggestInputEvent::Change(value) = event;
                this.remote_typed = value.to_string();
                cx.notify();
            }),
        ];
        let polls = vec![
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(NEXT_RUN_POLL).await;
                    let polled = this.update(cx, |this, cx| {
                        this.next_run_inputs = None;
                        this.load_next_run(cx);
                    });
                    if polled.is_err() {
                        break;
                    }
                }
            }),
        ];
        let mut section = Self {
            installed: None,
            pending_provider: None,
            pending_review: None,
            setting_model: false,
            stopping: false,
            saving_period: false,
            period_error: None,
            next_run: None,
            next_run_inputs: None,
            synced: None,
            was_downloading: AppState::read(cx).download.running,
            installed_revision: None,
            remote_models: None,
            remote_provider: None,
            remote_typed: String::new(),
            embedding,
            provider,
            llm,
            period,
            remote,
            model_save: Task::ready(()),
            period_save: Task::ready(()),
            stop_task: Task::ready(()),
            installed_load: Task::ready(()),
            next_run_load: Task::ready(()),
            models_load: Task::ready(()),
            _polls: polls,
            _subscriptions: subscriptions,
        };
        section.on_app_state(window, cx);
        section
    }

    pub fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.installed_revision = None;
        self.next_run_inputs = None;
        self.on_app_state(window, cx);
    }

    pub fn set_pending_review(&mut self, pending: Option<i64>, cx: &mut Context<Self>) {
        self.pending_review = pending;
        cx.notify();
    }

    fn on_app_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = AppState::read(cx);
        let revision = state.revisions.installed_models;
        let downloading = state.download.running;
        if self.installed_revision != Some(revision) {
            self.installed_revision = Some(revision);
            self.installed_load = data::fetch_in(
                window,
                cx,
                |core| {
                    core.downloads
                        .installed()
                        .into_iter()
                        .map(|model| (model.name, model.role))
                        .collect::<Vec<_>>()
                },
                |this, installed, window, cx| {
                    this.installed = Some(installed);
                    this.sync_selects(window, cx);
                },
            );
        }
        // A download keeps going in the core while this screen is away. Reopen the dialog when
        // one starts, so its progress bar and Stop button are reachable again.
        if downloading && !self.was_downloading && !AddModelDialog::is_open(cx) {
            let section = cx.entity().downgrade();
            dialogs::open::<AddModelDialog>(
                AddModelData {
                    on_downloaded: Some(std::rc::Rc::new(move |role, model, window, cx| {
                        let _ = section.update(cx, |this, cx| {
                            this.apply_model(role, model.to_string(), window, cx)
                        });
                    })),
                    ..AddModelData::default()
                },
                window,
                cx,
            );
        }
        self.was_downloading = downloading;
        self.sync_selects(window, cx);
        self.load_next_run(cx);
    }

    fn settings(cx: &App) -> &Settings {
        &AppState::read(cx).settings
    }

    fn saved_provider(cx: &App) -> Option<ProviderConfig> {
        let ai = &Self::settings(cx).ai;
        ai.llm_provider
            .as_deref()
            .and_then(|id| ai.provider(id))
            .cloned()
    }

    fn shown_provider(&self, cx: &App) -> Option<ProviderConfig> {
        let ai = &Self::settings(cx).ai;
        let id = self
            .pending_provider
            .clone()
            .or_else(|| ai.llm_provider.clone())?;
        ai.provider(&id).cloned()
    }

    fn installed_names(&self, role: ModelRole) -> Vec<String> {
        self.installed
            .iter()
            .flatten()
            .filter(|(_, model_role)| *model_role == role)
            .map(|(name, _)| name.clone())
            .collect()
    }

    fn sync_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ai = Self::settings(cx).ai.clone();
        let key = SyncKey::of(&ai, &self.installed, &self.pending_provider);
        if self.synced.as_ref() == Some(&key) {
            return;
        }
        self.synced = Some(key);
        let set = |select: &Select_,
                   items: Vec<SelectItem>,
                   value: Option<&str>,
                   window: &mut Window,
                   cx: &mut Context<Self>| {
            let index = value.and_then(|value| select_item_index(&items, value));
            select.update(cx, |select, cx| {
                select.set_items(SelectItems::from(items), window, cx);
                select.set_selected_index(index, window, cx);
            });
        };
        let embedding = model_choices(
            &ai.embedding_model,
            &self.installed_names(ModelRole::Embedding),
        );
        set(
            &self.embedding,
            embedding,
            Some(&ai.embedding_model),
            window,
            cx,
        );
        let llm = model_choices(&ai.llm_model, &self.installed_names(ModelRole::Llm));
        set(&self.llm, llm, Some(&ai.llm_model), window, cx);
        let shown = self.shown_provider(cx);
        let provider_value = shown
            .as_ref()
            .map_or(LOCAL_PROVIDER, |p| p.id.as_str())
            .to_string();
        set(
            &self.provider,
            provider_choices(&ai.providers),
            Some(&provider_value),
            window,
            cx,
        );
        let hours = ai.auto_categorize_hours;
        let period = (hours > 0).then(|| hours.to_string());
        set(
            &self.period,
            period_choices(),
            period.as_deref(),
            window,
            cx,
        );
        self.sync_remote(shown, window, cx);
    }

    /// The remote model field starts over for each provider shown, filled with the saved model
    /// when it is the saved provider.
    fn sync_remote(
        &mut self,
        shown: Option<ProviderConfig>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = shown.as_ref().map(|provider| provider.id.clone());
        if id == self.remote_provider {
            return;
        }
        self.remote_provider = id.clone();
        self.remote_models = None;
        let Some(id) = id else {
            self.models_load = Task::ready(());
            return;
        };
        let saved = Self::saved_provider(cx).is_some_and(|saved| saved.id == id);
        let value = if saved {
            Self::settings(cx).ai.llm_model.clone()
        } else {
            String::new()
        };
        self.remote_typed = value.clone();
        self.remote.update(cx, |input, cx| {
            input.set_value(value, window, cx);
            input.set_suggestions(Vec::new(), cx);
            input.set_placeholder("Loading models…", window, cx);
        });
        self.models_load = data::fetch_in(
            window,
            cx,
            move |core| {
                core.providers
                    .list_models(&id)
                    .map_err(|err| UiError::from(err).to_string())
            },
            |this, result, window, cx| {
                let suggestions = result
                    .as_ref()
                    .map(|models| {
                        models
                            .iter()
                            .map(|model| Suggestion::new(model.clone()))
                            .collect()
                    })
                    .unwrap_or_default();
                this.remote.update(cx, |input, cx| {
                    input.set_suggestions(suggestions, cx);
                    input.set_placeholder("Model id", window, cx);
                });
                this.remote_models = Some(result);
            },
        );
    }

    fn load_next_run(&mut self, cx: &mut Context<Self>) {
        let inputs = (
            Self::settings(cx).ai.auto_categorize_hours,
            self.running(cx),
        );
        if self.next_run_inputs == Some(inputs) {
            return;
        }
        self.next_run_inputs = Some(inputs);
        self.next_run_load = data::fetch(
            cx,
            |core| core.schedule.next_run(Utc::now()),
            |this, next, _| this.next_run = next,
        );
    }

    fn running(&self, cx: &App) -> bool {
        AppState::read(cx).categorize.running
    }

    fn downloading(cx: &App) -> bool {
        AppState::read(cx).download.running
    }

    fn models_disabled(&self, cx: &App) -> bool {
        self.running(cx) || Self::downloading(cx) || self.setting_model || self.installed.is_none()
    }

    fn pick_model(
        &mut self,
        role: ModelRole,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if value != ADD_MODEL {
            self.apply_model(role, value, window, cx);
            return;
        }
        self.synced = None;
        self.sync_selects(window, cx);
        self.open_add_model(Some(role), window, cx);
    }

    fn open_add_model(
        &mut self,
        role: Option<ModelRole>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let section = cx.entity().downgrade();
        dialogs::open::<AddModelDialog>(
            AddModelData {
                role,
                model: None,
                on_downloaded: Some(std::rc::Rc::new(move |role, model, window, cx| {
                    let _ = section.update(cx, |this, cx| {
                        this.apply_model(role, model.to_string(), window, cx)
                    });
                })),
            },
            window,
            cx,
        );
    }

    fn apply_model(
        &mut self,
        role: ModelRole,
        model: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_model(role, model, None, window, cx);
    }

    fn set_model(
        &mut self,
        role: ModelRole,
        model: String,
        provider: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.setting_model = true;
        self.model_save = data::fetch_in(
            window,
            cx,
            move |core| {
                let result = match role {
                    ModelRole::Llm => core.settings.set_llm(provider.as_deref(), &model),
                    ModelRole::Embedding => core.settings.set_embedding_model(&model),
                };
                (result.map_err(UiError::from), core.settings.get())
            },
            move |this, (result, settings), window, cx| {
                this.setting_model = false;
                this.synced = None;
                match result {
                    Ok(_) => {
                        if role == ModelRole::Llm {
                            this.pending_provider = None;
                        }
                    }
                    Err(err) => Toast::error(err.to_string()).show(window, cx),
                }
                AppState::update(cx, |state, cx| state.set_settings(settings, cx));
            },
        );
        cx.notify();
    }

    fn switch_to_local(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pending_provider = None;
        let ai = Self::settings(cx).ai.clone();
        self.synced = None;
        if ai.llm_provider.is_none() {
            self.sync_selects(window, cx);
            return;
        }
        let installed = self.installed_names(ModelRole::Llm);
        let model = if installed.contains(&ai.llm_model) {
            Some(ai.llm_model.clone())
        } else {
            installed.first().cloned()
        };
        self.sync_selects(window, cx);
        match model {
            Some(model) => self.apply_model(ModelRole::Llm, model, window, cx),
            None => self.open_add_model(Some(ModelRole::Llm), window, cx),
        }
    }

    /// `None` is the local runtime. A remote provider only takes effect once a model is chosen.
    fn choose_provider(
        &mut self,
        provider: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(provider) = provider else {
            self.switch_to_local(window, cx);
            return;
        };
        let saved = Self::settings(cx).ai.llm_provider.clone();
        self.pending_provider = (Some(&provider) != saved.as_ref()).then_some(provider);
        self.sync_selects(window, cx);
        cx.notify();
    }

    fn apply_remote(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(provider) = self.shown_provider(cx) else {
            return;
        };
        let model = self.remote_typed.trim().to_string();
        if !self.can_apply_remote(cx) {
            return;
        }
        self.set_model(ModelRole::Llm, model, Some(provider.id), window, cx);
    }

    fn remote_value(&self, cx: &App) -> String {
        let shown = self.shown_provider(cx);
        let saved = Self::saved_provider(cx);
        match (shown, saved) {
            (Some(shown), Some(saved)) if shown.id == saved.id => {
                Self::settings(cx).ai.llm_model.clone()
            }
            _ => String::new(),
        }
    }

    fn can_apply_remote(&self, cx: &App) -> bool {
        let typed = self.remote_typed.trim();
        !self.models_disabled(cx) && !typed.is_empty() && typed != self.remote_value(cx)
    }

    fn save_period(&mut self, hours: u32, cx: &mut Context<Self>) {
        self.saving_period = true;
        self.period_error = None;
        self.period_save = data::fetch(
            cx,
            move |core| {
                core.settings
                    .set_auto_categorize_hours(hours)
                    .map_err(UiError::from)
            },
            |this, result, cx| {
                this.saving_period = false;
                this.synced = None;
                match result {
                    Ok(settings) => {
                        AppState::update(cx, |state, cx| state.set_settings(settings, cx))
                    }
                    Err(err) => this.period_error = Some(err.to_string()),
                }
            },
        );
        cx.notify();
    }

    fn run(&mut self, request: RunRequest, cx: &mut Context<Self>) {
        AppState::update(cx, |state, cx| state.start_run(request, cx));
        cx.notify();
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.stopping = true;
        self.stop_task = data::fetch(
            cx,
            |core| core.runner.cancel(),
            |this, _, _| this.stopping = false,
        );
        cx.notify();
    }

    fn render_auto(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let hours = Self::settings(cx).ai.auto_categorize_hours;
        let enabled = hours > 0;
        let running = self.running(cx);
        let theme = cx.theme();
        let next = self
            .next_run
            .filter(|_| enabled)
            .map(|next| describe_next_run(next, Utc::now(), running));

        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Switch::new("auto-categorize")
                            .checked(enabled)
                            .disabled(self.saving_period)
                            .accessibility_label("Categorize automatically")
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.save_period(if *on { DEFAULT_HOURS } else { 0 }, cx)
                            })),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_medium()
                            .child("Categorize and remind me to review every"),
                    )
                    .child(
                        div().w(px(112.)).child(
                            Select::new(&self.period)
                                .small()
                                .disabled(!enabled || self.saving_period)
                                .accessibility_label("Period between automatic runs"),
                        ),
                    ),
            )
            .when_some(next, |this, next| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(next),
                )
            })
            .when_some(self.period_error.clone(), |this, error| {
                this.child(div().text_xs().text_color(theme.danger).child(error))
            })
    }

    fn render_llm(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let disabled = self.models_disabled(cx);
        match self.shown_provider(cx) {
            Some(provider) => {
                let can_apply = self.can_apply_remote(cx);
                let error = match &self.remote_models {
                    Some(Err(err)) => {
                        Some(format!("Could not list {}'s models: {err}", provider.name))
                    }
                    _ => None,
                };
                labeled(
                    "Language model",
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex()
                                .gap_2()
                                .child(div().w(px(288.)).child(self.remote.clone()))
                                .child(
                                    Button::new("use-model")
                                        .outline()
                                        .small()
                                        .label("Use")
                                        .disabled(!can_apply)
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.apply_remote(window, cx)
                                        })),
                                ),
                        )
                        .when_some(error, |this, error| {
                            this.child(div().text_xs().text_color(cx.theme().danger).child(error))
                        }),
                )
                .into_any_element()
            }
            None => labeled(
                "Language model",
                div().w(px(288.)).child(
                    Select::new(&self.llm)
                        .small()
                        .disabled(disabled)
                        .accessibility_label("Language model"),
                ),
            )
            .into_any_element(),
        }
    }
}

fn labeled(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xs().font_medium().child(label))
        .child(control)
}

impl Render for AiSection {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = AppState::read(cx);
        let settings = state.settings.clone();
        let status = state.categorize.clone();
        let downloading = state.download.running;
        let categories_loaded = state.categories_loaded;
        let no_categories = state.has_no_categories();
        let disabled = self.models_disabled(cx);
        let running = self.running(cx);
        let rechecking = running && status.mode == RunMode::Recheck;
        let run_error = state.run_error.clone();
        let progress = (status.running && status.total > 0).then_some((status.done, status.total));
        let saved_provider = Self::saved_provider(cx);
        let shown_provider = self.shown_provider(cx);
        let run_label = |idle: &'static str, active: bool| -> SharedString {
            if !active {
                return idle.into();
            }
            match progress {
                Some((done, total)) => format!("Categorizing {done}/{total}…").into(),
                None if saved_provider.is_some() => "Starting…".into(),
                None => "Loading models…".into(),
            }
        };
        let readiness = readiness(&settings, self.installed.as_deref());
        let choosing = shown_provider
            .as_ref()
            .is_some_and(|shown| Some(&shown.id) != settings.ai.llm_provider.as_ref());
        let can_run = !running
            && !downloading
            && readiness.ready
            && categories_loaded
            && !no_categories
            && !choosing;
        let can_recheck = can_run && self.pending_review != Some(0);
        let blocker = match &shown_provider {
            Some(shown) if choosing => Some(format!("Choose a model to switch to {}.", shown.name)),
            _ if !readiness.ready => readiness.blocker.clone(),
            _ if no_categories => Some("Create a category to begin.".into()),
            _ => None,
        };
        let llm = self.render_llm(cx).into_any_element();
        let auto = self.render_auto(cx).into_any_element();
        let theme = cx.theme();
        let muted = theme.muted_foreground;

        Card::new().small().title("Configurations").child(
            v_flex()
                .gap_3()
                .text_sm()
                .child(labeled(
                    "Embedding model",
                    div().w(px(288.)).child(
                        Select::new(&self.embedding)
                            .small()
                            .disabled(disabled)
                            .accessibility_label("Embedding model"),
                    ),
                ))
                .child(
                    h_flex()
                        .flex_wrap()
                        .items_end()
                        .gap_4()
                        .child(labeled(
                            "Language model runs on",
                            div().w(px(224.)).child(
                                Select::new(&self.provider)
                                    .small()
                                    .disabled(disabled)
                                    .accessibility_label("Language model runs on"),
                            ),
                        ))
                        .child(llm),
                )
                .when_some(shown_provider.clone(), |this, provider| {
                    this.child(div().text_xs().text_color(muted).child(format!(
                        "Titles, URLs and category names are sent to {}.",
                        provider.name
                    )))
                })
                .child(auto)
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_3()
                        .child(
                            Button::new("categorize")
                                .primary()
                                .label(run_label("Categorize", running && !rechecking))
                                .disabled(!can_run)
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.run(RunRequest::label(), cx)),
                                ),
                        )
                        .when(running, |this| {
                            this.child(
                                Button::new("stop")
                                    .danger()
                                    .label("Stop")
                                    .disabled(self.stopping)
                                    .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
                            )
                        })
                        .child(
                            Button::new("recheck")
                                .outline()
                                .label(run_label("Re-check AI labels", rechecking))
                                .disabled(!can_recheck)
                                .tooltip(
                                    "Re-run the AI labels you have not confirmed, using your \
                                         latest pins and rules",
                                )
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.run(RunRequest::recheck(), cx)),
                                ),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("reset-data")
                                .danger()
                                .label("Reset data")
                                .disabled(running || downloading)
                                .on_click(|_, window, cx| {
                                    dialogs::open::<ResetCategorizationDialog>((), window, cx)
                                }),
                        ),
                )
                .when_some(blocker, |this, blocker| {
                    this.child(div().text_xs().text_color(muted).child(blocker))
                })
                .when_some(run_error, |this, error| {
                    this.child(div().text_xs().text_color(theme.danger).child(error))
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use makora_lib::categorization::ports::RunUsage;

    use super::*;

    fn at(minutes: i64) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc.with_ymd_and_hms(2026, 11, 5, 12, 0, 0).unwrap();
        (now + chrono::Duration::minutes(minutes), now)
    }

    fn distance(minutes: i64) -> String {
        let (next, now) = at(minutes);
        distance_to_now(next, now)
    }

    #[test]
    fn distances_read_like_date_fns() {
        assert_eq!(distance(0), "in less than a minute");
        assert_eq!(distance(1), "in 1 minute");
        assert_eq!(distance(30), "in 30 minutes");
        assert_eq!(distance(60), "in about 1 hour");
        assert_eq!(distance(180), "in about 3 hours");
        assert_eq!(distance(24 * 60), "in 1 day");
        assert_eq!(distance(3 * 24 * 60), "in 3 days");
        assert_eq!(distance(7 * 24 * 60), "in 7 days");
        assert_eq!(distance(-30), "30 minutes ago");
    }

    #[test]
    fn the_next_run_is_described() {
        let (next, now) = at(0);
        assert_eq!(describe_next_run(next, now, true), "A run is in progress.");
        assert_eq!(
            describe_next_run(next, now, false),
            "Next run: any moment now."
        );
        let (next, now) = at(120);
        let text = describe_next_run(next, now, false);
        assert!(text.starts_with("Next run: "), "{text}");
        assert!(text.ends_with(" (in about 2 hours)."), "{text}");
    }

    #[test]
    fn periods_have_names() {
        let labels: Vec<String> = AUTO_CATEGORIZE_HOURS_OPTIONS
            .into_iter()
            .map(period_label)
            .collect();
        assert_eq!(
            labels,
            ["3 hours", "6 hours", "12 hours", "day", "3 days", "week"]
        );
    }

    #[test]
    fn a_missing_model_is_listed_first_and_marked() {
        let installed = vec!["a".to_string(), "b".to_string()];
        let labels: Vec<String> = model_choices("c", &installed)
            .iter()
            .map(|choice| choice.label.to_string())
            .collect();
        assert_eq!(labels, ["c (not installed)", "a", "b", "Add model"]);
        let values: Vec<String> = model_choices("b", &installed)
            .iter()
            .map(|choice| choice.value.to_string())
            .collect();
        assert_eq!(values, ["a", "b", ADD_MODEL]);
    }

    fn settings(provider: Option<&str>) -> Settings {
        let mut settings = Settings::default();
        settings.ai.llm_model = "llama".into();
        settings.ai.embedding_model = "minilm".into();
        settings.ai.llm_provider = provider.map(Into::into);
        settings
    }

    #[test]
    fn readiness_names_what_is_missing() {
        let both = vec![
            ("llama".to_string(), ModelRole::Llm),
            ("minilm".to_string(), ModelRole::Embedding),
        ];
        let ready = readiness(&settings(None), Some(&both));
        assert_eq!(
            ready,
            Readiness {
                ready: true,
                blocker: None
            }
        );

        let loading = readiness(&settings(None), None);
        assert_eq!(
            loading,
            Readiness {
                ready: false,
                blocker: None
            }
        );

        let no_llm = readiness(&settings(None), Some(&both[1..]));
        assert_eq!(
            no_llm.blocker.as_deref(),
            Some("Language model llama is not installed. Add it from the menu above.")
        );

        let no_embedding = readiness(&settings(None), Some(&both[..1]));
        assert_eq!(
            no_embedding.blocker.as_deref(),
            Some("Embedding model minilm is not installed. Add it from the menu above.")
        );

        let gone = readiness(&settings(Some("missing")), Some(&both));
        assert_eq!(
            gone.blocker.as_deref(),
            Some("The selected provider no longer exists. Choose another.")
        );
    }

    #[test]
    fn the_run_summary_counts_what_changed() {
        let outcome = AiRunOutcome {
            labeled: 4,
            skipped: 1,
            changed: 2,
            stopped: false,
            usage: RunUsage::default(),
            suggested: 0,
        };
        let (title, description) = run_summary(&outcome, RunMode::Label);
        assert_eq!(title, "AI categorization finished");
        assert!(description.starts_with("4 activities labeled, 1 left uncategorized."));
        let (title, description) = run_summary(&outcome, RunMode::Recheck);
        assert_eq!(title, "AI labels re-checked");
        assert!(description.starts_with("2 labels changed, 1 cleared as uncertain."));
        let stopped = AiRunOutcome {
            stopped: true,
            ..outcome
        };
        assert_eq!(run_summary(&stopped, RunMode::Recheck).0, "AI categorization stopped");
    }

    #[test]
    fn the_run_summary_says_what_categories_it_found() {
        let found = AiRunOutcome {
            labeled: 4,
            suggested: 2,
            ..AiRunOutcome::default()
        };
        let (_, description) = run_summary(&found, RunMode::Label);
        assert!(description.starts_with("4 activities labeled, 0 left uncategorized. 2 new categories suggested."));
        let (title, description) = run_summary(&AiRunOutcome::default(), RunMode::FindCategories);
        assert_eq!(title, "New categories");
        assert!(description.starts_with("No new groups of activities found."));
    }
}
