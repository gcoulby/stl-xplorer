import { CircleCheck, Info, TriangleAlert, X } from "lucide-react";

import { useToasts } from "@/features/toast";
import { cn } from "@/lib/utils";

export function Toaster() {
  const toasts = useToasts((s) => s.toasts);
  const dismiss = useToasts((s) => s.dismiss);
  return (
    <div className="pointer-events-none fixed right-4 bottom-10 z-[60] flex w-80 flex-col gap-2">
      {toasts.map((t) => {
        const Icon = t.tone === "success" ? CircleCheck : t.tone === "error" ? TriangleAlert : Info;
        return (
          <div
            key={t.id}
            role="status"
            className="pointer-events-auto flex animate-in items-start gap-2.5 rounded-lg border bg-popover p-3 text-popover-foreground shadow-xl fade-in-0 slide-in-from-bottom-2"
          >
            <Icon
              className={cn(
                "mt-0.5 size-4 shrink-0",
                t.tone === "success" && "text-success",
                t.tone === "error" && "text-destructive",
                t.tone === "default" && "text-primary",
              )}
            />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium">{t.title}</p>
              {t.description && (
                <p className="mt-0.5 text-xs break-words text-muted-foreground selectable">
                  {t.description}
                </p>
              )}
            </div>
            <button
              type="button"
              onClick={() => dismiss(t.id)}
              className="rounded p-0.5 text-muted-foreground hover:text-foreground"
              aria-label="Dismiss"
            >
              <X className="size-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
}
