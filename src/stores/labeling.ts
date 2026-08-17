import { create } from "zustand";
import { persist } from "zustand/middleware";

export type LabelingTab = "categories" | "rules" | "ai";

type RulesCategoryFilter = number | "all";

interface LabelingState {
  tab: LabelingTab;
  rulesCategoryFilter: RulesCategoryFilter;
  setTab: (tab: LabelingTab) => void;
  setRulesCategoryFilter: (filter: RulesCategoryFilter) => void;
}

export const useLabelingStore = create<LabelingState>()(
  persist(
    (set) => ({
      tab: "categories",
      rulesCategoryFilter: "all",
      setTab: (tab) => set({ tab }),
      setRulesCategoryFilter: (rulesCategoryFilter) =>
        set({ rulesCategoryFilter }),
    }),
    {
      name: "makora-labeling",
      partialize: ({ tab, rulesCategoryFilter }) => ({
        tab,
        rulesCategoryFilter,
      }),
    },
  ),
);
