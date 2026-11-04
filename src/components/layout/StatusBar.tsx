import { CircleCheck, LoaderCircle, TriangleAlert } from "lucide-react";

import { useLibraries, useSevenZip } from "@/features/queries";
import { plural } from "@/lib/utils";
import { useUi } from "@/store/ui";

export function StatusBar() {
  const scans = useUi((s) => s.scans);
  const libs = useLibraries().data ?? [];
  const sz = useSevenZip().data;
  const openSettings = useUi((s) => s.openSettings);
  const navigate = useUi((s) => s.navigate);
  const active = Object.values(scans);
  const pack = useUi((s) => s.packJob);
  const total = libs.filter((l) => l.enabled).reduce((n, l) => n + l.modelCount, 0);
  const issues = libs.filter((l) => l.enabled).reduce((n, l) => n + l.issueCount, 0);

  return (
    <div className="flex h-7 shrink-0 items-center gap-4 border-t bg-pane px-3 text-[11px] text-muted-foreground">
      {active.length > 0 ? (
        <span className="flex min-w-0 items-center gap-1.5">
          <LoaderCircle className="size-3 shrink-0 animate-spin text-primary" />
          <span className="truncate">
            {active
              .map((p) =>
                p.phase === "discovering"
                  ? `Scanning ${p.libraryName}${p.done ? ` (${p.done.toLocaleString("en-GB")} entries)` : ""}`
                  : p.phase === "indexing"
                    ? `Indexing ${p.libraryName} ${p.done.toLocaleString("en-GB")} / ${p.total.toLocaleString("en-GB")} folders`
                    : `Finishing ${p.libraryName}`,
              )
              .join(" · ")}
          </span>
        </span>
      ) : (
        <span className="flex items-center gap-1.5">
          <CircleCheck className="size-3 text-success" />
          Up to date
        </span>
      )}
      {pack && (
        <span className="flex min-w-0 items-center gap-1.5">
          <LoaderCircle className="size-3 shrink-0 animate-spin text-amber-400" />
          <span className="truncate">
            Zipping {pack.label}{" "}
            {pack.total > 0 ? `${Math.round((pack.done / pack.total) * 100)}%` : ""}
          </span>
        </span>
      )}
      <span className="tabular-nums">{plural(total, "model")}</span>
      {issues > 0 && (
        <button
          type="button"
          onClick={() => navigate({ kind: "problems" })}
          className="flex items-center gap-1 text-warning hover:underline"
        >
          <TriangleAlert className="size-3" />
          {plural(issues, "archive")} {issues === 1 ? "needs" : "need"} attention
        </button>
      )}
      <div className="flex-1" />
      {sz && !sz.path && (
        <button
          type="button"
          onClick={() => openSettings("archives")}
          className="flex items-center gap-1 text-warning hover:underline"
        >
          <TriangleAlert className="size-3" />
          7-Zip not found: 7z and rar archives are unavailable
        </button>
      )}
      <span className="hidden sm:inline">
        <kbd className="font-sans">Ctrl K</kbd> commands · <kbd className="font-sans">/</kbd> search
      </span>
    </div>
  );
}
