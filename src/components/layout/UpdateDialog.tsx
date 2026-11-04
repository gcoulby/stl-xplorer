import { Download, LoaderCircle } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/overlay";
import { useUpdater } from "@/features/updater";
import { formatBytes } from "@/lib/utils";

/** Checks once shortly after startup, unless turned off in settings. */
export function useStartupUpdateCheck() {
  useEffect(() => {
    // A dev build would replace the installed app with a release, so leave it to a manual check.
    if (import.meta.env.DEV || !useUpdater.getState().autoCheck) return;
    const t = window.setTimeout(() => void useUpdater.getState().check({ quiet: true }), 5000);
    return () => window.clearTimeout(t);
  }, []);
}

export function UpdateDialog() {
  const { status, dialogOpen, setDialogOpen, install } = useUpdater();
  const update =
    status.kind === "available" || status.kind === "downloading" || status.kind === "installing"
      ? status.update
      : null;
  const busy = status.kind === "downloading" || status.kind === "installing";
  const pct =
    status.kind === "downloading" && status.total
      ? Math.min(100, Math.round((status.done / status.total) * 100))
      : null;

  return (
    <Dialog open={dialogOpen && update !== null} onOpenChange={(o) => !busy && setDialogOpen(o)}>
      <DialogContent className="sm:max-w-lg" hideClose={busy}>
        <div className="flex flex-col gap-1 pr-6">
          <DialogTitle className="text-base">
            STL Xplorer {update?.version} is available
          </DialogTitle>
          <DialogDescription>
            You have {update?.currentVersion}.
            {update?.date && ` Released ${formatDate(update.date)}.`} The app closes while the
            update installs, then opens again.
          </DialogDescription>
        </div>

        {update?.body && (
          <div className="max-h-72 overflow-y-auto rounded-md border bg-muted/30 p-3 text-xs leading-relaxed whitespace-pre-wrap selectable">
            {update.body.trim()}
          </div>
        )}

        {busy && (
          <div className="flex flex-col gap-1.5">
            <div className="h-1.5 overflow-hidden rounded-full bg-muted">
              <div
                className="h-full bg-primary transition-[width]"
                style={{ width: `${status.kind === "installing" ? 100 : (pct ?? 5)}%` }}
              />
            </div>
            <p className="text-xs text-muted-foreground tabular-nums">
              {status.kind === "installing"
                ? "Installing…"
                : `Downloading${status.total ? ` ${formatBytes(status.done)} of ${formatBytes(status.total)}` : "…"}`}
            </p>
          </div>
        )}

        <div className="flex justify-end gap-2">
          <Button variant="ghost" disabled={busy} onClick={() => setDialogOpen(false)}>
            Later
          </Button>
          <Button disabled={busy} onClick={() => void install()}>
            {busy ? <LoaderCircle className="animate-spin" /> : <Download />} Install and restart
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

function formatDate(raw: string): string {
  // Tauri reports RFC 3339 or "2026-11-04 10:00:00.0 +00:00:00"; take the date part either way.
  const d = new Date(raw.slice(0, 10));
  return Number.isNaN(d.getTime())
    ? raw
    : d.toLocaleDateString("en-GB", { day: "numeric", month: "long", year: "numeric" });
}
