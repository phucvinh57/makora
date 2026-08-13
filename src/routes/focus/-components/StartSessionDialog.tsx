import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import { toast } from "sonner";
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
import { createWorkSession, listCategories } from "@/api";
import { useDialog } from "@/hooks/useDialog";
import { CategoryPicker } from "./CategoryPicker";
import { WORK_SESSIONS_KEY } from "./constants";

const DEFAULT_MINUTES = 50;

export function StartSessionDialog() {
  const queryClient = useQueryClient();
  const dialog = useDialog("start-work-session");
  const fieldId = useId();
  const [name, setName] = useState("");
  const [note, setNote] = useState("");
  const [minutes, setMinutes] = useState(String(DEFAULT_MINUTES));
  const [categoryIds, setCategoryIds] = useState<number[]>([]);

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });

  const createMutation = useMutation({
    mutationFn: () =>
      createWorkSession(
        name.trim(),
        note.trim() ? note.trim() : null,
        Math.round(Number(minutes) * 60),
        categoryIds,
      ),
    onSuccess: () => {
      toast.success("Focus session started");
      queryClient.invalidateQueries({ queryKey: WORK_SESSIONS_KEY });
      dialog.setOpen(false);
      setName("");
      setNote("");
      setMinutes(String(DEFAULT_MINUTES));
      setCategoryIds([]);
    },
    onError: (error) => toast.error(String(error)),
  });

  const minutesValue = Number(minutes);
  const valid =
    name.trim().length > 0 && Number.isFinite(minutesValue) && minutesValue > 0;

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!valid) return;
    createMutation.mutate();
  }

  return (
    <Dialog open={dialog.open} onOpenChange={dialog.setOpen}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Start a focus session</DialogTitle>
          <DialogDescription>
            Set a goal and pick which categories count as on-task. The session
            ends by itself when the time is up.
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
              placeholder="e.g. Finish the release notes"
              autoFocus
            />
          </label>
          <label
            htmlFor={`${fieldId}-minutes`}
            className="flex flex-col gap-1.5 text-sm font-medium"
          >
            Duration (minutes)
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
              placeholder="Anything to remember about this block"
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
            <Button type="submit" disabled={!valid || createMutation.isPending}>
              Start session
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
