import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { CategoriesTab } from "./-components/CategoriesTab";
import { ALL_CATEGORIES, RulesTab } from "./-components/RulesTab";
import { AiTab } from "./-components/AiTab";

export const Route = createFileRoute("/labeling")({
  component: LabelingPage,
});

function LabelingPage() {
  const [tab, setTab] = useState("categories");
  const [rulesCategoryFilter, setRulesCategoryFilter] = useState<
    number | typeof ALL_CATEGORIES
  >(ALL_CATEGORIES);

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Labeling</h1>
        <p className="m-0 text-sm text-muted-foreground">Categories & rules</p>
      </header>

      <Tabs
        value={tab}
        onValueChange={(value) => setTab(value as string)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="mb-4 self-start">
          <TabsTrigger value="categories">Categories</TabsTrigger>
          <TabsTrigger value="rules">Rules</TabsTrigger>
          <TabsTrigger value="ai">AI</TabsTrigger>
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
      </Tabs>
    </div>
  );
}
