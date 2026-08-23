import { create } from "zustand";
import { persist } from "zustand/middleware";

export type LabelingTab = "ai" | "clusters";
export type CategoryTab = "categories" | "rules";

type RulesCategoryFilter = number | "all";

interface LabelingState {
  tab: LabelingTab;
  categoryTab: CategoryTab;
  rulesCategoryFilter: RulesCategoryFilter;
  setTab: (tab: LabelingTab) => void;
  setCategoryTab: (tab: CategoryTab) => void;
  setRulesCategoryFilter: (filter: RulesCategoryFilter) => void;
}

export const useLabelingStore = create<LabelingState>()(
  persist(
    (set) => ({
      tab: "ai",
      categoryTab: "categories",
      rulesCategoryFilter: "all",
      setTab: (tab) => set({ tab }),
      setCategoryTab: (categoryTab) => set({ categoryTab }),
      setRulesCategoryFilter: (rulesCategoryFilter) =>
        set({ rulesCategoryFilter }),
    }),
    {
      name: "makora-labeling",
      version: 2,
      migrate: (persisted, version) => {
        const state = persisted as Partial<LabelingState>;
        const oldTab = state.tab as string | undefined;
        const categoryTab =
          state.categoryTab === "categories" || state.categoryTab === "rules"
            ? state.categoryTab
            : oldTab === "rules"
              ? "rules"
              : "categories";

        return {
          tab:
            version < 2 && oldTab !== "ai" && oldTab !== "clusters"
              ? "ai"
              : (state.tab ?? "ai"),
          categoryTab,
          rulesCategoryFilter: state.rulesCategoryFilter ?? "all",
        };
      },
      partialize: ({ tab, categoryTab, rulesCategoryFilter }) => ({
        tab,
        categoryTab,
        rulesCategoryFilter,
      }),
    },
  ),
);
