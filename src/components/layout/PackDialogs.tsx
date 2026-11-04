import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CircleCheck, FileArchive, LoaderCircle, Trash2, TriangleAlert } from "lucide-react";
import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Checkbox, Label, Progress, Skeleton } from "@/components/ui/misc";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/overlay";
import { usePacks } from "@/features/packs";
import { invalidateIndex } from "@/features/queries";
import { toast } from "@/features/toast";
import { api, errorMessage } from "@/lib/api";
import { formatBytes, plural } from "@/lib/utils";
import { useUi } from "@/store/ui";
import type { UnpackedCopy } from "@/types";

export function PackDialogs() {
  return (
    <>
      <ZipDialog />
      <UnpackedDialog />
    </>
  );
}

function ZipDialog() {
  const targets = usePacks((s) => s.zipTargets);
  const close = usePacks((s) => s.closeZip);
  const open = targets.length > 0;
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-md">{open && <ZipForm />}</DialogContent>
    </Dialog>
  );
}

function ZipForm() {
  const qc = useQueryClient();
  const targets = usePacks((s) => s.zipTargets);
  const close = usePacks((s) => s.closeZip);
  const job = useUi((s) => s.packJob);
  const [deleteAfter, setDeleteAfter] = useState(true);
  const zip = useMutation({
    mutationFn: () =>
      api.zipFolders(
        targets.map((t) => t.id),
        deleteAfter,
      ),
    onSuccess: (r) => {
      invalidateIndex(qc);
      const made = r.zipped.length + r.already.length;
      if (made > 0) {
        toast.success(
          r.zipped.length > 0
            ? `Zipped ${plural(r.zipped.length, "folder")}`
            : "Matching zips were already there",
          [
            r.already.length > 0 ? `${r.already.length} already had a matching zip` : "",
            r.deleted.length > 0 ? `${r.deleted.length} moved to the Recycle Bin` : "",
          ]
            .filter(Boolean)
            .join(". "),
        );
      }
      for (const e of r.errors) toast.error("Not zipped", e);
      close();
    },
    onError: (e) => toast.error("Could not zip", errorMessage(e)),
  });
  const single = targets.length === 1 ? targets[0] : undefined;

  return (
    <>
      <div className="flex items-start gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-full bg-amber-400/15">
          <FileArchive className="size-4 text-amber-400" />
        </div>
        <div className="flex min-w-0 flex-col gap-1">
          <DialogTitle className="text-base leading-snug break-words">
            {single ? `Zip “${single.name}”?` : `Zip ${plural(targets.length, "folder")}?`}
          </DialogTitle>
          <DialogDescription>
            Each folder becomes a zip of the same name next to it. If a zip with that name is
            already there and holds every file, it is used as it is. If it differs, that folder is
            left alone.
          </DialogDescription>
        </div>
      </div>
      {!single && (
        <ul className="flex max-h-40 flex-col gap-0.5 overflow-y-auto rounded-md border bg-muted/30 p-2 text-xs">
          {targets.map((t) => (
            <li key={t.id} className="truncate">
              {t.name}
            </li>
          ))}
        </ul>
      )}
      <Label className="flex items-start gap-2 text-sm font-normal">
        <Checkbox
          checked={deleteAfter}
          onCheckedChange={(v) => setDeleteAfter(v === true)}
          className="mt-0.5"
          disabled={zip.isPending}
        />
        <span>
          Move each folder to the Recycle Bin once its zip has been checked
          <span className="block text-xs text-muted-foreground">
            The zip is read back and compared with the folder first. Nothing is deleted if they
            differ.
          </span>
        </span>
      </Label>
      {zip.isPending && (
        <div className="flex flex-col gap-1.5">
          <span className="truncate text-xs text-muted-foreground">
            {job?.label ? `Zipping ${job.label}…` : "Working…"}
          </span>
          <Progress value={job && job.total > 0 ? (job.done / job.total) * 100 : 0} />
        </div>
      )}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={close} disabled={zip.isPending}>
          Cancel
        </Button>
        <Button onClick={() => zip.mutate()} disabled={zip.isPending}>
          {zip.isPending ? <LoaderCircle className="animate-spin" /> : <FileArchive />}
          {deleteAfter ? "Zip and remove folders" : "Zip"}
        </Button>
      </div>
    </>
  );
}

function UnpackedDialog() {
  const scope = usePacks((s) => s.unpackedScope);
  const close = usePacks((s) => s.closeUnpacked);
  return (
    <Dialog open={scope !== undefined} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-2xl">
        {scope !== undefined && <UnpackedForm key={String(scope.id)} scope={scope} />}
      </DialogContent>
    </Dialog>
  );
}

