import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { toast } from "sonner";
import { aiProvider } from "@/api";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useDialog } from "@/hooks/useDialog";
import {
  CUSTOM_PRESET,
  PROVIDER_KIND_LABELS,
  PROVIDER_KINDS,
  PROVIDER_PRESETS,
  parseProviderKind,
  presetOf,
} from "@/providers";
import {
  providerHasKeyQueryOptions,
  providerModelsQueryOptions,
  queryKeys,
} from "@/queries";
import type { ProviderConfig, ProviderKind, Settings } from "@/types";

const PRESET_ITEMS = {
  ...Object.fromEntries(
    PROVIDER_PRESETS.map((preset) => [preset.id, preset.label]),
  ),
  [CUSTOM_PRESET]: "Other (OpenAI-compatible)",
};

export interface ProviderDialogData {
  /** The provider to edit; left out to add one. */
  provider?: ProviderConfig;
}

/** Mounted only while the dialog is open, so every open starts from the provider as saved. */
function ProviderForm({
  provider,
  onDone,
}: {
  provider: ProviderConfig | undefined;
  onDone: () => void;
}) {
  const queryClient = useQueryClient();
  const initialPreset = provider
    ? (presetOf(provider)?.id ?? CUSTOM_PRESET)
    : PROVIDER_PRESETS[0].id;
  const [presetId, setPresetId] = useState(initialPreset);
  const [kind, setKind] = useState<ProviderKind>(
    provider?.kind ?? PROVIDER_PRESETS[0].kind,
  );
  const [name, setName] = useState(provider?.name ?? PROVIDER_PRESETS[0].label);
  const [nameTouched, setNameTouched] = useState(provider !== undefined);
  const [baseUrl, setBaseUrl] = useState(
    provider?.base_url ?? PROVIDER_PRESETS[0].baseUrl,
  );
  const [apiKey, setApiKey] = useState("");
  const [clearKey, setClearKey] = useState(false);

  const preset = PROVIDER_PRESETS.find((item) => item.id === presetId);
  const hasKeyQuery = useQuery({
    ...providerHasKeyQueryOptions(provider?.id ?? ""),
    enabled: provider !== undefined,
  });
  const hasKey = provider !== undefined && hasKeyQuery.data === true;

  function choosePreset(next: string) {
    setPresetId(next);
    const chosen = PROVIDER_PRESETS.find((item) => item.id === next);
    if (chosen) {
      setKind(chosen.kind);
      setBaseUrl(chosen.baseUrl);
      if (!nameTouched) setName(chosen.label);
    } else {
      setKind("openai");
      setBaseUrl("");
      if (!nameTouched) setName("");
    }
  }

  const saveMutation = useMutation({
    mutationFn: () => {
      const key = apiKey.trim();
      return aiProvider.save(
        { id: provider?.id, name, kind, base_url: baseUrl },
        key !== "" ? key : clearKey ? "" : undefined,
      );
    },
    onSuccess: async (saved: Settings) => {
      const before = queryClient.getQueryData<Settings>(queryKeys.settings);
      queryClient.setQueryData(queryKeys.settings, saved);
      const id =
        provider?.id ??
        saved.ai.providers.find(
          (item) => !before?.ai.providers.some((old) => old.id === item.id),
        )?.id;
      onDone();
      if (!id) return;
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: [...queryKeys.aiProviderHasKey, id],
        }),
        queryClient.removeQueries({
          queryKey: [...queryKeys.aiProviderModels, id],
        }),
      ]);
      try {
        const models = await queryClient.fetchQuery(
          providerModelsQueryOptions(id),
        );
        toast.success(`Saved ${name.trim()}`, {
          description: `Connected; ${models.length} models available.`,
        });
      } catch (err) {
        toast.warning(`Saved ${name.trim()}, but could not connect`, {
          description: String(err),
        });
      }
    },
  });

  const canSave =
    name.trim() !== "" && baseUrl.trim() !== "" && !saveMutation.isPending;
  const keyPlaceholder = hasKey
    ? clearKey
      ? "The saved key will be removed"
      : "Leave blank to keep the saved key"
    : preset && !preset.needsKey
      ? "Not needed for a local server"
      : "sk-…";

  return (
    <form
      className="contents"
      onSubmit={(event) => {
        event.preventDefault();
        if (canSave) saveMutation.mutate();
      }}
    >
      <DialogHeader>
        <DialogTitle>
          {provider ? `Edit ${provider.name}` : "Add an AI provider"}
        </DialogTitle>
        <DialogDescription>
          A remote language model can categorize activities instead of the local
          one. The API key is kept in your system keychain.
        </DialogDescription>
      </DialogHeader>
      <div className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <label htmlFor="provider-preset" className="text-xs font-medium">
            Provider
          </label>
          <Select
            value={presetId}
            onValueChange={(next) => next && choosePreset(next)}
            items={PRESET_ITEMS}
          >
            <SelectTrigger id="provider-preset" className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {Object.entries(PRESET_ITEMS).map(([id, label]) => (
                <SelectItem key={id} value={id}>
                  {label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        {presetId === CUSTOM_PRESET && (
          <div className="flex flex-col gap-1">
            <label htmlFor="provider-kind" className="text-xs font-medium">
              API
            </label>
            <Select
              value={kind}
              onValueChange={(next) => {
                const parsed = parseProviderKind(next);
                if (parsed) setKind(parsed);
              }}
              items={PROVIDER_KIND_LABELS}
            >
              <SelectTrigger id="provider-kind" className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PROVIDER_KINDS.map((item) => (
                  <SelectItem key={item} value={item}>
                    {PROVIDER_KIND_LABELS[item]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
        <div className="flex flex-col gap-1">
          <label htmlFor="provider-name" className="text-xs font-medium">
            Name
          </label>
          <Input
            id="provider-name"
            value={name}
            onChange={(event) => {
              setNameTouched(true);
              setName(event.target.value);
            }}
            autoComplete="off"
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor="provider-url" className="text-xs font-medium">
            Base URL
          </label>
          <Input
            id="provider-url"
            value={baseUrl}
            onChange={(event) => setBaseUrl(event.target.value)}
            placeholder="https://example.com/v1"
            autoComplete="off"
            spellCheck={false}
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor="provider-key" className="text-xs font-medium">
            API key
          </label>
          <div className="flex items-center gap-2">
            <Input
              id="provider-key"
              type="password"
              value={apiKey}
              disabled={clearKey}
              onChange={(event) => setApiKey(event.target.value)}
              placeholder={keyPlaceholder}
              autoComplete="off"
              spellCheck={false}
            />
            {hasKey && (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() => {
                  setApiKey("");
                  setClearKey((value) => !value);
                }}
              >
                {clearKey ? "Keep key" : "Clear key"}
              </Button>
            )}
          </div>
        </div>
        {saveMutation.isError && (
          <p className="m-0 text-xs text-destructive">
            {String(saveMutation.error)}
          </p>
        )}
      </div>
      <DialogFooter>
        <Button type="button" variant="ghost" onClick={onDone}>
          Cancel
        </Button>
        <Button type="submit" disabled={!canSave}>
          {saveMutation.isPending ? "Saving…" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}

export function ProviderDialog() {
  const { open, setOpen, data } = useDialog("provider");
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent>
        <ProviderForm
          key={data?.provider?.id ?? "new"}
          provider={data?.provider}
          onDone={() => setOpen(false)}
        />
      </DialogContent>
    </Dialog>
  );
}
