import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Trash2Icon } from "lucide-react";
import { useState } from "react";
import {
  createUrlQueryParam,
  deleteUrlQueryParam,
  listUrlQueryParams,
} from "@/api";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";

export function UrlQueryWhitelistSection() {
  const queryClient = useQueryClient();
  const [domain, setDomain] = useState("");
  const [param, setParam] = useState("");
  const paramsQuery = useQuery({
    queryKey: ["urlQueryParams"],
    queryFn: listUrlQueryParams,
  });
  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["urlQueryParams"] });
  const createMutation = useMutation({
    mutationFn: () => createUrlQueryParam(domain.trim(), param.trim()),
    onSuccess: () => {
      setDomain("");
      setParam("");
      invalidate();
    },
  });
  const deleteMutation = useMutation({
    mutationFn: deleteUrlQueryParam,
    onSuccess: invalidate,
  });

  function submit(event: React.FormEvent) {
    event.preventDefault();
    if (domain.trim() && param.trim()) createMutation.mutate();
  }

  return (
    <Card size="sm">
      <CardHeader>
        <CardTitle>URL query whitelist</CardTitle>
        <CardDescription>
          Keep selected query parameters in new site identities. For example,
          YouTube uses <code>v</code> to distinguish videos.
        </CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <form onSubmit={submit} className="flex flex-wrap gap-2">
          <Input
            value={domain}
            onChange={(event) => setDomain(event.target.value)}
            placeholder="github.com"
            className="min-w-40 flex-1 font-mono"
          />
          <Input
            value={param}
            onChange={(event) => setParam(event.target.value)}
            placeholder="tab"
            className="w-24 font-mono"
          />
          <Button
            type="submit"
            disabled={
              createMutation.isPending || !domain.trim() || !param.trim()
            }
          >
            Add
          </Button>
        </form>
        <div className="flex flex-wrap gap-1.5">
          {(paramsQuery.data ?? []).map((item) => (
            <Badge key={item.id} variant="secondary" className="gap-1">
              <span className="font-mono">
                {item.domain}?{item.param}=
              </span>
              <button
                type="button"
                onClick={() => deleteMutation.mutate(item.id)}
                aria-label={`Remove ${item.domain} ${item.param}`}
              >
                <Trash2Icon className="size-3" />
              </button>
            </Badge>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
