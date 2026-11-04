import { ArrowUp, ChevronRight, Pencil } from "lucide-react";
import { useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { Tip } from "@/components/ui/overlay";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useUi } from "@/store/ui";
import type { ContainerInfo, Crumb } from "@/types";

const IS_WINDOWS = navigator.userAgent.includes("Windows");
const SEP = IS_WINDOWS ? "\\" : "/";

/** The real path of the place being browsed, with any folders inside an archive appended. */
export function displayPath(container: ContainerInfo, subpath: string): string {
  const base = container.path.replace("::", SEP);
  return subpath ? `${base}${SEP}${subpath.split("/").join(SEP)}` : base;
}

/**
 * Breadcrumbs that turn into a text box. Click a crumb to go there, click the empty space
 * (or the pencil, or press Ctrl+L) to type or paste a path.
 */
export function AddressBar({
  crumbs,
  container,
  subpath,
  title,
  onUp,
}: {
  crumbs: Crumb[] | null;
  container: ContainerInfo | null;
  subpath: string;
  title: string;
  onUp: () => void;
}) {
  const navigate = useUi((s) => s.navigate);
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const current = container ? displayPath(container, subpath) : "";

  const startEdit = () => {
    setText(current);
    setEditing(true);
    requestAnimationFrame(() => inputRef.current?.select());
  };

  const go = async () => {
    const t = text.trim();
    if (!t || t === current) return setEditing(false);
    setBusy(true);
    try {
      const r = await api.resolveLocation(t);
      navigate({ kind: "container", containerId: r.containerId, subpath: r.subpath });
      setEditing(false);
    } catch (e) {
      toast.error("Cannot go there", errorMessage(e));
      inputRef.current?.select();
    } finally {
      setBusy(false);
    }
  };

  // Long chains collapse in the middle so the start and the end stay readable.
  const shown: Array<Crumb | null> | null = crumbs
    ? crumbs.length > 6
      ? [crumbs[0]!, null, ...crumbs.slice(-4)]
      : crumbs
    : null;

  return (
    <div className="flex min-w-0 flex-1 items-center gap-1">
      <Tip label="Up one level (Backspace)">
        <Button
          variant="ghost"
          size="icon-xs"
          className="size-7 shrink-0"
          disabled={!crumbs || crumbs.length < 2}
          onClick={onUp}
        >
          <ArrowUp />
        </Button>
      </Tip>
      {editing ? (
        <input
          ref={inputRef}
          data-address-input
          autoFocus
          value={text}
          disabled={busy}
          spellCheck={false}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => !busy && setEditing(false)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void go();
            } else if (e.key === "Escape") {
              e.preventDefault();
              setEditing(false);
            }
          }}
          placeholder="Type or paste a folder or archive path"
          className="h-7 min-w-0 flex-1 rounded-md border border-ring bg-background px-2 font-mono text-xs ring-[3px] ring-ring/30 outline-none"
        />
      ) : (
        <div
          data-address-bar
          onClick={(e) => {
            if (e.target === e.currentTarget) startEdit();
          }}
          title={current || undefined}
          className="group flex h-7 min-w-0 flex-1 cursor-text items-center gap-0.5 overflow-hidden rounded-md border border-input bg-background/60 px-1 text-sm dark:bg-input/30"
        >
          {shown ? (
            shown.map((c, i) => (
              <span
                key={c ? `${c.containerId}:${c.subpath}` : "gap"}
                className="flex min-w-0 shrink items-center last:shrink-0"
              >
                {i > 0 && <ChevronRight className="size-3.5 shrink-0 text-muted-foreground/60" />}
                {c ? (
                  <button
                    type="button"
                    title={c.name}
                    onClick={() =>
                      navigate({
                        kind: "container",
                        containerId: c.containerId,
                        subpath: c.subpath,
                      })
                    }
                    className={cn(
                      "max-w-[14rem] truncate rounded px-1 py-0.5 hover:bg-accent",
                      i === shown.length - 1
                        ? "font-medium text-foreground"
                        : "text-muted-foreground",
                    )}
                  >
                    {c.name}
                  </button>
                ) : (
                  <span className="px-1 text-muted-foreground">…</span>
                )}
              </span>
            ))
          ) : (
            <span className="truncate px-1 font-medium">{title}</span>
          )}
          <span className="min-w-4 flex-1 self-stretch" onClick={startEdit} />
          <button
            type="button"
            onClick={startEdit}
            className="shrink-0 rounded p-1 text-muted-foreground opacity-0 group-hover:opacity-100 hover:text-foreground"
            aria-label="Edit path"
          >
            <Pencil className="size-3" />
          </button>
        </div>
      )}
    </div>
  );
}
