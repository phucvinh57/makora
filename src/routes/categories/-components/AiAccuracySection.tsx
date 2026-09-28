import { useQuery } from "@tanstack/react-query";
import { aiFeedbackStats } from "@/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export function AiAccuracySection() {
  const { data: stats } = useQuery({
    queryKey: ["aiFeedbackStats"],
    queryFn: aiFeedbackStats,
  });
  if (!stats) return null;
  const judged = stats.confirmed + stats.corrected + stats.rejected;
  const pct = (n: number) => `${Math.round((n / judged) * 100)}%`;

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>AI accuracy</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-1 text-sm">
        {judged === 0 ? (
          <p className="m-0 text-muted-foreground">No AI labels judged yet.</p>
        ) : (
          <>
            <p className="m-0">
              <span className="text-2xl font-semibold">
                {pct(stats.confirmed)}
              </span>{" "}
              accepted as-is ({judged} activities judged)
            </p>
            <p className="m-0 text-xs text-muted-foreground">
              {stats.confirmed} accepted · {stats.corrected} corrected (
              {pct(stats.corrected)}) · {stats.rejected} rejected (
              {pct(stats.rejected)})
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}
