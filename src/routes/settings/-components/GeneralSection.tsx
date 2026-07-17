import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Switch } from "@/components/ui/switch";
import { isAutostartEnabled, setAutostartEnabled } from "@/api";
import { toggleTheme, useTheme } from "@/theme";

export function GeneralSection() {
  const theme = useTheme();
  const queryClient = useQueryClient();

  const autostartQuery = useQuery({
    queryKey: ["autostartEnabled"],
    queryFn: isAutostartEnabled,
  });

  const toggleMutation = useMutation({
    mutationFn: (enabled: boolean) => setAutostartEnabled(enabled),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["autostartEnabled"] });
    },
  });

  const autostartEnabled = autostartQuery.data ?? false;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>General</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex items-center justify-between gap-2 w-100">
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
        <div className="flex items-center justify-between gap-2 w-100">
          <label htmlFor="autostart-checkbox" className="text-sm">
            Run when system starts
          </label>
          <Switch
            id="autostart-checkbox"
            checked={autostartEnabled}
            disabled={autostartQuery.isLoading || toggleMutation.isPending}
            onCheckedChange={(checked) => toggleMutation.mutate(checked)}
          />
        </div>
        {toggleMutation.isError && (
          <p className="text-xs text-destructive">
            {String(toggleMutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
