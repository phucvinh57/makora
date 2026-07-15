import { Trash2Icon } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  createCategory,
  createCategoryRule,
  deleteCategory,
  deleteCategoryRule,
  listCategories,
  listCategoryRules,
  updateCategory,
} from "./api";
import type { Category, CategoryRule, KeyKind } from "./types";

const DEFAULT_COLOR = "#6366f1";

export default function CategoriesPage() {
  const [categories, setCategories] = useState<Category[]>([]);
  const [rules, setRules] = useState<CategoryRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [newName, setNewName] = useState("");
  const [newColor, setNewColor] = useState(DEFAULT_COLOR);

  function refresh() {
    setLoading(true);
    Promise.all([listCategories(), listCategoryRules()])
      .then(([cats, rls]) => {
        setCategories(cats);
        setRules(rls);
        setError(null);
      })
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }

  useEffect(refresh, []);

  async function handleCreateCategory(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    try {
      await createCategory(newName.trim(), newColor);
      setNewName("");
      setNewColor(DEFAULT_COLOR);
      refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleRenameCategory(category: Category, name: string, color: string) {
    try {
      await updateCategory(category.id, name, color);
      refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleDeleteCategory(category: Category) {
    try {
      await deleteCategory(category.id);
      refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleAddRule(categoryId: number, keyKind: KeyKind, pattern: string) {
    if (!pattern.trim()) return;
    try {
      await createCategoryRule(keyKind, pattern.trim().toLowerCase(), categoryId);
      refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleDeleteRule(rule: CategoryRule) {
    try {
      await deleteCategoryRule(rule.id);
      refresh();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div className="flex h-full flex-col box-border">
      <header className="mb-4">
        <p className="m-0 text-sm text-muted-foreground">
          Categories & rules
        </p>
      </header>

      {error && (
        <div className="mb-3 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      )}

      <form
        onSubmit={handleCreateCategory}
        className="mb-4 flex items-center gap-2"
      >
        <input
          type="color"
          value={newColor}
          onChange={(e) => setNewColor(e.target.value)}
          className="h-7 w-9 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
          aria-label="New category color"
        />
        <input
          type="text"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          placeholder="New category name"
          className="h-7 rounded-md border bg-transparent px-2 text-sm"
        />
        <Button type="submit" size="sm" disabled={!newName.trim()}>
          Add category
        </Button>
      </form>

      <div className="flex-1 overflow-y-auto">
        {!loading && categories.length === 0 && (
          <p className="text-sm text-muted-foreground">
            No categories yet.
          </p>
        )}
        <div className="flex flex-col gap-3">
          {categories.map((category) => (
            <CategoryCard
              key={category.id}
              category={category}
              rules={rules.filter((r) => r.category_id === category.id)}
              onRename={handleRenameCategory}
              onDelete={handleDeleteCategory}
              onAddRule={handleAddRule}
              onDeleteRule={handleDeleteRule}
            />
          ))}
        </div>
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
}: {
  category: Category;
  rules: CategoryRule[];
  onRename: (category: Category, name: string, color: string) => void;
  onDelete: (category: Category) => void;
  onAddRule: (categoryId: number, keyKind: KeyKind, pattern: string) => void;
  onDeleteRule: (rule: CategoryRule) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(category.name);
  const [color, setColor] = useState(category.color);
  const [pattern, setPattern] = useState("");
  const [keyKind, setKeyKind] = useState<KeyKind>("exe");

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

  function handleAddRule(e: React.FormEvent) {
    e.preventDefault();
    onAddRule(category.id, keyKind, pattern);
    setPattern("");
  }

  return (
    <div className="rounded-md border p-3">
      <div className="flex items-center gap-2">
        {editing ? (
          <>
            <input
              type="color"
              value={color}
              onChange={(e) => setColor(e.target.value)}
              className="h-7 w-9 shrink-0 cursor-pointer rounded-md border bg-transparent p-0.5"
              aria-label="Category color"
            />
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="h-7 flex-1 rounded-md border bg-transparent px-2 text-sm"
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
          </>
        ) : (
          <>
            <span
              className="size-4 shrink-0 rounded-full"
              style={{ backgroundColor: category.color }}
            />
            <span className="flex-1 text-sm font-medium">
              {category.name}
            </span>
            <Button variant="outline" size="sm" onClick={startEditing}>
              Edit
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => onDelete(category)}
            >
              Delete
            </Button>
          </>
        )}
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-1.5">
        {rules.length === 0 && (
          <span className="text-xs text-muted-foreground">
            No rules — this category has to be assigned manually.
          </span>
        )}
        {rules.map((rule) => (
          <span
            key={rule.id}
            className="inline-flex items-center gap-1 rounded-md border bg-muted px-1.5 py-0.5 text-xs"
          >
            <span className="text-muted-foreground">
              {rule.key_kind === "exe" ? "app" : "site"}
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
          </span>
        ))}
      </div>

      <form
        onSubmit={handleAddRule}
        className="mt-2 flex items-center gap-1.5"
      >
        <select
          value={keyKind}
          onChange={(e) => setKeyKind(e.target.value as KeyKind)}
          className="h-6 rounded-md border bg-transparent px-1 text-xs"
        >
          <option value="exe">App (exe)</option>
          <option value="domain">Site (domain)</option>
        </select>
        <input
          type="text"
          value={pattern}
          onChange={(e) => setPattern(e.target.value)}
          placeholder={keyKind === "exe" ? "code.exe" : "github.com"}
          className="h-6 rounded-md border bg-transparent px-1.5 text-xs"
        />
        <Button type="submit" size="xs" variant="outline" disabled={!pattern.trim()}>
          Add rule
        </Button>
      </form>
    </div>
  );
}
