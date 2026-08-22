import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  assignActivityCluster,
  getClassifierStatus,
  listActivityClusters,
  listCategories,
  runAiCategorization,
  setAiClusteringGranularity,
} from "@/api";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { formatSeconds } from "@/duration";
import { invalidateLabelingQueries } from "@/queries";
import type { ActivityCluster, ClusteringGranularity } from "@/types";

const granularityLabels: Record<ClusteringGranularity, string> = {
  broad: "Broad",
  balanced: "Balanced",
  fine: "Fine",
};

function ClusterCard({
  cluster,
  categories,
  selectedMemberIds,
  categoryId,
  expanded,
  assigning,
  onToggleExpanded,
  onToggleMember,
  onCategoryChange,
  onAssign,
}: {
  cluster: ActivityCluster;
  categories: Parameters<typeof categoryOptions>[0];
  selectedMemberIds: Set<number>;
  categoryId: string | null;
  expanded: boolean;
  assigning: boolean;
  onToggleExpanded: () => void;
  onToggleMember: (memberId: number, checked: boolean) => void;
  onCategoryChange: (value: string) => void;
  onAssign: () => void;
}) {
  return (
    <Card size="sm">
      <CardHeader>
        <div className="flex flex-wrap items-start justify-between gap-2">
          <div className="min-w-0">
            <CardTitle className="truncate">{cluster.label}</CardTitle>
            <CardDescription className="mt-1">
              {cluster.scope === "title" && cluster.parent_key
                ? `${cluster.parent_kind}: ${cluster.parent_key} · `
                : ""}
              {formatSeconds(cluster.seconds)} tracked · cohesion{" "}
              {Math.round(cluster.cohesion * 100)}%
            </CardDescription>
          </div>
          <div className="flex items-center gap-1.5">
            <Badge variant="secondary">{cluster.member_count} activities</Badge>
            {cluster.lead != null && cluster.lead < 0.15 ? (
              <Badge variant="outline">low confidence</Badge>
            ) : null}
          </div>
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <Select
            value={categoryId}
            onValueChange={(value) => value && onCategoryChange(value)}
            items={categoryValueLabels(categories)}
          >
            <SelectTrigger size="sm" className="min-w-52">
              <SelectValue placeholder="Choose category" />
            </SelectTrigger>
            <SelectContent>{categoryOptions(categories)}</SelectContent>
          </Select>
          <Button size="sm" variant="outline" onClick={onToggleExpanded}>
            {expanded ? "Hide members" : "Show members"}
          </Button>
          <Button
            size="sm"
            disabled={assigning || !categoryId || selectedMemberIds.size === 0}
            onClick={onAssign}
          >
            Assign {selectedMemberIds.size}
          </Button>
        </div>
        {expanded ? (
          <div className="flex max-h-72 flex-col gap-1 overflow-y-auto rounded-lg border p-2">
            {cluster.members.map((member) => {
              const title = member.window_title?.trim() || member.key;
              return (
                <label
                  key={member.id}
                  htmlFor={`cluster-member-${member.id}`}
                  className="flex items-start gap-2 rounded-md p-2 text-sm hover:bg-muted/50"
                >
                  <Checkbox
                    id={`cluster-member-${member.id}`}
                    checked={selectedMemberIds.has(member.id)}
                    onCheckedChange={(checked) =>
                      onToggleMember(member.id, checked)
                    }
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{title}</span>
                    <span className="block truncate text-xs text-muted-foreground">
                      {member.window_title ? member.key : member.key_kind} ·{" "}
                      {formatSeconds(member.seconds)}
                    </span>
                  </span>
                </label>
              );
            })}
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}

function ClusterSection({
  title,
  clusters,
  groupByParent = false,
  categories,
  selected,
  categorySelections,
  expanded,
  assigning,
  onToggleExpanded,
  onToggleMember,
  onCategoryChange,
  onAssign,
}: {
  title: string;
  clusters: ActivityCluster[];
  groupByParent?: boolean;
  categories: Parameters<typeof categoryOptions>[0];
  selected: Record<number, Set<number>>;
  categorySelections: Record<number, string>;
  expanded: Set<number>;
  assigning: boolean;
  onToggleExpanded: (id: number) => void;
  onToggleMember: (
    clusterId: number,
    memberId: number,
    checked: boolean,
  ) => void;
  onCategoryChange: (id: number, value: string) => void;
  onAssign: (cluster: ActivityCluster) => void;
}) {
  if (clusters.length === 0) return null;
  const groups = groupByParent
    ? [
        ...clusters.reduce((map, cluster) => {
          const key = cluster.parent_key ?? "Unknown parent";
          const group = map.get(key) ?? [];
          group.push(cluster);
          map.set(key, group);
          return map;
        }, new Map<string, ActivityCluster[]>()),
      ]
    : [["", clusters] as [string, ActivityCluster[]]];
  return (
    <section className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">{title}</h2>
      {groups.map(([parent, group]) => (
        <div key={parent || "all"} className="flex flex-col gap-3">
          {parent ? (
            <h3 className="font-mono text-xs text-muted-foreground">
              {parent}
            </h3>
          ) : null}
          {group.map((cluster) => (
            <ClusterCard
              key={cluster.id}
              cluster={cluster}
              categories={categories}
              selectedMemberIds={selected[cluster.id] ?? new Set()}
              categoryId={
                categorySelections[cluster.id] ??
                (cluster.category_id == null
                  ? null
                  : String(cluster.category_id))
              }
              expanded={expanded.has(cluster.id)}
              assigning={assigning}
              onToggleExpanded={() => onToggleExpanded(cluster.id)}
              onToggleMember={(memberId, checked) =>
                onToggleMember(cluster.id, memberId, checked)
              }
              onCategoryChange={(value) => onCategoryChange(cluster.id, value)}
              onAssign={() => onAssign(cluster)}
            />
          ))}
        </div>
      ))}
    </section>
  );
}

export function ClustersTab() {
  const queryClient = useQueryClient();
  const [selected, setSelected] = useState<Record<number, Set<number>>>({});
  const [categorySelections, setCategorySelections] = useState<
    Record<number, string>
  >({});
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const clustersQuery = useQuery({
    queryKey: ["activityClusters"],
    queryFn: listActivityClusters,
  });
  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const statusQuery = useQuery({
    queryKey: ["classifierStatus"],
    queryFn: getClassifierStatus,
    refetchInterval: (query) =>
      query.state.data?.state === "queued" ||
      query.state.data?.state === "running"
        ? 1000
        : false,
  });
  const run = useMutation({
    mutationFn: runAiCategorization,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] }),
  });
  const setGranularity = useMutation({
    mutationFn: setAiClusteringGranularity,
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["classifierStatus"] }),
  });
  const assign = useMutation({
    mutationFn: ({
      clusterId,
      categoryId,
      memberIds,
    }: {
      clusterId: number;
      categoryId: number;
      memberIds: number[];
    }) => assignActivityCluster(clusterId, categoryId, memberIds),
    onSuccess: (result) => {
      invalidateLabelingQueries(queryClient);
      toast.success("Cluster assigned", {
        description: `${result.rules_created} rules · ${result.sessions_pinned} sessions pinned`,
      });
    },
  });
  const clusters = clustersQuery.data ?? [];
  const categories = categoriesQuery.data ?? [];
  const status = statusQuery.data;
  const keyClusters = useMemo(
    () => clusters.filter((cluster) => cluster.scope === "key"),
    [clusters],
  );
  const titleClusters = useMemo(
    () => clusters.filter((cluster) => cluster.scope === "title"),
    [clusters],
  );

  useEffect(() => {
    setSelected((current) => {
      const next = { ...current };
      for (const cluster of clusters) {
        if (!next[cluster.id])
          next[cluster.id] = new Set(
            cluster.members.map((member) => member.id),
          );
      }
      return next;
    });
  }, [clusters]);

  function toggleMember(clusterId: number, memberId: number, checked: boolean) {
    setSelected((current) => {
      const members = new Set(current[clusterId]);
      if (checked) members.add(memberId);
      else members.delete(memberId);
      return { ...current, [clusterId]: members };
    });
  }

  function assignCluster(cluster: ActivityCluster) {
    const categoryId = Number(
      categorySelections[cluster.id] ?? cluster.category_id,
    );
    if (!Number.isFinite(categoryId)) return;
    assign.mutate({
      clusterId: cluster.id,
      categoryId,
      memberIds: [...(selected[cluster.id] ?? new Set())],
    });
  }

  const busy = status?.state === "queued" || status?.state === "running";
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto pr-1">
      <Card size="sm">
        <CardHeader>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div>
              <CardTitle>Clusters</CardTitle>
              <CardDescription className="mt-1">
                {clusters.length} groups ·{" "}
                {clusters.reduce(
                  (sum, cluster) => sum + cluster.member_count,
                  0,
                )}{" "}
                activities
              </CardDescription>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <Select
                value={status?.cluster_granularity ?? "balanced"}
                disabled={setGranularity.isPending}
                onValueChange={(value) =>
                  value && setGranularity.mutate(value as ClusteringGranularity)
                }
              >
                <SelectTrigger size="sm" className="w-28">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {(
                    Object.keys(granularityLabels) as ClusteringGranularity[]
                  ).map((value) => (
                    <SelectItem key={value} value={value}>
                      {granularityLabels[value]}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <Button
                size="sm"
                disabled={busy || run.isPending}
                onClick={() => run.mutate()}
              >
                {busy ? "Running…" : "Run now"}
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent className="text-xs text-muted-foreground">
          Broad makes fewer, larger groups. Fine makes more, tighter groups.
          Re-run AI to apply.
        </CardContent>
      </Card>

      {clustersQuery.isLoading ? (
        <p className="text-sm text-muted-foreground">Loading clusters…</p>
      ) : null}
      {clustersQuery.error || assign.error || setGranularity.error ? (
        <p className="text-xs text-destructive">
          {String(clustersQuery.error ?? assign.error ?? setGranularity.error)}
        </p>
      ) : null}
      <ClusterSection
        title="Apps & sites"
        clusters={keyClusters}
        categories={categories}
        selected={selected}
        categorySelections={categorySelections}
        expanded={expanded}
        assigning={assign.isPending}
        onToggleExpanded={(id) =>
          setExpanded((current) => {
            const next = new Set(current);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
          })
        }
        onToggleMember={toggleMember}
        onCategoryChange={(id, value) =>
          setCategorySelections((current) => ({ ...current, [id]: value }))
        }
        onAssign={assignCluster}
      />
      <ClusterSection
        title="Activities inside multi-purpose sites"
        clusters={titleClusters}
        groupByParent
        categories={categories}
        selected={selected}
        categorySelections={categorySelections}
        expanded={expanded}
        assigning={assign.isPending}
        onToggleExpanded={(id) =>
          setExpanded((current) => {
            const next = new Set(current);
            if (next.has(id)) next.delete(id);
            else next.add(id);
            return next;
          })
        }
        onToggleMember={toggleMember}
        onCategoryChange={(id, value) =>
          setCategorySelections((current) => ({ ...current, [id]: value }))
        }
        onAssign={assignCluster}
      />
      {!clustersQuery.isLoading && clusters.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          No clusters yet. Run AI after recording enough unlabeled activity.
        </p>
      ) : null}
    </div>
  );
}
