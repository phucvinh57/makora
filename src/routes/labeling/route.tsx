import { createFileRoute } from "@tanstack/react-router";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { AiTab } from "./-components/AiTab";
import { ClustersTab } from "./-components/ClustersTab";
import { AccuracyTab } from "./-components/AccuracyTab";
import { useLabelingStore, type LabelingTab } from "@/stores/labeling";

const TABS: LabelingTab[] = ["ai", "clusters", "accuracy"];

export const Route = createFileRoute("/labeling")({
  component: LabelingPage,
});

function LabelingPage() {
  const tab = useLabelingStore((state) => state.tab);
  const setTab = useLabelingStore((state) => state.setTab);
  const activeTab = TABS.includes(tab) ? tab : "ai";

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Labeling</h1>
        <p className="m-0 text-sm text-muted-foreground">
          AI categorization, accuracy, and activity clusters
        </p>
      </header>

      <Tabs
        value={activeTab}
        onValueChange={(value) => setTab(value as LabelingTab)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <TabsList className="mb-4 self-start">
          <TabsTrigger value="ai">AI</TabsTrigger>
          <TabsTrigger value="clusters">Clusters</TabsTrigger>
          <TabsTrigger value="accuracy">Accuracy</TabsTrigger>
        </TabsList>
        <TabsContent value="ai" className="flex min-h-0 flex-col">
          <AiTab />
        </TabsContent>
        <TabsContent value="clusters" className="flex min-h-0 flex-col">
          <ClustersTab />
        </TabsContent>
        <TabsContent
          value="accuracy"
          className="min-h-0 flex-1 overflow-y-auto"
        >
          <AccuracyTab />
        </TabsContent>
      </Tabs>
    </div>
  );
}
