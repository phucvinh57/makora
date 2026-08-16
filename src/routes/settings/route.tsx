import { createFileRoute } from "@tanstack/react-router";
import { DistractionSection } from "./-components/DistractionSection";
import { GeneralSection } from "./-components/GeneralSection";
import { IgnoreListSection } from "./-components/IgnoreListSection";

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
      <GeneralSection />
      <DistractionSection />
      <IgnoreListSection />
    </div>
  );
}
