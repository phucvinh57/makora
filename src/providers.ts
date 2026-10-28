import type { ProviderConfig, ProviderKind } from "./types";

export interface ProviderPreset {
  id: string;
  label: string;
  kind: ProviderKind;
  baseUrl: string;
  /** Local servers take no key. */
  needsKey: boolean;
}

export const CUSTOM_PRESET = "custom";

export const PROVIDER_PRESETS: ProviderPreset[] = [
  {
    id: "openai",
    label: "OpenAI",
    kind: "openai",
    baseUrl: "https://api.openai.com/v1",
    needsKey: true,
  },
  {
    id: "anthropic",
    label: "Anthropic",
    kind: "anthropic",
    baseUrl: "https://api.anthropic.com",
    needsKey: true,
  },
  {
    id: "openrouter",
    label: "OpenRouter",
    kind: "openai",
    baseUrl: "https://openrouter.ai/api/v1",
    needsKey: true,
  },
  {
    id: "groq",
    label: "Groq",
    kind: "openai",
    baseUrl: "https://api.groq.com/openai/v1",
    needsKey: true,
  },
  {
    id: "ollama",
    label: "Ollama server",
    kind: "openai",
    baseUrl: "http://localhost:11434/v1",
    needsKey: false,
  },
  {
    id: "lmstudio",
    label: "LM Studio",
    kind: "openai",
    baseUrl: "http://localhost:1234/v1",
    needsKey: false,
  },
];

export const PROVIDER_KIND_LABELS: Record<ProviderKind, string> = {
  openai: "OpenAI-compatible",
  anthropic: "Anthropic",
};

/** The preset a saved provider was made from, judged by its kind and address. */
export function presetOf(provider: ProviderConfig): ProviderPreset | undefined {
  const url = provider.base_url.replace(/\/+$/, "");
  return PROVIDER_PRESETS.find(
    (preset) => preset.kind === provider.kind && preset.baseUrl === url,
  );
}
