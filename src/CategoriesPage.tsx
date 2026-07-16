import {
  closestCenter,
  DndContext,
  type DragEndEvent,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  arrayMove,
  SortableContext,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ChevronRightIcon,
  GripVerticalIcon,
  PencilIcon,
  Trash2Icon,
} from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  createCategory,
  createCategoryRule,
  deleteCategory,
  deleteCategoryRule,
  listCategories,
  listCategoryRules,
  reorderCategories,
  updateCategory,
} from "./api";
import type { Category, CategoryRule, KeyKind, MatchKind } from "./types";

const MATCH_KIND_LABEL: Record<MatchKind, string> = {
  exact: "is",
  substring: "contains",
  regex: "matches",
};

function matchKindPlaceholder(keyKind: KeyKind, matchKind: MatchKind): string {
  if (matchKind === "regex") {
    return keyKind === "exe"
      ? "^game\\d*\\.exe$"
      : "^.*\\.corp\\.example\\.com$";
  }
  if (matchKind === "substring") {
    return keyKind === "exe" ? "steam" : "google.com";
  }
  return keyKind === "exe" ? "code.exe" : "github.com";
}

const DEFAULT_COLOR = "#6366f1";

export default function CategoriesPage() {
  const queryClient = useQueryClient();

  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(DEFAULT_COLOR);

  const categoriesQuery = useQuery({
    queryKey: ["categories"],
    queryFn: listCategories,
  });
  const rulesQuery = useQuery({
    queryKey: ["categoryRules"],
    queryFn: listCategoryRules,
  });

  const categories = categoriesQuery.data ?? [];
  const rules = rulesQuery.data ?? [];
  const loading = categoriesQuery.isLoading || rulesQuery.isLoading;
  const error =
    (categoriesQuery.error && String(categoriesQuery.error)) ||
    (rulesQuery.error && String(rulesQuery.error)) ||
    null;

  function invalidateAll() {
    queryClient.invalidateQueries({ queryKey: ["categories"] });
    queryClient.invalidateQueries({ queryKey: ["categoryRules"] });
  }

  const createCategoryMutation = useMutation({
    mutationFn: () => createCategory(newName.trim(), newColor),
    onSuccess: () => {
      setNewName("");
      setNewColor(DEFAULT_COLOR);
      invalidateAll();
    },
  });

  const renameCategoryMutation = useMutation({
    mutationFn: ({
      category,
      name,
      color,
    }: {
      category: Category;
      name: string;
      color: string;
    }) => updateCategory(category.id, name, color),
    onSuccess: invalidateAll,
  });

  const deleteCategoryMutation = useMutation({
    mutationFn: (category: Category) => deleteCategory(category.id),
    onSuccess: invalidateAll,
  });

  const addRuleMutation = useMutation({
    mutationFn: ({
      categoryId,
      keyKind,
      pattern,
      matchKind,
    }: {
      categoryId: number;
      keyKind: KeyKind;
      pattern: string;
      matchKind: MatchKind;
    }) => {
      // Lowercasing canonicalizes exact/substring patterns (matching is
      // case-insensitive either way), but would corrupt case-sensitive
      // regex escapes like `\D`/`\S`/`\W`, so regex patterns are sent as-is.
      const normalized =
        matchKind === "regex" ? pattern.trim() : pattern.trim().toLowerCase();
      return createCategoryRule(keyKind, normalized, matchKind, categoryId);
    },
    onSuccess: invalidateAll,
  });

  const deleteRuleMutation = useMutation({
    mutationFn: (rule: CategoryRule) => deleteCategoryRule(rule.id),
    onSuccess: invalidateAll,
  });

  const reorderCategoriesMutation = useMutation({
    mutationFn: (orderedIds: number[]) => reorderCategories(orderedIds),
    onMutate: async (orderedIds: number[]) => {
      await queryClient.cancelQueries({ queryKey: ["categories"] });
      const previous = queryClient.getQueryData<Category[]>(["categories"]);
      const priorityById = new Map(orderedIds.map((id, index) => [id, index]));
      queryClient.setQueryData<Category[]>(["categories"], (old) =>
        old
          ?.map((c) =>
            priorityById.has(c.id)
              ? { ...c, priority: priorityById.get(c.id) as number }
              : c,
          )
          .sort((a, b) => a.priority - b.priority),
      );
      return { previous };
    },
    onError: (_err, _vars, context) => {
      if (context?.previous) {
        queryClient.setQueryData(["categories"], context.previous);
      }
    },
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: ["categories"] });
    },
  });

  function handleCreateCategory(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    createCategoryMutation.mutate();
  }

  const categorySensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
  );

  function handleCategoryDragEnd(event: DragEndEvent) {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = categories.findIndex((c) => c.id === active.id);
    const newIndex = categories.findIndex((c) => c.id === over.id);
    if (oldIndex === -1 || newIndex === -1) return;
    reorderCategoriesMutation.mutate(
      arrayMove(categories, oldIndex, newIndex).map((c) => c.id),
    );
  }

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <h1 className="m-0 text-lg font-semibold">Categories</h1>
        <p className="m-0 text-sm text-muted-foreground">Categories & rules</p>
      </header>

      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <form
        onSubmit={handleCreateCategory}
        className="mb-4 flex flex-wrap items-center gap-2"
      >
        <input
          type="color"
          value={newColor}
          onChange={(e) => setNewColor(e.target.value)}
          className="h-8 w-10 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
          aria-label="New category color"
        />
        <Input
          type="text"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          placeholder="New category name"
          className="w-56"
        />
        <Button type="submit" size="sm" disabled={!newName.trim()}>
          Add category
        </Button>
      </form>

      {categories.length > 1 && (
        <p className="mb-3 text-xs text-muted-foreground">
          Drag a category to reorder it. When rules in more than one category
          match the same app or site, the category closer to the top wins.
        </p>
      )}

      <div className="flex-1 overflow-y-auto subtle-scrollbar">
        {!loading && categories.length === 0 && (
          <p className="text-sm text-muted-foreground">No categories yet.</p>
        )}
        <DndContext
          sensors={categorySensors}
          collisionDetection={closestCenter}
          onDragEnd={handleCategoryDragEnd}
        >
          <SortableContext
            items={categories.map((c) => c.id)}
            strategy={verticalListSortingStrategy}
          >
            <div className="flex flex-col gap-3">
              {categories.map((category) => (
                <SortableCategoryCard
                  key={category.id}
                  category={category}
                  rules={rules.filter((r) => r.category_id === category.id)}
                  onRename={(cat, name, color) =>
                    renameCategoryMutation.mutate({
                      category: cat,
                      name,
                      color,
                    })
                  }
                  onDelete={(cat) => deleteCategoryMutation.mutate(cat)}
                  onAddRule={(categoryId, keyKind, pattern, matchKind) => {
                    if (!pattern.trim()) return;
                    addRuleMutation.mutate({
                      categoryId,
                      keyKind,
                      pattern,
                      matchKind,
                    });
                  }}
                  onDeleteRule={(rule) => deleteRuleMutation.mutate(rule)}
                />
              ))}
            </div>
          </SortableContext>
        </DndContext>
      </div>
    </div>
  );
}

