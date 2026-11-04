import { FileWarning, FolderSearch, Lock, RefreshCw, TriangleAlert } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/misc";
import { useArchiveProblem } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import type { ContainerInfo } from "@/types";

export function hasProblem(c: ContainerInfo): boolean {
  return c.kind !== "folder" && (c.status !== "ok" || c.encrypted);
}

interface Explained {
  icon: React.ReactNode;
  title: string;
  body: string;
  fixes: string[];
}

function explain(c: ContainerInfo): Explained {
  const detail = c.statusDetail ?? "";
  switch (c.status) {
    case "missing_parts":
      return {
        icon: <FileWarning className="size-9 text-warning" />,
        title: detail || "Part of this archive is missing",
        body: "This archive is split into several files and some of them are not in the folder. Nothing inside it can be read until every part is there.",
        fixes: [
          "Download the missing parts again and put them in the same folder as the others.",
          "If a download is still running, wait for it to finish. The archive is picked up again on its own.",
          "Once the parts are in place, rescan if it does not update by itself.",
        ],
      };
    case "unsupported":
      return {
        icon: <TriangleAlert className="size-9 text-warning" />,
        title: "This archive cannot be opened",
        body: detail || "The archive format is not one 7-Zip can read.",
        fixes: [
          "Check Settings → Archives to see which 7-Zip is in use.",
          "Try opening it in 7-Zip directly to see whether the file itself is the problem.",
        ],
      };
    case "corrupt":
      return {
        icon: <TriangleAlert className="size-9 text-destructive" />,
        title: "This archive looks damaged",
        body: detail || "7-Zip could not read the archive's contents.",
        fixes: [
          "The download may have been cut short. Download it again.",
          "For split archives, check that every part has the right size.",
        ],
      };
    default:
      return {
        icon: <Lock className="size-9 text-muted-foreground" />,
        title: "This archive is password protected",
        body: "Previews and slicer launches are skipped for encrypted archives in this version.",
        fixes: ["Extract it with the password in 7-Zip, then the files show up as normal."],
      };
  }
}

function useActions(c: ContainerInfo) {
  return {
    reveal: () =>
      void api
        .revealInFolder({ containerId: c.id })
        .catch((e: unknown) => toast.error("Could not show it", errorMessage(e))),
    rescan: () =>
      void api
        .rescanLibrary(c.libraryId)
        .then(() => toast.info("Rescanning", "The archive is checked again in the background."))
        .catch((e: unknown) => toast.error("Rescan failed", errorMessage(e))),
  };
}

/** Shown in place of an archive's contents when it cannot be read. */
export function ArchiveProblemPanel({ container }: { container: ContainerInfo }) {
  const info = useArchiveProblem(container.id).data;
  const e = explain(container);
  const { reveal, rescan } = useActions(container);
  return (
    <div className="flex h-full items-start justify-center overflow-y-auto p-8">
      <div className="flex w-full max-w-lg flex-col gap-4 rounded-xl border bg-card p-6 shadow-sm">
        <div className="flex items-start gap-3">
          {e.icon}
          <div className="flex min-w-0 flex-col gap-1">
            <h2 className="text-base font-semibold">{e.title}</h2>
            <p className="text-sm text-muted-foreground">{e.body}</p>
          </div>
        </div>

        <div className="flex flex-col gap-1.5">
          <span className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
            {container.partCount > 1 || container.status === "missing_parts"
              ? "Parts found"
              : "File"}
          </span>
          {info ? (
            <ul className="flex flex-col gap-0.5 rounded-md border bg-muted/30 p-2 font-mono text-xs">
              {info.parts.map((p) => (
                <li key={p} className="truncate selectable" title={p}>
                  {p}
                </li>
              ))}
            </ul>
          ) : (
            <Skeleton className="h-12" />
          )}
          {info?.folder && (
            <p className="truncate text-xs text-muted-foreground selectable" title={info.folder}>
              In {info.folder}
            </p>
          )}
        </div>

        <div className="flex flex-col gap-1.5">
          <span className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
            How to fix it
          </span>
          <ul className="flex list-disc flex-col gap-1 pl-5 text-sm">
            {e.fixes.map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        </div>

        <div className="flex flex-wrap gap-2">
          <Button size="sm" variant="outline" onClick={reveal}>
            <FolderSearch /> Show in folder
          </Button>
          <Button size="sm" variant="outline" onClick={rescan}>
            <RefreshCw /> Rescan library
          </Button>
        </div>
      </div>
    </div>
  );
}

/** A one-line note above an archive's contents when some of it cannot be used. */
export function ArchiveProblemBanner({ container }: { container: ContainerInfo }) {
  const e = explain(container);
  const { reveal } = useActions(container);
  return (
    <div className="flex items-center gap-2 border-b bg-warning/10 px-3 py-2 text-xs">
      <TriangleAlert className="size-3.5 shrink-0 text-warning" />
      <span className="min-w-0 flex-1 truncate">
        <span className="font-medium">{e.title}.</span>{" "}
        <span className="text-muted-foreground">{e.body}</span>
      </span>
      <Button size="xs" variant="ghost" onClick={reveal}>
        <FolderSearch /> Show in folder
      </Button>
    </div>
  );
}
