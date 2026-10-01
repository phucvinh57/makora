import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { Category } from "@/types";

export function DeleteCategoryDialog({
  open,
  onOpenChange,
  category,
  descendantCount,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  category: Category | null;
  /** How many categories sit under this one and go with it. */
  descendantCount: number;
  onConfirm: (category: Category) => void;
}) {
  const branch = descendantCount > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {branch
              ? `Delete ${category?.name} and everything under it?`
              : `Delete ${category?.name}?`}
          </DialogTitle>
          <DialogDescription>
            {branch
              ? `This also deletes ${descendantCount} subcategor${
                  descendantCount === 1 ? "y" : "ies"
                } and their pins. `
              : "Pins to this category are cleared. "}
            This cannot be undone.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="ghost"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            type="button"
            variant="destructive"
            onClick={() => category && onConfirm(category)}
          >
            {branch ? "Delete branch" : "Delete"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
