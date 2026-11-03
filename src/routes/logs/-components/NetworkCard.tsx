import { ArrowDown, ArrowUp, ShieldCheck } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
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
          <div className="flex items-center gap-2 rounded-md border bg-muted/50 px-3 py-2 text-sm">
            <ShieldCheck className="size-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
            <span>Nothing was sent over the network {periodPhrase}.</span>
          </div>
        ) : (
          <Table className="[&_td:first-child]:pl-0 [&_td:last-child]:pr-0 [&_th:first-child]:pl-0 [&_th:last-child]:pr-0">
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead>Host</TableHead>
                <TableHead>Why</TableHead>
                <TableHead className="text-right">Requests</TableHead>
                <TableHead className="text-right">Sent</TableHead>
                <TableHead className="text-right">Received</TableHead>
                <TableHead className="text-right">Last contact</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {hosts.map((host) => (
                <TableRow key={host.host} className="hover:bg-transparent">
                  <TableCell>
                    <button
                      type="button"
                      onClick={() => onSelectHost(host.host)}
                      className="font-medium hover:underline"
                      title="Show these requests in the log"
                    >
                      {host.host}
                    </button>
                  </TableCell>
                  <TableCell className="whitespace-normal text-muted-foreground">
                    {host.purposes.join(", ") || "—"}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {host.requests.toLocaleString()}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    <span className="inline-flex items-center gap-1">
                      <ArrowUp className="size-3 text-muted-foreground" />
                      {formatSize(host.bytes_sent)}
                    </span>
                    {host.activities_sent > 0 && (
                      <div className="text-xs text-muted-foreground">
                        {host.activities_sent.toLocaleString()} activities
                      </div>
                    )}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    <span className="inline-flex items-center gap-1">
                      <ArrowDown className="size-3 text-muted-foreground" />
                      {formatSize(host.bytes_received)}
                    </span>
                  </TableCell>
                  <TableCell className="text-right text-muted-foreground tabular-nums">
                    {formatRelative(host.last_contact)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
