import { createFileRoute } from "@tanstack/react-router";
import { IgnoreListSection } from "./-components/IgnoreListSection";
import { QueryParamSection } from "./-components/QueryParamSection";
import { StartupSection } from "./-components/StartupSection";
import { ThemeSection } from "./-components/ThemeSection";

export const Route = createFileRoute("/settings")({
  component: SettingsPage,
});

function SettingsPage() {
  return (
    <div className="flex h-full flex-col gap-6 overflow-y-auto subtle-scrollbar box-border">
      <header>
        <h1 className="m-0 text-lg font-semibold">Settings</h1>
        <p className="m-0 text-sm text-muted-foreground">
          App preferences and tracking exclusions
        </p>
      </header>
      <ThemeSection />
      <StartupSection />
      <IgnoreListSection />
      <QueryParamSection />
    </div>
  );
}
