import { create } from "zustand";
import { persist } from "zustand/middleware";

export type CategoryTab = "categories" | "rules";

type RulesCategoryFilter = number | "all";

interface CategoryPageState {
  categoryTab: CategoryTab;
  rulesCategoryFilter: RulesCategoryFilter;
  setCategoryTab: (tab: CategoryTab) => void;
  setRulesCategoryFilter: (filter: RulesCategoryFilter) => void;
}

export const useCategoryPageStore = create<CategoryPageState>()(
  persist(
    (set) => ({
      categoryTab: "categories",
      rulesCategoryFilter: "all",
      setCategoryTab: (categoryTab) => set({ categoryTab }),
      setRulesCategoryFilter: (rulesCategoryFilter) =>
        set({ rulesCategoryFilter }),
    }),
    {
      name: "makora-category-page",
      partialize: ({ categoryTab, rulesCategoryFilter }) => ({
        categoryTab,
        rulesCategoryFilter,
      }),
    },
  ),
);
