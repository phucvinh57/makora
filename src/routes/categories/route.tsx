import { createFileRoute } from "@tanstack/react-router";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useCategoryPageStore, type CategoryTab } from "@/stores/categoryPage";
import { CategoriesTab } from "./-components/CategoriesTab";
import { RulesTab } from "./-components/RulesTab";

const TABS: CategoryTab[] = ["categories", "rules"];

export const Route = createFileRoute("/categories")({
  component: CategoriesPage,
});

function CategoriesPage() {
  const tab = useCategoryPageStore((state) => state.categoryTab);
  const rulesCategoryFilter = useCategoryPageStore(
    (state) => state.rulesCategoryFilter,
  );
  const setCategoryTab = useCategoryPageStore((state) => state.setCategoryTab);
  const setRulesCategoryFilter = useCategoryPageStore(
    (state) => state.setRulesCategoryFilter,
  );
  const activeTab = TABS.includes(tab) ? tab : "categories";

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Categories</h1>
        <p className="m-0 text-sm text-muted-foreground">
          Organize activity with categories and rules
        </p>
      </header>

      <Tabs
        value={activeTab}
        onValueChange={(value) => setCategoryTab(value as CategoryTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="mb-4 self-start">
          <TabsTrigger value="categories">Categories</TabsTrigger>
          <TabsTrigger value="rules">Rules</TabsTrigger>
        </TabsList>
        <TabsContent value="categories" className="flex min-h-0 flex-col">
          <CategoriesTab
            onShowRules={(categoryId) => {
              setRulesCategoryFilter(categoryId);
              setCategoryTab("rules");
            }}
          />
        </TabsContent>
        <TabsContent value="rules" className="flex min-h-0 flex-col">
          <RulesTab
            categoryFilter={rulesCategoryFilter}
            onCategoryFilterChange={setRulesCategoryFilter}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
