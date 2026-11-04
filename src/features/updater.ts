import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { create } from "zustand";
import { persist } from "zustand/middleware";

import { errorMessage } from "@/lib/api";

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current"; checkedAt: number }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; update: Update; done: number; total: number | null }
  | { kind: "installing"; update: Update }
  | { kind: "error"; message: string };

interface UpdaterState {
  status: UpdateStatus;
  dialogOpen: boolean;
  /** Persisted: look for a new release a few seconds after startup. */
  autoCheck: boolean;
  setAutoCheck: (v: boolean) => void;
  setDialogOpen: (v: boolean) => void;
  /** `quiet` checks never show errors: the user did not ask for them. */
  check: (opts?: { quiet?: boolean }) => Promise<void>;
  install: () => Promise<void>;
}

export const useUpdater = create<UpdaterState>()(
  persist(
    (set, get) => ({
      status: { kind: "idle" },
      dialogOpen: false,
      autoCheck: true,
      setAutoCheck: (autoCheck) => set({ autoCheck }),
      setDialogOpen: (dialogOpen) => set({ dialogOpen }),

      check: async ({ quiet = false } = {}) => {
        const s = get().status.kind;
        if (s === "checking" || s === "downloading" || s === "installing") return;
        set({ status: { kind: "checking" } });
        try {
          const update = await check({ timeout: 20_000 });
          set({
            status: update
              ? { kind: "available", update }
              : { kind: "current", checkedAt: Date.now() },
          });
        } catch (e) {
          set({ status: quiet ? { kind: "idle" } : { kind: "error", message: describe(e) } });
        }
      },

      install: async () => {
        const st = get().status;
        if (st.kind !== "available") return;
        const update = st.update;
        let done = 0;
        let total: number | null = null;
        try {
          await update.downloadAndInstall((ev) => {
            if (ev.event === "Started") total = ev.data.contentLength ?? null;
            else if (ev.event === "Progress") done += ev.data.chunkLength;
            else {
              set({ status: { kind: "installing", update } });
              return;
            }
            set({ status: { kind: "downloading", update, done, total } });
          });
          // On Windows the installer closes the app and opens the new version itself.
          set({ status: { kind: "installing", update } });
        } catch (e) {
          set({ status: { kind: "error", message: describe(e) } });
        }
      },
    }),
    {
      name: "stl-xplorer-updater",
      partialize: (s) => ({ autoCheck: s.autoCheck }),
    },
  ),
);

function describe(e: unknown): string {
  const msg = errorMessage(e);
  // No latest.json yet (a release from before the updater) reads as a missing file, not a fault.
  if (/404|not found|valid release JSON/i.test(msg))
    return "No update information is published yet.";
  if (/network|dns|connect|timed out|timeout/i.test(msg))
    return "Could not reach GitHub. Check your connection and try again.";
  return msg;
}

let version: Promise<string> | null = null;
export function appVersion(): Promise<string> {
  version ??= getVersion();
  return version;
}