function CategoryCard({
  category,
  rules,
  onRename,
  onDelete,
  onAddRule,
  onDeleteRule,
  dragHandleProps,
}: {
  category: Category;
  rules: CategoryRule[];
  onRename: (category: Category, name: string, color: string) => void;
  onDelete: (category: Category) => void;
  onAddRule: (
    categoryId: number,
    keyKind: KeyKind,
    pattern: string,
    matchKind: MatchKind,
  ) => void;
  onDeleteRule: (rule: CategoryRule) => void;
  dragHandleProps?: React.HTMLAttributes<HTMLButtonElement>;
}) {
  const [expanded, setExpanded] = useState(false);
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(category.name);
  const [color, setColor] = useState(category.color);
  const [pattern, setPattern] = useState("");
  const [keyKind, setKeyKind] = useState<KeyKind>("exe");
  const [matchKind, setMatchKind] = useState<MatchKind>("exact");

  function startEditing() {
    setName(category.name);
    setColor(category.color);
    setEditing(true);
    setExpanded(true);
  }

  function saveEditing() {
    if (name.trim()) {
      onRename(category, name.trim(), color);
    }
    setEditing(false);
  }

  function handleAddRule(e: React.FormEvent) {
    e.preventDefault();
    onAddRule(category.id, keyKind, pattern, matchKind);
    setPattern("");
  }

  return (
    <Card size="sm">
      <CardHeader
        className={
          editing ? "items-center" : "cursor-pointer items-center select-none"
        }
        onClick={editing ? undefined : () => setExpanded((v) => !v)}
      >
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
              <button
                type="button"
                {...dragHandleProps}
                onClick={(e) => e.stopPropagation()}
                aria-label={`Reorder category ${category.name}`}
                className="cursor-grab touch-none text-muted-foreground active:cursor-grabbing"
              >
                <GripVerticalIcon size={14} />
              </button>
              <ChevronRightIcon
                className={`size-4 shrink-0 text-muted-foreground transition-transform ${
                  expanded ? "rotate-90" : ""
                }`}
              />
              <span
                className="size-2.5 shrink-0 rounded-full"
                style={{ backgroundColor: category.color }}
              />
              {category.name}
              <span className="text-xs font-normal text-muted-foreground">
                {rules.length === 0
                  ? "no rules"
                  : `${rules.length} rule${rules.length === 1 ? "" : "s"}`}
              </span>
            </CardTitle>
            <CardAction className="row-span-1 flex items-center gap-2 self-center">
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="Edit category"
                onClick={(e) => {
                  e.stopPropagation();
                  startEditing();
                }}
              >
                <PencilIcon size={16} />
              </Button>
              <Button
                variant="destructive"
                size="icon-sm"
                aria-label="Delete category"
                onClick={(e) => {
                  e.stopPropagation();
                  onDelete(category);
                }}
              >
                <Trash2Icon size={16} />
              </Button>
            </CardAction>
          </>
        )}
      </CardHeader>

      {(expanded || editing) && (
        <CardContent>
          <div className="flex flex-wrap items-center gap-1.5">
            {rules.length === 0 && (
              <span className="text-xs text-muted-foreground">
                No rules — this category has to be assigned manually.
              </span>
            )}
            {rules.map((rule) => (
              <Badge
                key={rule.id}
                variant="secondary"
                className="h-auto py-0.5"
              >
                <span className="text-muted-foreground">
                  {rule.key_kind === "exe" ? "app" : "site"}{" "}
                  {MATCH_KIND_LABEL[rule.match_kind]}
                </span>
                <span className="font-mono">{rule.pattern}</span>
                <button
                  type="button"
                  onClick={() => onDeleteRule(rule)}
                  aria-label={`Remove rule ${rule.pattern}`}
                  className="text-muted-foreground hover:text-destructive"
                >
                  <Trash2Icon className="size-3" />
                </button>
              </Badge>
            ))}
          </div>

          <form
            onSubmit={handleAddRule}
            className="mt-3 flex flex-wrap items-center gap-1.5"
          >
            <Select
              value={keyKind}
              onValueChange={(value) => setKeyKind(value as KeyKind)}
            >
              <SelectTrigger size="sm" className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="exe">App (exe)</SelectItem>
                <SelectItem value="domain">Site (domain)</SelectItem>
              </SelectContent>
            </Select>
            <Select
              value={matchKind}
              onValueChange={(value) => setMatchKind(value as MatchKind)}
            >
              <SelectTrigger size="sm" className="w-32" aria-label="Match kind">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="exact">is</SelectItem>
                <SelectItem value="substring">contains</SelectItem>
                <SelectItem value="regex">matches (regex)</SelectItem>
              </SelectContent>
            </Select>
            <Input
              type="text"
              value={pattern}
              onChange={(e) => setPattern(e.target.value)}
              placeholder={matchKindPlaceholder(keyKind, matchKind)}
              className="h-7 min-w-32 flex-1 font-mono"
            />
            <Button
              type="submit"
              size="xs"
              variant="outline"
              disabled={!pattern.trim()}
            >
              Add rule
            </Button>
          </form>
        </CardContent>
      )}
    </Card>
  );
}

// Wraps a `CategoryCard` as a `dnd-kit` sortable item, passing the drag
// handle's listeners down so the grip icon in the card header is the only
// draggable surface — the rest of the card stays clickable/editable as
// normal.
function SortableCategoryCard(
  props: React.ComponentProps<typeof CategoryCard>,
) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: props.category.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  };

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={isDragging ? "opacity-60" : ""}
    >
      <CategoryCard
        {...props}
        dragHandleProps={{ ...attributes, ...listeners }}
      />
    </div>
  );
}
