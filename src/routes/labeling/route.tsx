import { createFileRoute } from "@tanstack/react-router";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CategoriesTab } from "./-components/CategoriesTab";
import { RulesTab } from "./-components/RulesTab";
import { AiTab } from "./-components/AiTab";
import { ClustersTab } from "./-components/ClustersTab";
import { useLabelingStore, type LabelingTab } from "@/stores/labeling";

const TABS: LabelingTab[] = ["categories", "rules", "ai", "clusters"];

export const Route = createFileRoute("/labeling")({
  component: LabelingPage,
});

function LabelingPage() {
  const tab = useLabelingStore((state) => state.tab);
  const rulesCategoryFilter = useLabelingStore(
    (state) => state.rulesCategoryFilter,
  );
  const setTab = useLabelingStore((state) => state.setTab);
  const setRulesCategoryFilter = useLabelingStore(
    (state) => state.setRulesCategoryFilter,
  );
  const activeTab = TABS.includes(tab) ? tab : "categories";

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Labeling</h1>
        <p className="m-0 text-sm text-muted-foreground">Categories & rules</p>
      </header>

      <Tabs
        value={activeTab}
        onValueChange={(value) => setTab(value as LabelingTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="mb-4 self-start">
          <TabsTrigger value="ai">AI</TabsTrigger>
          <TabsTrigger value="clusters">Clusters</TabsTrigger>
          <TabsTrigger value="categories">Categories</TabsTrigger>
          <TabsTrigger value="rules">Rules</TabsTrigger>
        </TabsList>
        <TabsContent value="categories" className="flex min-h-0 flex-col">
          <CategoriesTab
            onShowRules={(categoryId) => {
              setRulesCategoryFilter(categoryId);
              setTab("rules");
            }}
          />
        </TabsContent>
        <TabsContent value="rules" className="flex min-h-0 flex-col">
          <RulesTab
            categoryFilter={rulesCategoryFilter}
            onCategoryFilterChange={setRulesCategoryFilter}
          />
        </TabsContent>
        <TabsContent value="ai" className="flex min-h-0 flex-col">
          <AiTab />
        </TabsContent>
        <TabsContent value="clusters" className="flex min-h-0 flex-col">
          <ClustersTab />
        </TabsContent>
      </Tabs>
    </div>
  );
}
