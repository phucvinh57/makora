import { ArrowDown, ArrowUp, ShieldCheck } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import type { HostTraffic } from "@/types";
import { formatRelative, formatSize } from "./format";

export function NetworkCard({
  hosts,
  periodPhrase,
  error,
  onSelectHost,
}: {
  hosts: HostTraffic[] | undefined;
  periodPhrase: string;
  error: unknown;
  onSelectHost: (host: string) => void;
}) {
  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>Data that left this device</CardTitle>
        <CardDescription>
          Every server Makora contacted {periodPhrase}. Activity tracking and
          local AI never use the network.
        </CardDescription>
      </CardHeader>
      <CardContent>
        {error ? (
          <p className="m-0 text-sm text-destructive">
            Failed to load network activity: {String(error)}
          </p>
        ) : hosts === undefined ? (
          <Skeleton className="h-10 w-full" />
        ) : hosts.length === 0 ? (
          <div className="flex items-center gap-2 rounded-md bg-muted/50 px-3 py-2.5 text-sm">
            <ShieldCheck className="size-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
            <span>Nothing was sent over the network {periodPhrase}.</span>
          </div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-xs text-muted-foreground">
                  <th className="pb-1.5 font-medium">Host</th>
                  <th className="pb-1.5 font-medium">Why</th>
                  <th className="pb-1.5 text-right font-medium">Requests</th>
                  <th className="pb-1.5 text-right font-medium">Sent</th>
                  <th className="pb-1.5 text-right font-medium">Received</th>
                  <th className="pb-1.5 text-right font-medium">
                    Last contact
                  </th>
                </tr>
              </thead>
              <tbody>
                {hosts.map((host) => (
                  <tr
                    key={host.host}
                    className="border-t align-top first:border-t-0"
                  >
                    <td className="py-1.5 pr-3">
                      <button
                        type="button"
                        onClick={() => onSelectHost(host.host)}
                        className="font-mono text-xs font-medium hover:underline"
                        title="Show these requests in the log"
                      >
                        {host.host}
                      </button>
                    </td>
                    <td className="py-1.5 pr-3 text-muted-foreground">
                      {host.purposes.join(", ") || "—"}
                    </td>
                    <td className="py-1.5 pr-3 text-right tabular-nums">
                      {host.requests.toLocaleString()}
                    </td>
                    <td className="py-1.5 pr-3 text-right tabular-nums">
                      <span className="inline-flex items-center gap-1">
                        <ArrowUp className="size-3 text-muted-foreground" />
                        {formatSize(host.bytes_sent)}
                      </span>
                      {host.activities_sent > 0 && (
                        <div className="text-xs text-muted-foreground">
                          {host.activities_sent.toLocaleString()} activities
                        </div>
                      )}
                    </td>
                    <td className="py-1.5 pr-3 text-right tabular-nums">
                      <span className="inline-flex items-center gap-1">
                        <ArrowDown className="size-3 text-muted-foreground" />
                        {formatSize(host.bytes_received)}
                      </span>
                    </td>
                    <td className="py-1.5 text-right whitespace-nowrap text-muted-foreground tabular-nums">
                      {formatRelative(host.last_contact)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
