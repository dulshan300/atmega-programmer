import clsx from "clsx";
import { Cpu, Hammer, Redo2, Save, Search, Square, Undo2, Upload } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { COMMANDS, MENUS, runCommand, shortcutLabel, type MenuId } from "@/commands";
import { Button, IconButton, Kbd } from "@/components/ui";
import { useApp } from "@/store/app";
import { useBuild } from "@/store/build";
import { useProgrammer } from "@/store/programmer";
import { useProject } from "@/store/project";

function Menu({
  id,
  label,
  open,
  onOpen,
  onClose,
}: {
  id: MenuId;
  label: string;
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
}) {
  const items = COMMANDS.filter((c) => c.menu === id && !c.paletteOnly);
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (open) listRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [open]);

  const onKey = (e: React.KeyboardEvent) => {
    const buttons = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
    );
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === "ArrowDown") buttons[(i + 1) % buttons.length]?.focus();
    else if (e.key === "ArrowUp") buttons[(i - 1 + buttons.length) % buttons.length]?.focus();
    else if (e.key === "Escape") onClose();
    else return;
    e.preventDefault();
  };

  return (
    <div className="relative">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => (open ? onClose() : onOpen())}
        onMouseEnter={(e) => e.currentTarget.closest("[data-menubar-open]") && onOpen()}
        className={clsx(
          "h-7 rounded px-2 text-[13px] text-muted hover:bg-raised hover:text-text",
          open && "bg-raised text-text",
        )}
      >
        {label}
      </button>
      {open && (
        <div
          ref={listRef}
          role="menu"
          aria-label={label}
          onKeyDown={onKey}
          className="absolute top-8 left-0 z-50 min-w-60 rounded-md border border-line bg-panel py-1 shadow-xl"
        >
          {items.map((c, i) => {
            const enabled = c.enabled();
            const sep = i > 0 && items[i - 1]!.group !== c.group;
            return (
              <div key={c.id}>
                {sep && <div className="my-1 border-t border-line" role="separator" />}
                <button
                  type="button"
                  role="menuitem"
                  disabled={!enabled}
                  onClick={() => {
                    onClose();
                    runCommand(c.id);
                  }}
                  className="flex w-full items-center justify-between gap-6 px-3 py-1 text-left text-[13px] hover:bg-accent/15 focus:bg-accent/15 focus:outline-none disabled:text-faint"
                >
                  <span>{c.label}</span>
                  {c.shortcut && (
                    <span className="font-mono text-[11px] text-faint">{shortcutLabel(c.shortcut)}</span>
                  )}
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export function TopBar() {
  const [openMenu, setOpenMenu] = useState<MenuId | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  const project = useProject((s) => s.project);
  const dirty = useProject((s) => s.project !== null && JSON.stringify(s.project) !== s.saved);
  const canUndo = useProject((s) => s.past.length > 0);
  const canRedo = useProject((s) => s.future.length > 0);
  const saving = useProject((s) => s.saving);
  const building = useBuild((s) => s.running);
  const hwBusy = useProgrammer((s) => s.busy);
  const setPaletteOpen = useApp((s) => s.setPaletteOpen);

  useEffect(() => {
    if (!openMenu) return;
    const onDown = (e: MouseEvent) => {
      if (!barRef.current?.contains(e.target as Node)) setOpenMenu(null);
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [openMenu]);

  return (
    <header className="flex h-10 shrink-0 items-center gap-2 border-b border-line bg-panel px-2">
      <div className="flex items-center gap-1.5 pr-2 pl-1">
        <Cpu size={16} className="text-accent" aria-hidden />
        <span className="text-[13px] font-semibold tracking-tight">AVR Studio Visual</span>
      </div>
      <nav
        ref={barRef}
        className="flex items-center"
        role="menubar"
        data-menubar-open={openMenu ? "" : undefined}
      >
        {MENUS.map((m) => (
          <Menu
            key={m.id}
            id={m.id}
            label={m.label}
            open={openMenu === m.id}
            onOpen={() => setOpenMenu(m.id)}
            onClose={() => setOpenMenu(null)}
          />
        ))}
      </nav>

      <div className="mx-2 h-5 border-l border-line" />

      {project && (
        <div className="flex items-center gap-0.5" role="toolbar" aria-label="Project toolbar">
          <IconButton
            icon={Save}
            label="Save (Ctrl+S)"
            disabled={!dirty || saving}
            onClick={() => runCommand("project.save")}
          />
          <IconButton icon={Undo2} label="Undo" disabled={!canUndo} onClick={() => runCommand("edit.undo")} />
          <IconButton icon={Redo2} label="Redo" disabled={!canRedo} onClick={() => runCommand("edit.redo")} />
          <div className="mx-1 h-5 border-l border-line" />
          {building ? (
            <Button size="sm" icon={Square} onClick={() => runCommand("build.cancel")}>
              Cancel build
            </Button>
          ) : (
            <Button
              size="sm"
              icon={Hammer}
              disabled={hwBusy !== null}
              onClick={() => runCommand("build.start")}
            >
              Build
            </Button>
          )}
          {hwBusy ? (
            <Button size="sm" variant="danger" icon={Square} onClick={() => runCommand("device.cancel")}>
              Cancel {hwBusy.replace("-", " ")}
            </Button>
          ) : (
            <Button
              size="sm"
              variant="primary"
              icon={Upload}
              disabled={building}
              onClick={() => runCommand("device.upload")}
            >
              Build &amp; Upload
            </Button>
          )}
        </div>
      )}

      <div className="flex-1" />

      {project && (
        <div className="hidden truncate text-xs text-muted md:block" title={useProject.getState().root ?? ""}>
          {project.manifest.name}
          {dirty && <span className="ml-1 text-warn">● unsaved</span>}
        </div>
      )}
      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        className="flex h-7 items-center gap-2 rounded border border-line bg-panel-2 px-2 text-xs text-muted hover:border-line-strong"
      >
        <Search size={13} aria-hidden /> Commands <Kbd>{shortcutLabel("Mod+Shift+P")}</Kbd>
      </button>
    </header>
  );
}
