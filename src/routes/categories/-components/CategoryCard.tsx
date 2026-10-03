import { PencilIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Card, CardAction, CardHeader, CardTitle } from "@/components/ui/card";
import type { Category } from "@/types";

export function CategoryCard({
  category,
  depth,
  subcategoryCount,
  onEdit,
  onDelete,
  onAddSubcategory,
}: {
  category: Category;
  depth: number;
  subcategoryCount: number;
  onEdit: () => void;
  onDelete: (category: Category) => void;
  onAddSubcategory: () => void;
}) {
  return (
    <Card size="sm" style={{ marginLeft: depth * 24 }}>
      <CardHeader className="items-center">
        <CardTitle className="flex items-center gap-2 leading-none">
          <span
            className="size-2.5 shrink-0 rounded-full"
            style={{ backgroundColor: category.color }}
          />
          {category.name}
          {subcategoryCount > 0 && (
            <span className="shrink-0 whitespace-nowrap text-xs font-normal text-muted-foreground">
              {subcategoryCount}{" "}
              {subcategoryCount === 1 ? "subcategory" : "subcategories"}
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
            onClick={onEdit}
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
      </CardHeader>
    </Card>
  );
}
