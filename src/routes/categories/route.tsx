import { useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { useState } from "react";
import { aiFeedbackStats } from "@/api";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AiCategorizationSection } from "./-components/AiCategorizationSection";
import { CategoriesTab } from "./-components/CategoriesTab";
import { ReviewQueue } from "./-components/ReviewQueue";

export const Route = createFileRoute("/categories")({
  component: CategoriesPage,
});

const TAB_STORAGE_KEY = "makora-categories-tab";
type TabName = "taxonomy" | "review";

function readStoredTab(): TabName {
  try {
    return localStorage.getItem(TAB_STORAGE_KEY) === "review"
      ? "review"
      : "taxonomy";
  } catch {
    return "taxonomy";
  }
}

function CategoriesPage() {
  const [tab, setTab] = useState<TabName>(readStoredTab);
  const statsQuery = useQuery({
    queryKey: ["aiFeedbackStats"],
    queryFn: aiFeedbackStats,
  });
  const pending = statsQuery.data?.pending_review ?? 0;

  function changeTab(value: unknown) {
    const next: TabName = value === "review" ? "review" : "taxonomy";
    setTab(next);
    try {
      localStorage.setItem(TAB_STORAGE_KEY, next);
    } catch {
      // The tab just won't be remembered.
    }
  }

  return (
    <div className="flex flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Categories</h1>
        <p className="m-0 text-sm text-muted-foreground">
          Organize activity with categories and teach the AI what belongs where
        </p>
      </header>
      <Tabs value={tab} onValueChange={changeTab}>
        <TabsList>
          <TabsTrigger value="taxonomy">Taxonomy</TabsTrigger>
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
        <TabsContent value="review" className="flex flex-col gap-6 mt-4">
          <AiCategorizationSection />
          <ReviewQueue />
        </TabsContent>
      </Tabs>
    </div>
  );
}
