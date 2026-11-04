import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { invalidateIndex } from "@/features/queries";
import { toast } from "@/features/toast";
import { useUi } from "@/store/ui";
import type { ScanFinished, ScanProgress } from "@/types";

/** Wire backend scan events into the status bar and the query cache. */
export function useScanEvents() {
  const qc = useQueryClient();
  useEffect(() => {
    const offs: Array<Promise<() => void>> = [];
    offs.push(listen<ScanProgress>("scan-progress", (e) => useUi.getState().setScan(e.payload)));
    offs.push(
      listen<ScanFinished>("scan-finished", (e) => {
        const f = e.payload;
        useUi.getState().clearScan(f.libraryId);
        if (f.error) {
          toast.error(`Could not scan ${f.libraryName || "library"}`, f.error);
        }
        if (f.changed > 0 || f.error) invalidateIndex(qc);
        else void qc.invalidateQueries({ queryKey: ["libraries"] });
      }),
    );
    return () => {
      for (const p of offs) void p.then((off) => off());
    };
  }, [qc]);
}
