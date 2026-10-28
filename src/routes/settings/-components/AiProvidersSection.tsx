import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import type { ProviderDialogData } from "@/components/dialogs/ProviderDialog";
import type { RemoveProviderDialogData } from "@/components/dialogs/RemoveProviderDialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { openDialog } from "@/hooks/useDialog";
import { PROVIDER_KIND_LABELS } from "@/providers";
import {
  providerHasKeyQueryOptions,
  providerModelsQueryOptions,
  queryKeys,
  settingsQueryOptions,
} from "@/queries";
import type { ProviderConfig } from "@/types";

function ProviderRow({
  provider,
  active,
}: {
  provider: ProviderConfig;
  active: boolean;
}) {
  const queryClient = useQueryClient();
  const hasKey = useQuery(providerHasKeyQueryOptions(provider.id)).data;

  const testMutation = useMutation({
    mutationFn: async () => {
      queryClient.removeQueries({
        queryKey: [...queryKeys.aiProviderModels, provider.id],
      });
      return queryClient.fetchQuery(providerModelsQueryOptions(provider.id));
    },
    onSuccess: (models) =>
      toast.success(`${provider.name} is reachable`, {
        description: `${models.length} models available.`,
      }),
    onError: (err) =>
      toast.error(`Could not reach ${provider.name}`, {
        description: String(err),
      }),
  });

  return (
    <li className="flex items-center justify-between gap-3 px-3 py-2 text-sm">
      <div className="flex min-w-0 flex-col gap-0.5">
        <div className="flex items-center gap-2">
          <span className="truncate font-medium">{provider.name}</span>
          <Badge variant="outline">{PROVIDER_KIND_LABELS[provider.kind]}</Badge>
          {active && <Badge variant="secondary">In use</Badge>}
        </div>
        <span className="truncate text-xs text-muted-foreground">
          {provider.base_url}
          {hasKey !== undefined && (hasKey ? " · Key saved" : " · No key")}
        </span>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={testMutation.isPending}
          onClick={() => testMutation.mutate()}
        >
          {testMutation.isPending ? "Testing…" : "Test"}
        </Button>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label={`Edit ${provider.name}`}
          title="Edit"
          onClick={() =>
            openDialog<ProviderDialogData>("provider", { provider })
          }
        >
          <Pencil />
        </Button>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label={`Remove ${provider.name}`}
          title="Remove"
          onClick={() =>
            openDialog<RemoveProviderDialogData>("remove-provider", {
              provider,
            })
          }
        >
          <Trash2 />
        </Button>
      </div>
    </li>
  );
}

export function AiProvidersSection() {
  const settingsQuery = useQuery(settingsQueryOptions);
  const providers = settingsQuery.data?.ai.providers ?? [];
  const activeId = settingsQuery.data?.ai.llm_provider ?? null;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>AI providers</CardTitle>
        <CardAction>
          <Button
            type="button"
            size="sm"
            onClick={() => openDialog<ProviderDialogData>("provider", {})}
          >
            <Plus />
            Add provider
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {providers.length > 0 ? (
          <ul className="m-0 flex list-none flex-col divide-y rounded-md border p-0">
            {providers.map((provider) => (
              <ProviderRow
                key={provider.id}
                provider={provider}
                active={provider.id === activeId}
              />
            ))}
          </ul>
        ) : (
          <p className="m-0 text-sm text-muted-foreground">
            Optional. AI categorization runs on a local model unless you add a
            provider here, such as OpenAI, Anthropic or any OpenAI-compatible
            server, and pick it under Categories.
          </p>
        )}
        <p className="m-0 text-xs text-muted-foreground">
          When a remote provider categorizes, the titles and addresses of the
          activities being labeled and your category names are sent to it. Local
          servers like Ollama keep everything on this machine.
        </p>
      </CardContent>
    </Card>
  );
}
