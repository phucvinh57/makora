import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { createFileRoute } from "@tanstack/react-router";
import { FolderOpen, RefreshCw } from "lucide-react";
import { useState } from "react";
import { toast } from "sonner";
import { logs } from "@/api";
import { Button } from "@/components/ui/button";
import { useDebouncedCallback } from "@/hooks/useDebouncedCallback";
import { cn } from "@/lib/utils";
import {
  logDirQueryOptions,
  logsQueryOptions,
  networkSummaryQueryOptions,
} from "@/queries";
import type { LogArea } from "@/types";
import {
  type GroupBy,
  type MinLevel,
  PERIOD_PHRASES,
  PERIODS,
  type Period,
  SEARCH_DEBOUNCE_MS,
} from "./-components/constants";
import { LogList } from "./-components/LogList";
import { LogsToolbar } from "./-components/LogsToolbar";
import { NetworkCard } from "./-components/NetworkCard";

export const Route = createFileRoute("/logs")({
  component: LogsPage,
});

function LogsPage() {
  const [period, setPeriod] = useState<Period>("7d");
  const [minLevel, setMinLevel] = useState<MinLevel>("info");
  const [areas, setAreas] = useState<LogArea[]>([]);
  const [groupBy, setGroupBy] = useState<GroupBy>("day");
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");
  const commitSearch = useDebouncedCallback(
    (text: string) => setSearch(text.trim()),
    SEARCH_DEBOUNCE_MS,
  );
  function changeSearch(text: string, immediate = false) {
    setSearchInput(text);
    if (immediate) setSearch(text.trim());
    else commitSearch(text);
  }

  const days = PERIODS.find((p) => p.value === period)?.days ?? 7;
  const periodPhrase = PERIOD_PHRASES[period];

  const networkQuery = useQuery(networkSummaryQueryOptions(days));
  const logsQuery = useQuery({
    ...logsQueryOptions({
      days,
      minLevel,
      areas: areas.length > 0 ? areas : undefined,
      search: search || undefined,
    }),
    placeholderData: keepPreviousData,
  });
  const logDir = useQuery(logDirQueryOptions).data;

  const page = logsQuery.data;
  const loading = logsQuery.isFetching || networkQuery.isFetching;
  const filtered = search !== "" || areas.length > 0 || minLevel !== "info";

  function showHost(host: string) {
    changeSearch(host, true);
    setAreas(["network"]);
  }

  async function openFolder() {
    try {
      await logs.openDir();
    } catch (err) {
      toast.error(String(err));
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-center gap-2">
        <div className="mr-auto">
          <h1 className="m-0 text-lg font-semibold">Logs</h1>
          <p className="m-0 text-sm text-muted-foreground">
            What Makora has been doing on this device
          </p>
        </div>
        <Button variant="outline" size="sm" onClick={openFolder} title={logDir}>
          <FolderOpen />
          Open log folder
        </Button>
        <Button
          variant="outline"
          size="sm"
          disabled={loading}
          onClick={() => {
            void logsQuery.refetch();
            void networkQuery.refetch();
          }}
          aria-label="Refresh logs"
        >
          <RefreshCw className={loading ? "animate-spin" : undefined} />
        </Button>
      </header>

      <NetworkCard
        hosts={networkQuery.data}
        periodPhrase={periodPhrase}
        error={networkQuery.error}
        onSelectHost={showHost}
      />

      <LogsToolbar
        searchInput={searchInput}
        onSearchInputChange={(text) => changeSearch(text)}
        period={period}
        onPeriodChange={setPeriod}
        minLevel={minLevel}
        onMinLevelChange={setMinLevel}
        groupBy={groupBy}
        onGroupByChange={setGroupBy}
        areas={areas}
        onAreasChange={setAreas}
        areaCounts={page?.area_counts}
      />

      {logsQuery.error && (
        <div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          Failed to load logs: {String(logsQuery.error)}
        </div>
      )}

      {page && (
        <div
          className={cn(
            "flex flex-col gap-2 transition-opacity",
            logsQuery.isPlaceholderData && "opacity-60",
          )}
        >
          {page.truncated && (
            <p className="m-0 text-xs text-muted-foreground">
              Showing the newest {page.entries.length.toLocaleString()} matching
              entries. Narrow the search or period to see older ones.
            </p>
          )}
          <LogList
            entries={page.entries}
            groupBy={groupBy}
            emptyMessage={
              filtered
                ? `No log entries match these filters ${periodPhrase}.`
                : `Nothing was logged ${periodPhrase}.`
            }
          />
        </div>
      )}
    </div>
  );
}
