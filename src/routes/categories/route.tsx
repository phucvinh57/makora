import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";
import {
  type StorageCodec,
  usePersistentState,
} from "@/hooks/usePersistentState";
import { aiFeedbackStatsQueryOptions } from "@/queries";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AiCategorizationSection } from "./-components/AiCategorizationSection";
import { CategoriesTab } from "./-components/CategoriesTab";
import { ReviewQueue } from "./-components/ReviewQueue";
import { RulesTab } from "./-components/RulesTab";

const TAB_STORAGE_KEY = "makora-categories-tab";
type TabName = "taxonomy" | "rules" | "review";

function parseTab(raw: unknown): TabName | undefined {
  return raw === "review" || raw === "rules" || raw === "taxonomy"
    ? raw
    : undefined;
}

const TAB_CODEC: StorageCodec<TabName> = {
  parse: parseTab,
  serialize: (tab) => tab,
};

export const Route = createFileRoute("/categories")({
  validateSearch: (search): { tab?: TabName } => {
    const tab = parseTab(search.tab);
    return tab ? { tab } : {};
  },
  component: CategoriesPage,
});

function CategoriesPage() {
  const { tab: linkedTab } = Route.useSearch();
  const navigate = Route.useNavigate();
  const [storedTab, setTab] = usePersistentState<TabName>(
    TAB_STORAGE_KEY,
    "taxonomy",
    TAB_CODEC,
  );
  const tab = linkedTab ?? storedTab;

  // A `?tab=` link (the AI notification's) becomes the remembered tab, then leaves the URL.
  useEffect(() => {
    if (!linkedTab) return;
    setTab(linkedTab);
    void navigate({ search: {}, replace: true });
  }, [linkedTab, setTab, navigate]);

  const statsQuery = useQuery(aiFeedbackStatsQueryOptions);
  const pending = statsQuery.data?.pending_review ?? 0;

  function changeTab(value: unknown) {
    setTab(parseTab(value) ?? "taxonomy");
  }

  return (
    <div className="flex flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Categories</h1>
        <p className="m-0 text-sm text-muted-foreground">
          Organize activity with categories, rules and the AI
        </p>
      </header>
      <Tabs value={tab} onValueChange={changeTab}>
        <TabsList>
          <TabsTrigger value="taxonomy">Taxonomy</TabsTrigger>
          <TabsTrigger value="rules">Rules</TabsTrigger>
          <TabsTrigger value="review">
            AI review
            {pending > 0 && (
              <span className="rounded-full bg-primary px-1.5 text-xs text-primary-foreground">
                {pending}
              </span>
            )}
          </TabsTrigger>
        </TabsList>
        <TabsContent value="taxonomy" className="flex flex-col">
          <CategoriesTab />
        </TabsContent>
        <TabsContent value="rules" className="flex flex-col">
          <RulesTab />
        </TabsContent>
        <TabsContent value="review" className="flex flex-col gap-6 mt-4">
          <AiCategorizationSection />
          <ReviewQueue />
        </TabsContent>
      </Tabs>
    </div>
  );
}
