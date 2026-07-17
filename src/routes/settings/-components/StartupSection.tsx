import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { isAutostartEnabled, setAutostartEnabled } from "@/api";

export function StartupSection() {
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

  const enabled = autostartQuery.data ?? false;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Startup</CardTitle>
        <CardDescription>
          Launch Makora automatically when you sign in.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex w-fit items-center gap-2 text-sm">
          <Checkbox
            id="autostart-checkbox"
            checked={enabled}
            disabled={autostartQuery.isLoading || toggleMutation.isPending}
            onCheckedChange={(checked) =>
              toggleMutation.mutate(checked === true)
            }
          />
          <label htmlFor="autostart-checkbox">Run when system starts</label>
        </div>
        {toggleMutation.isError && (
          <p className="mt-2 text-xs text-destructive">
            {String(toggleMutation.error)}
          </p>
        )}
      </CardContent>
    </Card>
  );
}
