import { create } from "zustand";

import type { ContainerInfo } from "@/types";

interface PacksState {
  /** Folders waiting for the user to confirm zipping. */
  zipTargets: ContainerInfo[];
  /** Where to look for unpacked copies: a container id, `null` for every library, or closed. */
  unpackedScope: { id: number | null; name: string } | undefined;
  askZip: (folders: ContainerInfo[]) => void;
  closeZip: () => void;
  findUnpacked: (scope: { id: number | null; name: string }) => void;
  closeUnpacked: () => void;
}

export const usePacks = create<PacksState>()((set) => ({
  zipTargets: [],
  unpackedScope: undefined,
  askZip: (folders) => set({ zipTargets: folders }),
  closeZip: () => set({ zipTargets: [] }),
  findUnpacked: (scope) => set({ unpackedScope: scope }),
  closeUnpacked: () => set({ unpackedScope: undefined }),
}));

/** Folders in a selection that can be zipped: real folders, not library roots. */
export function zippable(containers: ContainerInfo[]): ContainerInfo[] {
  return containers.filter((c) => c.kind === "folder" && c.parentId !== null && !c.missing);
}
