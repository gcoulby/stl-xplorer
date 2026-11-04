import { useEffect } from "react";

import { AppShell } from "@/components/layout/AppShell";
import { TooltipProvider } from "@/components/ui/overlay";
import { useScanEvents } from "@/features/events";
import { useUi } from "@/store/ui";

function useTheme() {
  const theme = useUi((s) => s.theme);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.style.colorScheme = dark ? "dark" : "light";
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

export function App() {
  useTheme();
  useScanEvents();
  return (
    <TooltipProvider>
      <AppShell />
    </TooltipProvider>
  );
}
