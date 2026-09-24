import { createFileRoute } from "@tanstack/react-router";
import { CategorizationDataSection } from "./-components/CategorizationDataSection";
import { FeatureFlagsSection } from "./-components/FeatureFlagsSection";
import { GeneralSection } from "./-components/GeneralSection";

export const Route = createFileRoute("/settings")({
  component: SettingsPage,
});

function SettingsPage() {
  return (
    <div className="flex w-full max-w-3xl flex-col gap-6 box-border">
      <header>
        <h1 className="m-0 text-lg font-semibold">Settings</h1>
        <p className="m-0 text-sm text-muted-foreground">
          App preferences and tracking exclusions
        </p>
      </header>
      <GeneralSection />
      <FeatureFlagsSection />
      <CategorizationDataSection />
    </div>
  );
}
