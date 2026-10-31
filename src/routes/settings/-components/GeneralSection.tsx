import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { autostart } from "@/api";
import { queryKeys } from "@/queries";
import { toggleTheme, useTheme } from "@/theme";

export function GeneralSection() {
  const theme = useTheme();
  const queryClient = useQueryClient();

  const autostartQuery = useQuery({
    queryKey: queryKeys.autostartEnabled,
    queryFn: autostart.isEnabled,
    enabled: !import.meta.env.DEV,
  });

  const toggleMutation = useMutation({
    mutationFn: (enabled: boolean) => autostart.setEnabled(enabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.autostartEnabled });
    },
  });

  const autostartEnabled = autostartQuery.data ?? false;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>General</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center justify-between">
          <span className="text-sm">Theme</span>
          <div className="flex items-center gap-2">
            <Button
              variant={theme === "light" ? "default" : "outline"}
              size="sm"
              onClick={() => theme === "dark" && toggleTheme()}
            >
              <Sun className="size-4" />
              Light
            </Button>
            <Button
              variant={theme === "dark" ? "default" : "outline"}
              size="sm"
              onClick={() => theme === "light" && toggleTheme()}
            >
              <Moon className="size-4" />
              Dark
            </Button>
          </div>
        </div>
        <div className="flex items-center justify-between">
          <label htmlFor="autostart-checkbox" className="text-sm">
            Run when system starts
          </label>
          <Switch
            id="autostart-checkbox"
            checked={autostartEnabled}
            disabled={
              import.meta.env.DEV ||
              autostartQuery.isLoading ||
              toggleMutation.isPending
            }
            onCheckedChange={(checked) => toggleMutation.mutate(checked)}
          />
        </div>
        {import.meta.env.DEV && (
          <p className="text-xs text-muted-foreground">
            Autostart is disabled in dev builds.
          </p>
        )}
        {toggleMutation.isError && (
          <p className="text-xs text-destructive">
            {String(toggleMutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
