import { ShieldCheck, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/overlay";
import { useDeleteConfirm, useDeleteItems } from "@/features/delete";
import { plural } from "@/lib/utils";

const SHOWN = 6;

export function DeleteDialog() {
  const { targets, skipped, close } = useDeleteConfirm();
  const del = useDeleteItems();
  const open = targets.length > 0;
  const single = targets.length === 1 ? targets[0] : undefined;
  const vaulted = targets.filter((t) => t.vaulted).length;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-md">
        <div className="flex items-start gap-3">
          <div className="flex size-9 shrink-0 items-center justify-center rounded-full bg-destructive/15">
            <Trash2 className="size-4 text-destructive" />
          </div>
          <div className="flex min-w-0 flex-col gap-1">
            <DialogTitle className="text-base leading-snug break-words">
              {single ? `Delete “${single.name}”?` : `Delete ${plural(targets.length, "item")}?`}
            </DialogTitle>
            <DialogDescription>
              {single?.note
                ? `This deletes the ${single.note}. `
                : !single
                  ? "Folders go with everything in them, and split archives with all their parts. "
                  : ""}
              Everything goes to the Recycle Bin, so you can restore it from there.
            </DialogDescription>
          </div>
        </div>

        {!single && (
          <ul className="flex flex-col gap-0.5 rounded-md border bg-muted/30 p-2 text-xs">
            {targets.slice(0, SHOWN).map((t) => (
              <li key={t.key} className="flex gap-2">
                <span className="min-w-0 truncate">{t.name}</span>
                {t.note && <span className="shrink-0 text-muted-foreground">{t.note}</span>}
              </li>
            ))}
            {targets.length > SHOWN && (
              <li className="text-muted-foreground">and {targets.length - SHOWN} more</li>
            )}
          </ul>
        )}

        {vaulted > 0 && (
          <p className="flex items-center gap-1.5 text-xs text-warning">
            <ShieldCheck className="size-3.5" />
            {vaulted === targets.length && single
              ? "This is in the vault."
              : `${plural(vaulted, "item")} ${vaulted === 1 ? "is" : "are"} in the vault.`}
          </p>
        )}
        {skipped > 0 && (
          <p className="text-xs text-muted-foreground">
            {plural(skipped, "selected item")} inside archives {skipped === 1 ? "is" : "are"} left
            alone. Delete the archive to remove {skipped === 1 ? "it" : "them"}.
          </p>
        )}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={close} autoFocus>
            Cancel
          </Button>
          <Button
            variant="destructive"
            disabled={del.isPending}
            onClick={() => {
              del.mutate(targets);
              close();
            }}
          >
            <Trash2 /> Delete
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