function UnpackedForm({ scope }: { scope: { id: number | null; name: string } }) {
  const qc = useQueryClient();
  const close = usePacks((s) => s.closeUnpacked);
  const found = useQuery({
    queryKey: ["unpacked", scope.id],
    queryFn: () => api.findUnpacked(scope.id),
    staleTime: 0,
    gcTime: 0,
  });
  // Unticked by the user; safe copies start ticked.
  const [unticked, setUnticked] = useState<Set<number>>(new Set());
  const list = found.data ?? [];
  const safe = list.filter((c) => c.matched === c.files);
  const chosen = safe.filter((c) => !unticked.has(c.folder.id));
  const bytes = chosen.reduce((n, c) => n + c.bytes, 0);

  const del = useMutation({
    mutationFn: () => api.deleteUnpacked(chosen.map((c) => c.folder.id)),
    onSuccess: (r) => {
      invalidateIndex(qc);
      if (r.deleted.length > 0)
        toast.success(
          `Moved ${plural(r.deleted.length, "unpacked folder")} to the Recycle Bin`,
          `${formatBytes(bytes)} freed once the bin is emptied.`,
        );
      for (const e of r.errors) toast.error("Not deleted", e);
      close();
    },
    onError: (e) => toast.error("Could not delete", errorMessage(e)),
  });

  const toggle = (id: number) => {
    const next = new Set(unticked);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setUnticked(next);
  };

  return (
    <>
      <div className="flex flex-col gap-1">
        <DialogTitle className="text-base">Unpacked copies of archives</DialogTitle>
        <DialogDescription>
          Folders in {scope.name} that sit next to an archive of the same name and whose files are
          all inside it. They are safe to delete because the archive still has everything, and STL
          Xplorer can preview straight from the archive.
        </DialogDescription>
      </div>
      <div className="max-h-[50vh] min-h-24 overflow-y-auto rounded-md border">
        {found.isLoading ? (
          <div className="flex flex-col gap-2 p-3">
            <span className="flex items-center gap-2 text-xs text-muted-foreground">
              <LoaderCircle className="size-3.5 animate-spin" /> Comparing folders with their
              archives…
            </span>
            <Skeleton className="h-6" />
            <Skeleton className="h-6 w-2/3" />
          </div>
        ) : found.error ? (
          <p className="p-3 text-sm text-destructive">{errorMessage(found.error)}</p>
        ) : list.length === 0 ? (
          <p className="flex items-center gap-2 p-4 text-sm text-muted-foreground">
            <CircleCheck className="size-4 text-success" /> No unpacked copies found.
          </p>
        ) : (
          <ul className="divide-y">
            {list.map((c) => (
              <UnpackedRow
                key={c.folder.id}
                c={c}
                checked={c.matched === c.files && !unticked.has(c.folder.id)}
                onToggle={() => toggle(c.folder.id)}
              />
            ))}
          </ul>
        )}
      </div>
      <div className="flex items-center gap-2">
        <span className="flex-1 text-xs text-muted-foreground">
          {chosen.length > 0 &&
            `${plural(chosen.length, "folder")} · ${formatBytes(bytes)} to the Recycle Bin`}
        </span>
        <Button variant="ghost" onClick={close} disabled={del.isPending}>
          Cancel
        </Button>
        <Button
          variant="destructive"
          disabled={chosen.length === 0 || del.isPending}
          onClick={() => del.mutate()}
        >
          {del.isPending ? <LoaderCircle className="animate-spin" /> : <Trash2 />}
          Delete {chosen.length > 0 ? plural(chosen.length, "folder") : ""}
        </Button>
      </div>
    </>
  );
}

function UnpackedRow({
  c,
  checked,
  onToggle,
}: {
  c: UnpackedCopy;
  checked: boolean;
  onToggle: () => void;
}) {
  const safe = c.matched === c.files;
  return (
    <li className="flex items-start gap-3 px-3 py-2">
      <Checkbox checked={checked} disabled={!safe} onCheckedChange={onToggle} className="mt-0.5" />
      <div className="flex min-w-0 flex-1 flex-col">
        <span className="truncate text-sm font-medium" title={c.folder.path}>
          {c.folder.name}
        </span>
        <span className="truncate text-xs text-muted-foreground">
          {safe ? (
            <>
              All {plural(c.files, "file")} are in {c.archive.name}
            </>
          ) : (
            <span className="text-warning">
              <TriangleAlert className="mr-1 inline size-3" />
              {c.files - c.matched} of {plural(c.files, "file")} are not in {c.archive.name}
              {c.mismatched.length > 0 && ` (${c.mismatched.slice(0, 2).join(", ")}…)`}. Kept.
            </span>
          )}
        </span>
      </div>
      <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
        {formatBytes(c.bytes)}
      </span>
    </li>
  );
}
