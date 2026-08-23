import { create } from "zustand";
import { persist } from "zustand/middleware";

export type PluginFeature = "focus";

type FeatureFlags = Record<PluginFeature, boolean>;

interface FeatureFlagsState {
  flags: FeatureFlags;
  setFeatureEnabled: (feature: PluginFeature, enabled: boolean) => void;
}

const DEFAULT_FEATURE_FLAGS: FeatureFlags = {
  focus: true,
};

function normalizeFeatureFlags(value: unknown): FeatureFlags {
  if (!value || typeof value !== "object") return { ...DEFAULT_FEATURE_FLAGS };

  const flags = (value as { flags?: unknown }).flags;
  if (!flags || typeof flags !== "object") {
    return { ...DEFAULT_FEATURE_FLAGS };
  }

  const focus = (flags as { focus?: unknown }).focus;
  return {
    focus: typeof focus === "boolean" ? focus : DEFAULT_FEATURE_FLAGS.focus,
  };
}

export const useFeatureFlags = create<FeatureFlagsState>()(
  persist(
    (set) => ({
      flags: DEFAULT_FEATURE_FLAGS,
      setFeatureEnabled: (feature, enabled) =>
        set((state) => ({
          flags: { ...state.flags, [feature]: enabled },
        })),
    }),
    {
      name: "makora-feature-flags",
      partialize: ({ flags }) => ({ flags }),
      merge: (persisted, current) => ({
        ...current,
        flags: normalizeFeatureFlags(persisted),
      }),
    },
  ),
);

export function isFeatureEnabled(feature: PluginFeature): boolean {
  return useFeatureFlags.getState().flags[feature];
}
