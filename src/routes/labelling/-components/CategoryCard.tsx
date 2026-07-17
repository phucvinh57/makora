import { PencilIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import type { Category } from "@/types";

export function CategoryCard({
  category,
  ruleCount,
  onRename,
  onDelete,
  onShowRules,
}: {
  category: Category;
  ruleCount: number;
  onRename: (category: Category, name: string, color: string) => void;
  onDelete: (category: Category) => void;
  onShowRules: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(category.name);
  const [color, setColor] = useState(category.color);

  function startEditing() {
    setName(category.name);
    setColor(category.color);
    setEditing(true);
  }

  function saveEditing() {
    if (name.trim()) {
      onRename(category, name.trim(), color);
    }
    setEditing(false);
  }

  return (
    <Card size="sm">
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
              className="min-w-40 flex-1"
            />
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
              <button
                type="button"
                onClick={onShowRules}
                className="text-xs font-normal text-muted-foreground underline-offset-2 hover:text-foreground hover:underline"
              >
                {ruleCount === 0
                  ? "no rules"
                  : `${ruleCount} rule${ruleCount === 1 ? "" : "s"}`}
              </button>
            </CardTitle>
            <CardAction className="row-span-1 flex items-center gap-2 self-center">
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Edit category"
                onClick={startEditing}
              >
                <PencilIcon size={16} />
              </Button>
              <Button
                variant="destructive"
                size="icon-sm"
                aria-label="Delete category"
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
