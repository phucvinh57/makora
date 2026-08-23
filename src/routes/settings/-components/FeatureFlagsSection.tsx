import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { useFeatureFlags } from "@/stores/features";

export function FeatureFlagsSection() {
  const focusEnabled = useFeatureFlags((state) => state.flags.focus);
  const setFeatureEnabled = useFeatureFlags((state) => state.setFeatureEnabled);

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Optional features</CardTitle>
        <CardDescription>
          Turn plugin features on or off. Core features are always available.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex items-center justify-between gap-4">
          <div className="flex flex-col gap-1">
            <label htmlFor="focus-feature" className="text-sm font-medium">
              Focus
            </label>
            <p className="m-0 text-xs text-muted-foreground">
              Plan work blocks and compare them with your activity.
            </p>
          </div>
          <Switch
            id="focus-feature"
            checked={focusEnabled}
            onCheckedChange={(checked) => setFeatureEnabled("focus", checked)}
          />
        </div>
      </CardContent>
    </Card>
  );
}
