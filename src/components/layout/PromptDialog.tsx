import { useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/overlay";
import { usePrompt } from "@/features/selection";

export function PromptDialog() {
  const { open, title, close } = usePrompt();
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      <DialogContent className="sm:max-w-sm">
        <DialogTitle>{title}</DialogTitle>
        <DialogDescription className="sr-only">Enter a name</DialogDescription>
        {open && <PromptForm />}
      </DialogContent>
    </Dialog>
  );
}

function PromptForm() {
  const { placeholder, initial, onSubmit, close } = usePrompt();
  const [value, setValue] = useState(initial);
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(e) => {
        e.preventDefault();
        const v = value.trim();
        if (!v) return;
        onSubmit?.(v);
        close();
      }}
    >
      <Input
        autoFocus
        value={value}
        placeholder={placeholder}
        onChange={(e) => setValue(e.target.value)}
      />
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" onClick={close}>
          Cancel
        </Button>
        <Button type="submit" disabled={!value.trim()}>
          Save
        </Button>
      </div>
    </form>
  );
}
