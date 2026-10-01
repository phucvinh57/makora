import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useMemo, useState } from "react";
import type { CategoryDraft } from "@/api";
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { categorySubtreeIds } from "@/categories";
import type { Category } from "@/types";

export const NO_PARENT = "none";

/**
 * Mounted only while editing, so its drafts start from the category each time and the
 * parent-choice work is not done for every card on the page.
 */
function CategoryEditForm({
  category,
  categories,
  onSave,
  onCancel,
}: {
  category: Category;
  categories: Category[];
  onSave: (draft: CategoryDraft) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(category.name);
  const [color, setColor] = useState(category.color);
  const [description, setDescription] = useState(category.description ?? "");
  const [parentId, setParentId] = useState<number | null>(category.parent_id);

  // Moving a category inside its own branch would cut that branch loose from every root,
  // so its own subtree is not on offer as a new parent.
  const parentChoices = useMemo(() => {
    const ownSubtree = categorySubtreeIds(category.id, categories);
    return categories.filter((c) => !ownSubtree.has(c.id));
  }, [category.id, categories]);
  const options = useMemo(
    () => categoryOptions(parentChoices),
    [parentChoices],
  );
  const valueLabels = useMemo(
    () => categoryValueLabels(parentChoices),
    [parentChoices],
  );

  const trimmedName = name.trim();

  return (
    <form
      className="flex flex-1 flex-wrap items-center gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (!trimmedName) return;
        onSave({
          name: trimmedName,
          color,
          description: description.trim() || null,
          parentId,
        });
      }}
      onKeyDown={(event) => {
        // Portaled dropdowns bubble React events here too; only the form's own keys count.
        if (
          event.key === "Escape" &&
          event.currentTarget.contains(event.target as Node)
        ) {
          onCancel();
        }
      }}
    >
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
        aria-label="Category name"
        autoFocus
        className="min-w-40 flex-1"
      />
      <Input
        type="text"
        value={description}
        onChange={(e) => setDescription(e.target.value)}
        placeholder="What this category means"
        aria-label="Category description"
        className="min-w-56 flex-1"
      />
      <Select
        value={parentId === null ? NO_PARENT : String(parentId)}
        onValueChange={(value) =>
          setParentId(!value || value === NO_PARENT ? null : Number(value))
        }
        items={{ [NO_PARENT]: "No parent", ...valueLabels }}
      >
        <SelectTrigger size="sm" className="w-44" aria-label="Parent category">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={NO_PARENT}>No parent</SelectItem>
          {options}
        </SelectContent>
      </Select>
      <Button type="submit" size="sm" disabled={!trimmedName}>
        Save
      </Button>
      <Button type="button" variant="outline" size="sm" onClick={onCancel}>
        Cancel
      </Button>
    </form>
  );
}

export function CategoryCard({
  category,
  depth,
  subcategoryCount,
  categories,
  onSave,
  onDelete,
  onAddSubcategory,
}: {
  category: Category;
  depth: number;
  subcategoryCount: number;
  categories: Category[];
  onSave: (category: Category, draft: CategoryDraft) => void;
  onDelete: (category: Category) => void;
  onAddSubcategory: () => void;
}) {
  const [editing, setEditing] = useState(false);

  return (
    <Card size="sm" style={{ marginLeft: depth * 24 }}>
      <CardHeader className="items-center">
        {editing ? (
          <CategoryEditForm
            category={category}
            categories={categories}
            onSave={(draft) => {
              onSave(category, draft);
              setEditing(false);
            }}
            onCancel={() => setEditing(false)}
          />
        ) : (
          <>
            <CardTitle className="flex items-center gap-2 leading-none">
              <span
                className="size-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: category.color }}
              />
              {category.name}
              {subcategoryCount > 0 && (
                <span className="text-xs font-normal text-muted-foreground">
                  {subcategoryCount} subcategor
                  {subcategoryCount === 1 ? "y" : "ies"}
                </span>
              )}
            </CardTitle>
            {category.description && (
              <p className="text-xs text-muted-foreground">
                {category.description}
              </p>
            )}
            <CardAction className="row-span-1 flex items-center gap-2 self-center">
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Add subcategory of ${category.name}`}
                title="Add subcategory"
                onClick={onAddSubcategory}
              >
                <PlusIcon size={16} />
              </Button>
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label={`Edit ${category.name}`}
                title="Edit category"
                onClick={() => setEditing(true)}
              >
                <PencilIcon size={16} />
              </Button>
              <Button
                variant="destructive"
                size="icon-sm"
                aria-label={`Delete ${category.name}`}
                title="Delete category"
                onClick={() => onDelete(category)}
              >
                <Trash2Icon size={16} />
              </Button>
            </CardAction>
          </>
        )}
      </CardHeader>
    </Card>
  );
}
