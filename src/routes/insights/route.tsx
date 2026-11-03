import { createFileRoute } from "@tanstack/react-router";
import { useEffect } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  type StorageCodec,
  usePersistentState,
} from "@/hooks/usePersistentState";
import { LogsTab } from "./-components/logs/LogsTab";
import { AiUsageTab } from "./-components/usage/AiUsageTab";

const TAB_STORAGE_KEY = "makora-insights-tab";
type TabName = "usage" | "logs";

function parseTab(raw: unknown): TabName | undefined {
  return raw === "usage" || raw === "logs" ? raw : undefined;
}

const TAB_CODEC: StorageCodec<TabName> = {
  parse: parseTab,
  serialize: (tab) => tab,
};

export const Route = createFileRoute("/insights")({
  validateSearch: (search): { tab?: TabName } => {
    const tab = parseTab(search.tab);
    return tab ? { tab } : {};
  },
  component: InsightsPage,
});

function InsightsPage() {
  const { tab: linkedTab } = Route.useSearch();
  const navigate = Route.useNavigate();
  const [storedTab, setTab] = usePersistentState<TabName>(
    TAB_STORAGE_KEY,
    "usage",
    TAB_CODEC,
  );
  const tab = linkedTab ?? storedTab;

  useEffect(() => {
    if (!linkedTab) return;
    setTab(linkedTab);
    void navigate({ search: {}, replace: true });
  }, [linkedTab, setTab, navigate]);

  function changeTab(value: unknown) {
    setTab(parseTab(value) ?? "usage");
  }

  return (
    <div className="flex flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Insights</h1>
        <p className="m-0 text-sm text-muted-foreground">
          What the AI used and cost, and what Makora has been doing
        </p>
      </header>
      <Tabs value={tab} onValueChange={changeTab}>
        <TabsList>
          <TabsTrigger value="usage">AI usage</TabsTrigger>
          <TabsTrigger value="logs">Logs</TabsTrigger>
        </TabsList>
        <TabsContent value="usage" className="mt-4">
          <AiUsageTab />
        </TabsContent>
        <TabsContent value="logs" className="mt-4">
          <LogsTab />
        </TabsContent>
      </Tabs>
    </div>
  );
}
