import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { format } from "date-fns";
import { useEffect, useId, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DateTimePicker } from "@/components/ui/date-time-picker";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { listCategories, updateWorkSession } from "@/api";
import { useDialog } from "@/hooks/useDialog";
import type { WorkSession } from "@/types";
import { CategoryPicker } from "./CategoryPicker";
import { WORK_SESSIONS_KEY, workSessionStatsKey } from "./constants";

/** RFC 3339 UTC → the local wall-clock value `DateTimePicker` reads. */
function toLocalInput(value: string) {
  return format(new Date(value), "yyyy-MM-dd'T'HH:mm");
}

/** Local wall-clock picker value → RFC 3339 UTC. */
function toUtc(value: string) {
  return new Date(value).toISOString();
}

export function EditSessionDialog() {
  const queryClient = useQueryClient();
  const dialog = useDialog<WorkSession>("edit-work-session");
  const session = dialog.data;
  const fieldId = useId();
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [minutes, setMinutes] = useState("");
  const [start, setStart] = useState("");
  const [end, setEnd] = useState("");
  const [categoryIds, setCategoryIds] = useState<number[]>([]);

  // Re-seed the form whenever a (possibly different) session is opened.
  useEffect(() => {
    if (!session || !dialog.open) return;
    setName(session.name);
    setNote(session.note ?? "");
    setMinutes(String(Math.round(session.target_seconds / 60)));
    setStart(toLocalInput(session.start_time));
    setEnd(toLocalInput(session.end_time));
    setCategoryIds(session.category_ids);
  }, [session, dialog.open]);

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const updateMutation = useMutation({
    mutationFn: () => {
      if (!session) return Promise.reject(new Error("no session"));
      return updateWorkSession({
        id: session.id,
        name: name.trim(),
        note: note.trim() ? note.trim() : null,
        target_seconds: Math.round(Number(minutes) * 60),
        start_time: toUtc(start),
        end_time: toUtc(end),
        category_ids: categoryIds,
      });
    },
    onSuccess: () => {
      toast.success("Session updated");
      queryClient.invalidateQueries({ queryKey: WORK_SESSIONS_KEY });
      if (session) {
        queryClient.invalidateQueries({
          queryKey: workSessionStatsKey(session.id),
        });
      }
      dialog.setOpen(false);
    },
    onError: (error) => toast.error(String(error)),
  });

  const minutesValue = Number(minutes);
  const valid =
    name.trim().length > 0 &&
    Number.isFinite(minutesValue) &&
    minutesValue > 0 &&
    start !== "" &&
    end !== "" &&
    new Date(end) > new Date(start);

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!valid) return;
    updateMutation.mutate();
  }

  return (
    <Dialog open={dialog.open} onOpenChange={dialog.setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit session</DialogTitle>
          <DialogDescription>
            Every field is yours to change, including the time window the
            session is evaluated over.
          </DialogDescription>
        </DialogHeader>
        <form className="flex flex-col gap-4" onSubmit={handleSubmit}>
          <label
            htmlFor={`${fieldId}-name`}
            className="flex flex-col gap-1.5 text-sm font-medium"
          >
            Goal
            <Input
              id={`${fieldId}-name`}
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <div className="grid grid-cols-2 gap-3">
            <label
              htmlFor={`${fieldId}-start`}
              className="flex flex-col gap-1.5 text-sm font-medium"
            >
              Start
              <DateTimePicker
                id={`${fieldId}-start`}
                value={start}
                onValueChange={setStart}
              />
            </label>
            <label
              htmlFor={`${fieldId}-end`}
              className="flex flex-col gap-1.5 text-sm font-medium"
            >
              End
              <DateTimePicker
                id={`${fieldId}-end`}
                value={end}
                onValueChange={setEnd}
              />
            </label>
          </div>
          <label
            htmlFor={`${fieldId}-minutes`}
            className="flex flex-col gap-1.5 text-sm font-medium"
          >
            Planned duration (minutes)
            <Input
              id={`${fieldId}-minutes`}
              type="number"
              min={1}
              value={minutes}
              onChange={(e) => setMinutes(e.target.value)}
            />
          </label>
          <div className="flex flex-col gap-1.5">
            <span className="text-sm font-medium">On-task categories</span>
            <CategoryPicker
              categories={categoriesQuery.data ?? []}
              selected={categoryIds}
              onChange={setCategoryIds}
            />
          </div>
          <label
            htmlFor={`${fieldId}-note`}
            className="flex flex-col gap-1.5 text-sm font-medium"
          >
            Note (optional)
            <Input
              id={`${fieldId}-note`}
              value={note}
              onChange={(e) => setNote(e.target.value)}
            />
          </label>
          <DialogFooter>
            <Button
              type="button"
              variant="ghost"
              onClick={() => dialog.setOpen(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!valid || updateMutation.isPending}>
              Save changes
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
