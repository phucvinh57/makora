import { useQuery } from "@tanstack/react-query";
import { PlusIcon } from "lucide-react";
import { useState } from "react";
import { type Suggestion, SuggestInput } from "@/components/SuggestInput";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { openDialog } from "@/hooks/useDialog";
import { providerModelsQueryOptions } from "@/queries";
import type { ModelRole, ProviderConfig } from "@/types";
import { AutoCategorizeSetting } from "./AutoCategorizeSetting";
import { useCategorizationControls } from "./useCategorizationControls";

const ADD_MODEL_VALUE = "__add_model__";
const LOCAL_PROVIDER = "__local__";

function ProviderSelect({
  value,
  providers,
  disabled,
  onChange,
}: {
  value: string | null;
  providers: ProviderConfig[];
  disabled: boolean;
  onChange: (provider: string | null) => void;
}) {
  const items = {
    [LOCAL_PROVIDER]: "Local (on this machine)",
    ...Object.fromEntries(providers.map((p) => [p.id, p.name])),
  };
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor="ai-llm-provider" className="text-xs font-medium">
        Language model runs on
      </label>
      <Select
        value={value ?? LOCAL_PROVIDER}
        onValueChange={(next) => {
          if (!next) return;
          onChange(next === LOCAL_PROVIDER ? null : next);
        }}
        items={items}
        disabled={disabled}
      >
        <SelectTrigger id="ai-llm-provider" size="sm" className="w-56">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {Object.entries(items).map(([id, label]) => (
            <SelectItem key={id} value={id}>
              {label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

/** A remote model id: suggested from what the provider lists, typed freely when it lists none. */
function RemoteModelInput({
  provider,
  value,
  disabled,
  onApply,
}: {
  provider: ProviderConfig;
  value: string;
  disabled: boolean;
  onApply: (model: string) => void;
}) {
  const [typed, setTyped] = useState(value);
  const modelsQuery = useQuery(providerModelsQueryOptions(provider.id));
  const suggestions: Suggestion[] = (modelsQuery.data ?? []).map((model) => ({
    value: model,
  }));
  const trimmed = typed.trim();
  const canApply = !disabled && trimmed !== "" && trimmed !== value;
  return (
    <form
      className="flex flex-col gap-1"
      onSubmit={(event) => {
        event.preventDefault();
        if (canApply) onApply(trimmed);
      }}
    >
      <span className="text-xs font-medium">Language model</span>
      <div className="flex items-center gap-2">
        <div className="w-72">
          <SuggestInput
            value={typed}
            onValueChange={setTyped}
            suggestions={suggestions}
            placeholder={modelsQuery.isPending ? "Loading models…" : "Model id"}
            ariaLabel="Language model"
          />
        </div>
        <Button type="submit" size="sm" variant="outline" disabled={!canApply}>
          Use
        </Button>
      </div>
      {modelsQuery.isError && (
        <p className="m-0 text-xs text-destructive">
          Could not list {provider.name}'s models: {String(modelsQuery.error)}
        </p>
      )}
    </form>
  );
}

function ModelSelect({
  id,
  label,
  modelRole,
  value,
  installed,
  disabled,
  onChange,
}: {
  id: string;
  label: string;
  modelRole: ModelRole;
  value: string;
  installed: string[];
  disabled: boolean;
  onChange: (model: string) => void;
}) {
  const names = installed.includes(value) ? installed : [value, ...installed];
  const nameLabel = (name: string) =>
    installed.includes(name) ? name : `${name} (not installed)`;
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-xs font-medium">
        {label}
      </label>
      <Select
        value={value}
        onValueChange={(next) => {
          if (!next) return;
          if (next === ADD_MODEL_VALUE) {
            openDialog("add-model", {
              role: modelRole,
              onDownloaded: (_, model) => onChange(model),
            });
            return;
          }
          onChange(next);
        }}
        items={{
          ...Object.fromEntries(names.map((name) => [name, nameLabel(name)])),
          [ADD_MODEL_VALUE]: "Add model",
        }}
        disabled={disabled}
      >
        <SelectTrigger id={id} size="sm" className="w-72">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {names.map((name) => (
            <SelectItem key={name} value={name}>
              {nameLabel(name)}
            </SelectItem>
          ))}
          <SelectSeparator className={"p-0"} />
          <SelectItem
            value={ADD_MODEL_VALUE}
            className={"flex gap-2 items-center hover:rounded-none"}
          >
            <PlusIcon /> Add model
          </SelectItem>
        </SelectContent>
      </Select>
    </div>
  );
}

export function AiCategorizationSection() {
  const c = useCategorizationControls();
  const { shownProvider, llmModel, embeddingModel } = c;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Configurations</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3 text-sm">
        <div className="flex flex-wrap gap-4">
          {embeddingModel !== undefined && (
            <ModelSelect
              id="ai-embedding-model"
              label="Embedding model"
              modelRole="embedding"
              value={embeddingModel}
              installed={c.installedEmbeddings}
              disabled={c.modelsDisabled}
              onChange={(model) => c.applyModel("embedding", model)}
            />
          )}
        </div>
        <div className="flex flex-wrap items-end gap-4">
          <ProviderSelect
            value={shownProvider?.id ?? null}
            providers={c.providers}
            disabled={c.modelsDisabled}
            onChange={c.chooseProvider}
          />
          {shownProvider
            ? llmModel !== undefined && (
                <RemoteModelInput
                  key={shownProvider.id}
                  provider={shownProvider}
                  value={shownProvider === c.savedProvider ? llmModel : ""}
                  disabled={c.modelsDisabled}
                  onApply={(model) =>
                    c.applyRemoteModel(shownProvider.id, model)
                  }
                />
              )
            : llmModel !== undefined && (
                <ModelSelect
                  id="ai-llm-model"
                  label="Language model"
                  modelRole="llm"
                  value={llmModel}
                  installed={c.installedLlms}
                  disabled={c.modelsDisabled}
                  onChange={(model) => c.applyModel("llm", model)}
                />
              )}
        </div>
        {shownProvider && (
          <p className="m-0 text-xs text-muted-foreground">
            Titles, URLs and category names are sent to {shownProvider.name}.
          </p>
        )}
        <AutoCategorizeSetting />
        <div className="flex flex-wrap items-center gap-3">
          <Button disabled={!c.canRun} onClick={() => c.run(false)}>
            {c.runLabel("Categorize", c.running && !c.rechecking)}
          </Button>
          {c.running && (
            <Button
              variant="destructive"
              disabled={c.stopPending}
              onClick={c.stop}
            >
              Stop
            </Button>
          )}
          <Button
            variant="outline"
            disabled={!c.canRecheck}
            onClick={() => c.run(true)}
            title="Re-run the AI labels you have not confirmed, using your latest pins and rules"
          >
            {c.runLabel("Re-check AI labels", c.rechecking)}
          </Button>
          <Button
            variant="destructive"
            className="ml-auto"
            onClick={() => openDialog("reset-categorization")}
          >
            Reset data
          </Button>
        </div>
        {c.blocker && (
          <p className="m-0 text-xs text-muted-foreground">{c.blocker}</p>
        )}
        {c.modelsLoaded && !c.running && (
          <p className="m-0 text-xs text-muted-foreground">
            Models loaded in memory (released after 10 idle minutes).{" "}
            <button
              type="button"
              className="underline hover:text-foreground"
              disabled={c.unloadPending}
              onClick={c.unload}
            >
              Unload
            </button>
          </p>
        )}
        {c.runError != null && (
          <p className="m-0 text-xs text-destructive">{String(c.runError)}</p>
        )}
      </CardContent>
    </Card>
  );
}
