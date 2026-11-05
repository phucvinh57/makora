use makora_lib::settings::{ProviderConfig, ProviderKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ProviderKind,
    pub base_url: &'static str,
    /// Local servers take no key.
    pub needs_key: bool,
}

pub const CUSTOM_PRESET: &str = "custom";
pub const CUSTOM_PRESET_LABEL: &str = "Other (OpenAI-compatible)";

pub const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "openai",
        label: "OpenAI",
        kind: ProviderKind::OpenAi,
        base_url: "https://api.openai.com/v1",
        needs_key: true,
    },
    ProviderPreset {
        id: "anthropic",
        label: "Anthropic",
        kind: ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com",
        needs_key: true,
    },
    ProviderPreset {
        id: "openrouter",
        label: "OpenRouter",
        kind: ProviderKind::OpenAi,
        base_url: "https://openrouter.ai/api/v1",
        needs_key: true,
    },
    ProviderPreset {
        id: "groq",
        label: "Groq",
        kind: ProviderKind::OpenAi,
        base_url: "https://api.groq.com/openai/v1",
        needs_key: true,
    },
    ProviderPreset {
        id: "ollama",
        label: "Ollama server",
        kind: ProviderKind::OpenAi,
        base_url: "http://localhost:11434/v1",
        needs_key: false,
    },
    ProviderPreset {
        id: "lmstudio",
        label: "LM Studio",
        kind: ProviderKind::OpenAi,
        base_url: "http://localhost:1234/v1",
        needs_key: false,
    },
];

pub const PROVIDER_KINDS: [ProviderKind; 2] = [ProviderKind::OpenAi, ProviderKind::Anthropic];

pub fn provider_kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAi => "OpenAI-compatible",
        ProviderKind::Anthropic => "Anthropic",
    }
}

/// The kind's wire name, `openai` or `anthropic`.
pub fn provider_kind_id(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::OpenAi => "openai",
        ProviderKind::Anthropic => "anthropic",
    }
}

pub fn parse_provider_kind(value: &str) -> Option<ProviderKind> {
    PROVIDER_KINDS
        .into_iter()
        .find(|kind| provider_kind_id(*kind) == value)
}

pub fn preset_by_id(id: &str) -> Option<&'static ProviderPreset> {
    PROVIDER_PRESETS.iter().find(|preset| preset.id == id)
}

/// The preset a saved provider was made from, judged by its kind and address.
pub fn preset_of(provider: &ProviderConfig) -> Option<&'static ProviderPreset> {
    let url = provider.base_url.trim_end_matches('/');
    PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.kind == provider.kind && preset.base_url == url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(kind: ProviderKind, base_url: &str) -> ProviderConfig {
        ProviderConfig {
            id: "p1".into(),
            name: "Mine".into(),
            kind,
            base_url: base_url.into(),
        }
    }

    #[test]
    fn six_presets_in_order() {
        let ids: Vec<&str> = PROVIDER_PRESETS.iter().map(|preset| preset.id).collect();
        assert_eq!(
            ids,
            [
                "openai",
                "anthropic",
                "openrouter",
                "groq",
                "ollama",
                "lmstudio"
            ]
        );
        let keyless: Vec<&str> = PROVIDER_PRESETS
            .iter()
            .filter(|p| !p.needs_key)
            .map(|p| p.id)
            .collect();
        assert_eq!(keyless, ["ollama", "lmstudio"]);
    }

    #[test]
    fn kind_labels_and_ids() {
        assert_eq!(
            provider_kind_label(ProviderKind::OpenAi),
            "OpenAI-compatible"
        );
        assert_eq!(provider_kind_label(ProviderKind::Anthropic), "Anthropic");
        assert_eq!(parse_provider_kind("openai"), Some(ProviderKind::OpenAi));
        assert_eq!(
            parse_provider_kind("anthropic"),
            Some(ProviderKind::Anthropic)
        );
        assert_eq!(parse_provider_kind("OpenAI"), None);
        assert_eq!(parse_provider_kind(""), None);
    }

    #[test]
    fn kind_ids_match_serde() {
        for kind in PROVIDER_KINDS {
            assert_eq!(serde_json::to_value(kind).unwrap(), provider_kind_id(kind));
        }
    }

    #[test]
    fn preset_lookup_by_id() {
        assert_eq!(preset_by_id("groq").map(|p| p.label), Some("Groq"));
        assert_eq!(preset_by_id(CUSTOM_PRESET), None);
    }

    #[test]
    fn preset_of_matches_kind_and_url() {
        let groq = provider(ProviderKind::OpenAi, "https://api.groq.com/openai/v1");
        assert_eq!(preset_of(&groq).map(|p| p.id), Some("groq"));
        let trailing = provider(ProviderKind::Anthropic, "https://api.anthropic.com//");
        assert_eq!(preset_of(&trailing).map(|p| p.id), Some("anthropic"));
        let wrong_kind = provider(ProviderKind::Anthropic, "https://api.openai.com/v1");
        assert_eq!(preset_of(&wrong_kind), None);
        let custom = provider(ProviderKind::OpenAi, "http://my-box:8080/v1");
        assert_eq!(preset_of(&custom), None);
    }
}
