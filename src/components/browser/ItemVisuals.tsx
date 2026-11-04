import {
  Box,
  FileArchive,
  FileBox,
  FileText,
  Folder,
  FolderArchive,
  Image as ImageIcon,
  FolderClosed,
  Layers,
  Library as LibraryIcon,
  Lock,
  Package,
  ShieldCheck,
  TriangleAlert,
  Zap,
} from "lucide-react";

import { Badge } from "@/components/ui/misc";
import { Tip } from "@/components/ui/overlay";
import { useTags } from "@/features/queries";
import { cn, fileKind } from "@/lib/utils";
import type { ContainerInfo, ModelInfo } from "@/types";

export function ContainerIcon({
  container,
  className,
  library,
}: {
  container: Pick<ContainerInfo, "kind" | "isPack" | "vaulted">;
  className?: string;
  library?: boolean;
}) {
  const cls = cn("size-4 shrink-0", className);
  if (library) return <LibraryIcon className={cn(cls, "text-primary")} />;
  if (container.kind === "folder") {
    if (container.isPack) return <Package className={cn(cls, "text-violet-400")} />;
    return <Folder className={cn(cls, "text-sky-400/90 fill-sky-400/15")} />;
  }
  return <FileArchive className={cn(cls, "text-amber-400")} />;
}

export function DirIcon({ className }: { className?: string }) {
  return <FolderClosed className={cn("size-4 shrink-0 text-amber-300/80", className)} />;
}

export function ArchiveDirIcon({ className }: { className?: string }) {
  return <FolderArchive className={cn("size-4 shrink-0 text-amber-300/80", className)} />;
}

export function ModelIcon({ ext, className }: { ext: string; className?: string }) {
  const cls = cn("size-4 shrink-0", className);
  if (ext === "stl") return <Box className={cn(cls, "text-primary")} />;
  if (ext === "3mf") return <FileBox className={cn(cls, "text-emerald-400")} />;
  const kind = fileKind(ext);
  if (kind === "image") return <ImageIcon className={cn(cls, "text-teal-400")} />;
  if (kind === "pdf") return <FileText className={cn(cls, "text-rose-400")} />;
  return <Layers className={cn(cls, "text-pink-400")} />;
}

export function ContainerBadges({ c, compact = false }: { c: ContainerInfo; compact?: boolean }) {
  const out: React.ReactNode[] = [];
  if (c.status === "missing_parts") {
    out.push(
      <Tip key="mp" label={c.statusDetail ?? "A part is missing"}>
        <Badge variant="warning">
          <TriangleAlert />
          {c.statusDetail ?? "Missing parts"}
        </Badge>
      </Tip>,
    );
  } else if (c.status === "corrupt" || c.status === "unsupported") {
    out.push(
      <Tip key="err" label={c.statusDetail ?? "This archive could not be read"}>
        <Badge variant="destructive">
          <TriangleAlert />
          {c.status === "corrupt" ? "Damaged" : "Unreadable"}
        </Badge>
      </Tip>,
    );
  }
  if (c.encrypted) {
    out.push(
      <Tip key="enc" label="Password protected. Previews are skipped in this version.">
        <Badge variant="secondary">
          <Lock />
          {!compact && "Locked"}
        </Badge>
      </Tip>,
    );
  }
  if (c.solid && c.status === "ok") {
    out.push(
      <Tip
        key="solid"
        label="Solid archive: the first preview unpacks its models once, then every file opens instantly."
      >
        <Badge variant="outline">
          <Zap />
          {!compact && "Solid"}
        </Badge>
      </Tip>,
    );
  }
  if (c.partCount > 1 && !compact) {
    out.push(
      <Badge key="parts" variant="outline">
        {c.partCount} parts
      </Badge>,
    );
  }
  if (c.isPack && c.kind === "folder") {
    out.push(
      <Badge key="pack" variant="secondary">
        Pack
      </Badge>,
    );
  }
  if (c.vaulted) out.push(<VaultBadge key="v" compact={compact} />);
  return <>{out}</>;
}

export function VaultBadge({ compact }: { compact?: boolean }) {
  return (
    <Tip label="Saved in the vault. It will not be cleaned up.">
      <Badge variant="success">
        <ShieldCheck />
        {!compact && "Vault"}
      </Badge>
    </Tip>
  );
}

export function ModelBadges({ m, compact = false }: { m: ModelInfo; compact?: boolean }) {
  return (
    <>
      {m.encrypted && (
        <Tip label="Password protected">
          <Badge variant="secondary">
            <Lock />
          </Badge>
        </Tip>
      )}
      {m.vaulted && !m.inArchive && <VaultBadge compact={compact} />}
    </>
  );
}

export function TagDots({ ids, max = 4 }: { ids: number[]; max?: number }) {
  const tags = useTags().data ?? [];
  if (ids.length === 0) return null;
  const shown = ids
    .map((id) => tags.find((t) => t.id === id))
    .filter((t): t is NonNullable<typeof t> => Boolean(t));
  return (
    <Tip label={shown.map((t) => t.name).join(", ")}>
      <span className="flex items-center -space-x-1">
        {shown.slice(0, max).map((t) => (
          <span
            key={t.id}
            className="size-2.5 rounded-full ring-2 ring-background"
            style={{ backgroundColor: t.colour }}
          />
        ))}
      </span>
    </Tip>
  );
}
