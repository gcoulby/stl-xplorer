import { ArrowLeft, ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Tip } from "@/components/ui/overlay";
import { cn } from "@/lib/utils";
import { useUi } from "@/store/ui";
import type { Crumb } from "@/types";

export function Breadcrumbs({
  crumbs,
  onUp,
  className,
  compact,
}: {
  crumbs: Crumb[];
  onUp?: () => void;
  className?: string;
  compact?: boolean;
}) {
  const navigate = useUi((s) => s.navigate);
  // Long chains collapse in the middle so the start and the end stay readable.
  const shown: Array<Crumb | null> =
    crumbs.length > 5 ? [crumbs[0]!, null, ...crumbs.slice(-3)] : crumbs;
  return (
    <div className={cn("flex min-w-0 items-center gap-0.5 text-sm", className)}>
      {onUp && (
        <Tip label="Up one level (Backspace)">
          <Button
            variant="ghost"
            size="icon-xs"
            className="mr-1 size-6"
            disabled={crumbs.length < 2}
            onClick={onUp}
          >
            <ArrowLeft />
          </Button>
        </Tip>
      )}
      {shown.map((c, i) => {
        const last = i === shown.length - 1;
        return (
          <span
            key={c ? `${c.containerId}:${c.subpath}` : "gap"}
            className="flex min-w-0 items-center"
          >
            {i > 0 && (
              <ChevronRight className="mx-0.5 size-3.5 shrink-0 text-muted-foreground/60" />
            )}
            {c ? (
              <button
                type="button"
                title={c.name}
                onClick={() =>
                  navigate({ kind: "container", containerId: c.containerId, subpath: c.subpath })
                }
                className={cn(
                  "truncate rounded px-1 py-0.5 hover:bg-accent",
                  last ? "font-medium text-foreground" : "text-muted-foreground",
                  compact ? "max-w-[12rem] text-xs" : "max-w-[16rem]",
                )}
              >
                {c.name}
              </button>
            ) : (
              <span className="px-1 text-muted-foreground">…</span>
            )}
          </span>
        );
      })}
    </div>
  );
}
