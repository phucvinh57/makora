import { useInfiniteQuery } from "@tanstack/react-query";
import { format } from "date-fns";
import { Check, Search } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { searchWorkSessions } from "@/api";
import { formatSeconds } from "@/duration";
import { cn } from "@/lib/utils";
import type { WorkSession } from "@/types";
import { SEARCH_DEBOUNCE_MS, WORK_SESSION_PAGE_SIZE } from "./constants";

export function WorkSessionPicker({
  selectedId,
  onSelect,
}: {
  selectedId?: number;
  onSelect: (session: WorkSession) => void;
}) {
  const [searchInput, setSearchInput] = useState("");
  const [search, setSearch] = useState("");

  useEffect(() => {
    const handle = setTimeout(
      () => setSearch(searchInput.trim()),
      SEARCH_DEBOUNCE_MS,
    );
    return () => clearTimeout(handle);
  }, [searchInput]);

  const sessionsQuery = useInfiniteQuery({
    queryKey: ["workSessionSearch", search],
    queryFn: ({ pageParam }) =>
      searchWorkSessions(
        search || undefined,
        WORK_SESSION_PAGE_SIZE,
        pageParam,
      ),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) =>
      lastPage.length < WORK_SESSION_PAGE_SIZE
        ? undefined
        : allPages.reduce((count, page) => count + page.length, 0),
  });

  const sessions = sessionsQuery.data?.pages.flat() ?? [];
  const { fetchNextPage, hasNextPage, isFetchingNextPage } = sessionsQuery;

  const observerRef = useRef<IntersectionObserver | null>(null);
  const sentinelRef = useCallback(
    (node: HTMLDivElement | null) => {
      observerRef.current?.disconnect();
      if (!node) return;
      observerRef.current = new IntersectionObserver((entries) => {
        if (entries[0]?.isIntersecting && hasNextPage && !isFetchingNextPage) {
          fetchNextPage();
        }
      });
      observerRef.current.observe(node);
    },
    [fetchNextPage, hasNextPage, isFetchingNextPage],
  );
  useEffect(() => () => observerRef.current?.disconnect(), []);

  return (
    <div className="flex flex-col gap-2">
      <div className="relative">
        <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
        <Input
          value={searchInput}
          onChange={(e) => setSearchInput(e.target.value)}
          placeholder="Search focus sessions…"
          className="h-8 pl-8 text-sm"
        />
      </div>

      <div className="subtle-scrollbar max-h-56 overflow-y-auto">
        {sessionsQuery.isPending ? (
          <div className="flex flex-col gap-1.5 p-1">
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
            <Skeleton className="h-9 w-full" />
          </div>
        ) : sessionsQuery.error ? (
          <p className="px-2 py-6 text-center text-xs text-destructive">
            Failed to load sessions: {String(sessionsQuery.error)}
          </p>
        ) : sessions.length === 0 ? (
          <p className="px-2 py-6 text-center text-xs text-muted-foreground">
            {search
              ? "No sessions match that search."
              : "No focus sessions yet."}
          </p>
        ) : (
          <ul className="flex flex-col gap-0.5">
            {sessions.map((session) => (
              <li key={session.id}>
                <button
                  type="button"
                  onClick={() => onSelect(session)}
                  className={cn(
                    "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left transition-colors",
                    session.id === selectedId
                      ? "bg-accent"
                      : "hover:bg-accent/50",
                  )}
                >
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-sm">{session.name}</span>
                    <span className="text-xs tabular-nums text-muted-foreground">
                      {format(new Date(session.start_time), "MMM d, HH:mm")} ·{" "}
                      {formatSeconds(session.target_seconds)}
                    </span>
                  </span>
                  {session.id === selectedId && (
                    <Check className="size-3.5 shrink-0 text-muted-foreground" />
                  )}
                </button>
              </li>
            ))}
            <div ref={sentinelRef} className="h-px" />
            {isFetchingNextPage && (
              <li className="px-2 py-1.5">
                <Skeleton className="h-9 w-full" />
              </li>
            )}
          </ul>
        )}
      </div>
    </div>
  );
}
