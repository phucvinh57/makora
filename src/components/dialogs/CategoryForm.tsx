import { useId, useState } from "react";
import type { CategoryDraft } from "@/api";
import { CategoryCombobox } from "@/components/CategoryCombobox";
import {
  type CategoryChoice,
  useCategoryPicker,
} from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
import {
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import type { Category } from "@/types";

const NO_PARENT = "none";
const NO_PARENT_CHOICES: CategoryChoice[] = [
  { value: NO_PARENT, label: "No parent (top level)" },
];

/**
 * The fields shared by adding and editing a category. Dialogs mount it only while open, so
 * every open starts from `initial`.
 */
export function CategoryForm({
  initial,
  parentChoices,
  title,
  submitLabel,
  pending,
  error,
  onSubmit,
  onClose,
}: {
  initial: CategoryDraft;
  /** The categories that may become the parent. */
  parentChoices: Category[];
  /** Given the chosen parent's path, if one is chosen. */
  title: (parentPath: string | undefined) => string;
  submitLabel: string;
  pending: boolean;
  error: unknown;
  onSubmit: (draft: CategoryDraft) => void;
  onClose: () => void;
}) {
  const picker = useCategoryPicker(parentChoices);
  const helpId = useId();
  const [name, setName] = useState(initial.name);
  const [color, setColor] = useState(initial.color);
  const [description, setDescription] = useState(initial.description ?? "");
  const [parentId, setParentId] = useState(initial.parentId);

  const trimmedName = name.trim();
  const parentPath =
    parentId !== null ? picker.pathById.get(parentId) : undefined;

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        if (!trimmedName) return;
        onSubmit({
          name: trimmedName,
          color,
          description: description.trim() || null,
          parentId,
        });
      }}
      className="flex flex-col gap-4"
    >
      <DialogHeader>
        <DialogTitle>{title(parentPath)}</DialogTitle>
      </DialogHeader>
      <div className="flex items-center gap-2">
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
          className="h-8 w-10 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
          aria-label="Category color"
        />
        <Input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Category name"
          aria-label="Category name"
          className="flex-1"
          autoFocus
        />
      </div>
      <Textarea
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="Description (optional)"
        aria-label="Category description"
        aria-describedby={helpId}
      />
      <p id={helpId} className="text-xs text-muted-foreground">
        Describe what belongs here.
      </p>
      <CategoryCombobox
        picker={picker}
        extraChoices={NO_PARENT_CHOICES}
        value={parentId === null ? NO_PARENT : String(parentId)}
        onValueChange={(value) =>
          setParentId(value === NO_PARENT ? null : Number(value))
        }
        ariaLabel="Parent category"
        className="w-full"
      />
      {error != null && (
        <p className="text-xs text-destructive">{String(error)}</p>
      )}
      <DialogFooter>
        <Button type="button" variant="outline" onClick={onClose}>
          Cancel
        </Button>
        <Button type="submit" disabled={!trimmedName || pending}>
          {submitLabel}
        </Button>
      </DialogFooter>
    </form>
  );
}
