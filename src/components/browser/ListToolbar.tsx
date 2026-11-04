import { ArrowDown, ArrowUp, ArrowUpDown, Columns3, Filter, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/menu";
import { Tip } from "@/components/ui/overlay";
import { activeFilterCount, TYPE_FILTERS } from "@/features/filtering";
import { useTags } from "@/features/queries";
import { cn } from "@/lib/utils";
import { useUi, type Columns, type SortKey } from "@/store/ui";

export const SORT_LABELS: Record<SortKey, string> = {
  name: "Name",
  size: "Size",
  modified: "Last modified",
  type: "Type",
};

export function FilterMenu() {
  const filters = useUi((s) => s.filters);
  const setFilters = useUi((s) => s.setFilters);
  const tags = useTags().data ?? [];
  const n = activeFilterCount(filters);
  const toggle = <T,>(list: T[], v: T) =>
    list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
  return (
    <DropdownMenu>
      <Tip label="Filter files by type or tag">
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="xs"
            className={cn("h-7 gap-1 px-2", n > 0 && "bg-primary/15 text-primary")}
          >
            <Filter className="size-3.5" />
            Filter
            {n > 0 && (
              <span className="rounded bg-primary px-1 text-[10px] text-primary-foreground tabular-nums">
                {n}
              </span>
            )}
          </Button>
        </DropdownMenuTrigger>
      </Tip>
      <DropdownMenuContent align="end" className="max-h-96 w-56 overflow-y-auto">
        <DropdownMenuLabel>File type</DropdownMenuLabel>
        {TYPE_FILTERS.map((t) => (
          <DropdownMenuCheckboxItem
            key={t.id}
            checked={filters.types.includes(t.id)}
            onSelect={(e) => e.preventDefault()}
            onCheckedChange={() => setFilters({ types: toggle(filters.types, t.id) })}
          >
            {t.label}
          </DropdownMenuCheckboxItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuLabel>Tagged with any of</DropdownMenuLabel>
        {tags.length === 0 && (
          <p className="px-2 pb-1.5 text-xs text-muted-foreground">No tags yet.</p>
        )}
        {tags.map((t) => (
          <DropdownMenuCheckboxItem
            key={t.id}
            checked={filters.tagIds.includes(t.id)}
            onSelect={(e) => e.preventDefault()}
            onCheckedChange={() => setFilters({ tagIds: toggle(filters.tagIds, t.id) })}
          >
            <span className="size-2.5 rounded-full" style={{ backgroundColor: t.colour }} />
            {t.name}
          </DropdownMenuCheckboxItem>
        ))}
        {n > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => setFilters({ types: [], tagIds: [] })}>
              <X /> Clear filters
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export function SortMenu() {
  const sort = useUi((s) => s.sort);
  const setSort = useUi((s) => s.setSort);
  return (
    <DropdownMenu>
      <Tip label="Sort">
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="xs" className="h-7 gap-1 px-2">
            <ArrowUpDown className="size-3.5" />
            {SORT_LABELS[sort.key]}
            {sort.desc ? <ArrowDown className="size-3" /> : <ArrowUp className="size-3" />}
          </Button>
        </DropdownMenuTrigger>
      </Tip>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuLabel>Sort by</DropdownMenuLabel>
        {(Object.keys(SORT_LABELS) as SortKey[]).map((k) => (
          <DropdownMenuCheckboxItem
            key={k}
            checked={sort.key === k}
            onCheckedChange={() =>
              // Sizes and dates read best largest and newest first.
              setSort({ key: k, desc: k === "size" || k === "modified" })
            }
          >
            {SORT_LABELS[k]}
          </DropdownMenuCheckboxItem>
        ))}
        <DropdownMenuSeparator />
        <DropdownMenuCheckboxItem
          checked={!sort.desc}
          onCheckedChange={() => setSort({ ...sort, desc: false })}
        >
          Ascending
        </DropdownMenuCheckboxItem>
        <DropdownMenuCheckboxItem
          checked={sort.desc}
          onCheckedChange={() => setSort({ ...sort, desc: true })}
        >
          Descending
        </DropdownMenuCheckboxItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export const COLUMN_LABELS: Record<keyof Columns, string> = {
  type: "Type",
  size: "Size",
  modified: "Last modified",
  location: "Location",
  tags: "Tags",
};

export function ColumnsMenu() {
  const columns = useUi((s) => s.columns);
  const setColumns = useUi((s) => s.setColumns);
  return (
    <DropdownMenu>
      <Tip label="Choose columns">
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="icon-xs" className="size-6" aria-label="Choose columns">
            <Columns3 className="size-3.5" />
          </Button>
        </DropdownMenuTrigger>
      </Tip>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuLabel>Columns</DropdownMenuLabel>
        {(Object.keys(COLUMN_LABELS) as Array<keyof Columns>).map((k) => (
          <DropdownMenuCheckboxItem
            key={k}
            checked={columns[k]}
            onSelect={(e) => e.preventDefault()}
            onCheckedChange={(v) => setColumns({ [k]: v === true })}
          >
            {COLUMN_LABELS[k]}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
