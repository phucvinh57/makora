import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
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
import {
  categoryOptions,
  categoryValueLabels,
} from "@/components/CategoryOptions";
import { categorySubtreeIds, userCategories } from "@/categories";
import type { Category } from "@/types";

export const NO_PARENT = "none";

export function CategoryCard({
  category,
  depth,
  subcategoryCount,
  ruleCount,
  categories,
  onSave,
  onDelete,
  onAddSubcategory,
  onShowRules,
}: {
  category: Category;
  depth: number;
  subcategoryCount: number;
  ruleCount: number;
  categories: Category[];
  onSave: (
    category: Category,
    name: string,
    color: string,
    parentId: number | null,
  ) => void;
  onDelete: (category: Category) => void;
  onAddSubcategory: () => void;
  onShowRules: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(category.name);
  const [color, setColor] = useState(category.color);
  const [parentId, setParentId] = useState<number | null>(category.parent_id);

  // Built-in categories keep their name and their place at the root — code looks them up
  // by key. Only the colour is the user's to change.
  const isBuiltIn = category.system_key !== null;

  // Moving a category inside its own branch would cut that branch loose from every root,
  // so its own subtree is not on offer as a new parent. Neither is a built-in, which
  // cannot take children.
  const ownSubtree = categorySubtreeIds(category.id, categories);
  const parentChoices = userCategories(categories).filter(
    (c) => !ownSubtree.has(c.id),
  );

  function startEditing() {
    setName(category.name);
    setColor(category.color);
    setParentId(category.parent_id);
    setEditing(true);
  }

  function saveEditing() {
    if (name.trim()) {
      onSave(category, name.trim(), color, parentId);
    }
    setEditing(false);
  }

  return (
    <Card size="sm" style={{ marginLeft: depth * 24 }}>
      <CardHeader className="items-center">
        {editing ? (
          <div className="flex flex-1 flex-wrap items-center gap-2">
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
              disabled={isBuiltIn}
              className="min-w-40 flex-1"
            />
            <Select
              value={parentId === null ? NO_PARENT : String(parentId)}
              onValueChange={(value) =>
                setParentId(
                  !value || value === NO_PARENT ? null : Number(value),
                )
              }
              items={{
                [NO_PARENT]: "No parent",
                ...categoryValueLabels(parentChoices),
              }}
            >
              <SelectTrigger
                size="sm"
                className="w-44"
                aria-label="Parent category"
                disabled={isBuiltIn}
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value={NO_PARENT}>No parent</SelectItem>
                {categoryOptions(parentChoices)}
              </SelectContent>
            </Select>
            <Button size="sm" onClick={saveEditing}>
              Save
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setEditing(false)}
            >
              Cancel
            </Button>
          </div>
        ) : (
          <>
            <CardTitle className="flex items-center gap-2 leading-none">
              <span
                className="size-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: category.color }}
              />
              {category.name}
              {isBuiltIn && (
                <span className="rounded bg-muted px-1.5 py-0.5 text-xs font-normal text-muted-foreground">
                  built-in
                </span>
              )}
              <button
                type="button"
                onClick={onShowRules}
                className="text-xs font-normal text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
              >
                {ruleCount === 0
                  ? "no rules"
                  : `${ruleCount} rule${ruleCount === 1 ? "" : "s"}`}
              </button>
              {subcategoryCount > 0 && (
                <span className="text-xs font-normal text-muted-foreground">
                  {subcategoryCount} subcategor
                  {subcategoryCount === 1 ? "y" : "ies"}
                </span>
              )}
            </CardTitle>
            <CardAction className="row-span-1 flex items-center gap-2 self-center">
              {!isBuiltIn && (
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Add subcategory of ${category.name}`}
                  title="Add subcategory"
                  onClick={onAddSubcategory}
                >
                  <PlusIcon size={16} />
                </Button>
              )}
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Edit category"
                title={isBuiltIn ? "Change colour" : "Edit category"}
                onClick={startEditing}
              >
                <PencilIcon size={16} />
              </Button>
              {!isBuiltIn && (
                <Button
                  variant="destructive"
                  size="icon-sm"
                  aria-label="Delete category"
                  onClick={() => onDelete(category)}
                >
                  <Trash2Icon size={16} />
                </Button>
              )}
            </CardAction>
          </>
        )}
      </CardHeader>
    </Card>
  );
}
