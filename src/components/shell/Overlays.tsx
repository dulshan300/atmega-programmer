import { Command as Cmdk } from "cmdk";
import { EMPTY } from "@/lib/hooks";
import clsx from "clsx";
import { AlertCircle, CheckCircle2, Info, TriangleAlert, X } from "lucide-react";
import { useState } from "react";
import type { TextPrompt } from "@/store/prompt";
import { COMMANDS, runCommand, shortcutLabel } from "@/commands";
import { Button, Dialog, Field, Input } from "@/components/ui";
import { useApp } from "@/store/app";
import { usePrompt } from "@/store/prompt";
import { useProject } from "@/store/project";

const MENU_LABEL = {
  file: "File",
  edit: "Edit",
  view: "View",
  project: "Project",
  device: "Device",
  help: "Help",
};

export function CommandPalette() {
  const open = useApp((s) => s.paletteOpen);
  const setOpen = useApp((s) => s.setPaletteOpen);
  const recent = useApp((s) => s.settings?.recentProjects ?? EMPTY);
  const openProject = useProject((s) => s.open);
  if (!open) return null;
  return (
    <div
      className="fixed inset-0 z-50 flex justify-center bg-black/40 pt-[12vh]"
      onMouseDown={() => setOpen(false)}
    >
      <Cmdk
        label="Command palette"
        loop
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.key === "Escape" && setOpen(false)}
        className="flex h-fit max-h-[60vh] w-[min(560px,92vw)] flex-col overflow-hidden rounded-lg border border-line bg-panel shadow-2xl"
      >
        <Cmdk.Input
          autoFocus
          placeholder="Type a command or project name…"
          className="h-10 border-b border-line bg-transparent px-3 text-sm outline-none placeholder:text-faint"
        />
        <Cmdk.List className="overflow-auto p-1">
          <Cmdk.Empty className="px-3 py-4 text-center text-xs text-muted">No matching commands.</Cmdk.Empty>
          <Cmdk.Group
            heading="Commands"
            className="text-[11px] text-faint [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1"
          >
            {COMMANDS.filter((c) => c.enabled()).map((c) => (
              <Cmdk.Item
                key={c.id}
                value={`${MENU_LABEL[c.menu]}: ${c.label}`}
                onSelect={() => {
                  setOpen(false);
                  runCommand(c.id);
                }}
                className="flex cursor-pointer items-center justify-between rounded px-2 py-1.5 text-[13px] text-text data-[selected=true]:bg-accent/15"
              >
                <span>
                  <span className="text-faint">{MENU_LABEL[c.menu]}: </span>
                  {c.label}
                </span>
                {c.shortcut && (
                  <span className="font-mono text-[11px] text-faint">{shortcutLabel(c.shortcut)}</span>
                )}
              </Cmdk.Item>
            ))}
          </Cmdk.Group>
          {recent.length > 0 && (
            <Cmdk.Group
              heading="Open recent"
              className="text-[11px] text-faint [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1"
            >
              {recent.map((r) => (
                <Cmdk.Item
                  key={r.path}
                  value={`Open recent: ${r.name} ${r.path}`}
                  onSelect={() => {
                    setOpen(false);
                    void openProject(r.path);
                  }}
                  className="flex cursor-pointer flex-col rounded px-2 py-1.5 text-[13px] text-text data-[selected=true]:bg-accent/15"
                >
                  <span>{r.name}</span>
                  <span className="truncate text-[11px] text-faint">{r.path}</span>
                </Cmdk.Item>
              ))}
            </Cmdk.Group>
          )}
        </Cmdk.List>
      </Cmdk>
    </div>
  );
}

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismiss);
  const icons = { info: Info, success: CheckCircle2, warning: TriangleAlert, error: AlertCircle };
  const tones = { info: "text-info", success: "text-ok", warning: "text-warn", error: "text-danger" };
  return (
    <div
      className="pointer-events-none fixed right-3 bottom-9 z-50 flex w-[min(420px,90vw)] flex-col gap-2"
      aria-live="polite"
    >
      {toasts.map((t) => {
        const Icon = icons[t.kind];
        return (
          <div
            key={t.id}
            role={t.kind === "error" ? "alert" : "status"}
            className="pointer-events-auto flex items-start gap-2 rounded-md border border-line bg-panel px-3 py-2 text-xs shadow-xl"
          >
            <Icon size={15} className={clsx("mt-px shrink-0", tones[t.kind])} aria-hidden />
            <div className="selectable flex-1 break-words whitespace-pre-wrap">{t.message}</div>
            <button
              type="button"
              aria-label="Dismiss"
              onClick={() => dismiss(t.id)}
              className="text-faint hover:text-text"
            >
              <X size={13} />
            </button>
          </div>
        );
      })}
    </div>
  );
}

export function PromptDialog() {
  const current = usePrompt((s) => s.current);
  return current ? <PromptBody key={`${current.title}:${current.initial ?? ""}`} current={current} /> : null;
}

function PromptBody({ current }: { current: TextPrompt & { resolve: (v: string | null) => void } }) {
  const [value, setValue] = useState(current.initial ?? "");
  const error = current.validate?.(value.trim()) ?? (value.trim() ? null : "Required");
  const submit = () => !error && current.resolve(value);
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && current.resolve(null)}
      title={current.title}
      width={420}
      footer={
        <>
          <Button onClick={() => current.resolve(null)}>Cancel</Button>
          <Button variant="primary" disabled={!!error} onClick={submit}>
            OK
          </Button>
        </>
      }
    >
      <Field label={current.label} hint={current.hint} error={value.trim() ? error : undefined}>
        <Input
          autoFocus
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
      </Field>
    </Dialog>
  );
}
