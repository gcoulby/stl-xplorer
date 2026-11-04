import { useMemo } from "react";

import {
  useBrowse,
  useCollectionItems,
  useCollections,
  useSearch,
  useTagged,
  useTags,
} from "@/features/queries";
import type { Location } from "@/store/ui";
import type { ContainerInfo, Crumb, ModelInfo, VirtualDir } from "@/types";

export type ListItem =
  | { key: string; type: "dir"; containerId: number; dir: VirtualDir }
  | { key: string; type: "container"; container: ContainerInfo }
  | { key: string; type: "model"; model: ModelInfo; location?: string };

export const modelKey = (id: number) => `m:${id}`;
export const containerKey = (id: number) => `c:${id}`;
export const dirKey = (containerId: number, path: string) => `d:${containerId}:${path}`;

export interface Listing {
  items: ListItem[];
  isLoading: boolean;
  error: unknown;
  title: string;
  crumbs: Crumb[] | null;
  /** The container being browsed, if any. */
  container: ContainerInfo | null;
  emptyHint: string;
}

/** Everything the middle pane shows for the current location, as one flat list. */
export function useListing(loc: Location): Listing {
  const browse = useBrowse(
    loc.kind === "container" ? loc.containerId : null,
    loc.kind === "container" ? loc.subpath : "",
  );
  const search = useSearch(loc.kind === "search" ? loc.query : "");
  const collection = useCollectionItems(loc.kind === "collection" ? loc.collectionId : null);
  const tagged = useTagged(loc.kind === "tag" ? loc.tagId : null);
  const collections = useCollections();
  const tags = useTags();

  return useMemo<Listing>(() => {
    switch (loc.kind) {
      case "container": {
        const d = browse.data;
        const stale =
          d &&
          (d.container.id !== loc.containerId ||
            d.subpath !== loc.subpath.replace(/^\/+|\/+$/g, ""));
        const items: ListItem[] = [];
        if (d) {
          for (const dir of d.dirs)
            items.push({
              key: dirKey(d.container.id, dir.path),
              type: "dir",
              containerId: d.container.id,
              dir,
            });
          for (const c of d.containers)
            items.push({ key: containerKey(c.id), type: "container", container: c });
          for (const m of d.models) items.push({ key: modelKey(m.id), type: "model", model: m });
        }
        return {
          items,
          isLoading: browse.isLoading || Boolean(stale && browse.isFetching),
          error: browse.error,
          title: d?.breadcrumbs.at(-1)?.name ?? "",
          crumbs: d?.breadcrumbs ?? null,
          container: d?.container ?? null,
          emptyHint:
            d?.container.kind === "folder"
              ? "No models or archives in this folder."
              : "No models at this level of the archive.",
        };
      }
      case "search": {
        const items: ListItem[] = (search.data ?? []).map((h) => ({
          key: modelKey(h.model.id),
          type: "model",
          model: h.model,
          location: h.location,
        }));
        return {
          items,
          isLoading: search.isLoading,
          error: search.error,
          title: `Results for “${loc.query.trim()}”`,
          crumbs: null,
          container: null,
          emptyHint: "Nothing matches. Try fewer or shorter words.",
        };
      }
      case "collection": {
        const name = collections.data?.find((c) => c.id === loc.collectionId)?.name ?? "Collection";
        return {
          items: (collection.data ?? []).map((c) => ({
            key: containerKey(c.id),
            type: "container" as const,
            container: c,
          })),
          isLoading: collection.isLoading,
          error: collection.error,
          title: name,
          crumbs: null,
          container: null,
          emptyHint: "This collection is empty. Right-click a pack and choose Add to collection.",
        };
      }
      case "tag": {
        const name = tags.data?.find((t) => t.id === loc.tagId)?.name ?? "Tag";
        const d = tagged.data;
        const items: ListItem[] = [];
        for (const c of d?.containers ?? [])
          items.push({ key: containerKey(c.id), type: "container", container: c });
        for (const m of d?.models ?? [])
          items.push({ key: modelKey(m.id), type: "model", model: m });
        return {
          items,
          isLoading: tagged.isLoading,
          error: tagged.error,
          title: `Tagged “${name}”`,
          crumbs: null,
          container: null,
          emptyHint: "Nothing has this tag yet.",
        };
      }
      default:
        return {
          items: [],
          isLoading: false,
          error: null,
          title: "",
          crumbs: null,
          container: null,
          emptyHint: "",
        };
    }
  }, [
    loc,
    browse.data,
    browse.isLoading,
    browse.isFetching,
    browse.error,
    search.data,
    search.isLoading,
    search.error,
    collection.data,
    collection.isLoading,
    collection.error,
    tagged.data,
    tagged.isLoading,
    tagged.error,
    collections.data,
    tags.data,
  ]);
}
