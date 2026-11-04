import type { ListItem } from "@/features/listing";
import { fileKind } from "@/lib/utils";
import type { Filters, Sort, TypeFilter } from "@/store/ui";

export const TYPE_FILTERS: Array<{ id: TypeFilter; label: string }> = [
  { id: "stl", label: "STL" },
  { id: "3mf", label: "3MF" },
  { id: "obj", label: "OBJ" },
  { id: "image", label: "Images" },
  { id: "pdf", label: "PDFs" },
];

function typeOf(ext: string): TypeFilter | null {
  const k = fileKind(ext);
  if (k === "image" || k === "pdf") return k;
  return ext === "stl" || ext === "3mf" || ext === "obj" ? ext : null;
}

export function activeFilterCount(f: Filters): number {
  return f.types.length + f.tagIds.length;
}

/** Label for the Type column. */
export function typeLabel(it: ListItem): string {
  switch (it.type) {
    case "model":
      return it.model.ext.toUpperCase();
    case "container":
      return it.container.kind === "folder" ? "Folder" : it.container.kind.toUpperCase();
    case "dir":
      return "Folder";
    case "tag":
      return "Tag";
  }
}

function name(it: ListItem): string {
  if (it.type === "model") return it.model.name;
  if (it.type === "container") return it.container.name;
  if (it.type === "tag") return it.tag.name;
  return it.dir.name;
}

function size(it: ListItem): number {
  if (it.type === "model") return it.model.size;
  if (it.type === "container") return it.container.size;
  return 0;
}

function modified(it: ListItem): number {
  if (it.type === "model") return it.model.mtime;
  if (it.type === "container") return it.container.mtime;
  return 0;
}

const collator = new Intl.Collator("en-GB", { numeric: true, sensitivity: "base" });

/**
 * Narrow and order a listing. Filters apply to files; folders and archives stay so you can
 * keep moving around, and always sort ahead of files.
 */
export function applyFilters(items: ListItem[], filters: Filters, sort: Sort): ListItem[] {
  const types = new Set(filters.types);
  const tags = new Set(filters.tagIds);
  const kept = items.filter((it) => {
    if (it.type !== "model") return true;
    if (types.size > 0) {
      const t = typeOf(it.model.ext);
      if (!t || !types.has(t)) return false;
    }
    if (tags.size > 0 && !it.model.tagIds.some((id) => tags.has(id))) return false;
    return true;
  });

  const dir = sort.desc ? -1 : 1;
  const cmp = (a: ListItem, b: ListItem): number => {
    let r = 0;
    if (sort.key === "size") r = size(a) - size(b);
    else if (sort.key === "modified") r = modified(a) - modified(b);
    else if (sort.key === "type") r = collator.compare(typeLabel(a), typeLabel(b));
    if (r === 0) r = collator.compare(name(a), name(b)) * (sort.key === "name" ? 1 : dir);
    return r * dir;
  };
  // Folders first, then archives, then files, as in a file manager.
  const group = (it: ListItem) =>
    it.type === "model" ? 2 : it.type === "container" && it.container.kind !== "folder" ? 1 : 0;
  return kept
    .map((it, i) => ({ it, i }))
    .sort((a, b) => group(a.it) - group(b.it) || cmp(a.it, b.it) || a.i - b.i)
    .map((x) => x.it);
}
