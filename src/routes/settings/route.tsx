import { createFileRoute } from "@tanstack/react-router";
import { AiProvidersSection } from "./-components/AiProvidersSection";
import { AiUsageSection } from "./-components/AiUsageSection";
import { GeneralSection } from "./-components/GeneralSection";
import { ExclusionsSection } from "./-components/ExclusionsSection";
import { TrackingSection } from "./-components/TrackingSection";

export const Route = createFileRoute("/settings")({
  component: SettingsPage,
});

function SettingsPage() {
  return (
    <div className="flex w-full max-w-3xl flex-col gap-6 box-border">
      <header>
        <h1 className="m-0 text-lg font-semibold">Settings</h1>
        <p className="m-0 text-sm text-muted-foreground">
          App preferences, tracking exclusions, AI providers and usage
        </p>
      </header>
      <GeneralSection />
      <TrackingSection />
      <ExclusionsSection />
      <AiProvidersSection />
      <AiUsageSection />
    </div>
  );
}
