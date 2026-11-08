// Single source of truth for user actions. The menu bar, command palette and
// keyboard shortcuts are all generated from this list, so every visible
// action has exactly one implementation and one enabled/disabled rule.

import { api, dialogs, toAppError } from "@/lib/ipc";
import { basename, joinPath } from "@/lib/format";
import { useApp, type WorkspaceId } from "@/store/app";
import { askText } from "@/store/prompt";
import { useBuild } from "@/store/build";
import { useProgrammer } from "@/store/programmer";
import { useProject } from "@/store/project";

export type MenuId = "file" | "edit" | "view" | "project" | "device" | "help";

export interface Command {
  id: string;
  label: string;
  menu: MenuId;
  group: number;
  /** e.g. "Mod+S" — Mod is Ctrl on Windows/Linux and ⌘ on macOS. */
  shortcut?: string;
  enabled: () => boolean;
  run: () => void | Promise<void>;
  /** Hidden from the menu bar (palette and shortcut only). */
  paletteOnly?: boolean;
}

const hasProject = () => useProject.getState().project !== null;
const hwIdle = () => hasProject() && !useProgrammer.getState().busy && !useBuild.getState().running;
const go = (w: WorkspaceId) => () => useApp.getState().setWorkspace(w);
const notify = (k: "info" | "success" | "warning" | "error", m: string) => useApp.getState().notify(k, m);

export const WORKSPACES: { id: WorkspaceId; label: string; needsProject: boolean; shortcut?: string }[] = [
  { id: "welcome", label: "Welcome", needsProject: false },
  { id: "visual", label: "Visual Logic", needsProject: true, shortcut: "Mod+1" },
  { id: "pins", label: "MCU & Pins", needsProject: true, shortcut: "Mod+2" },
  { id: "clock", label: "Clock & Peripherals", needsProject: true, shortcut: "Mod+3" },
  { id: "fuses", label: "Fuses", needsProject: true, shortcut: "Mod+4" },
  { id: "code", label: "Generated Code", needsProject: true, shortcut: "Mod+5" },
  { id: "build", label: "Build & Memory", needsProject: true, shortcut: "Mod+6" },
  { id: "programmer", label: "Device Programmer", needsProject: true, shortcut: "Mod+7" },
  { id: "serial", label: "Serial Monitor", needsProject: false, shortcut: "Mod+8" },
  { id: "settings", label: "Settings", needsProject: false, shortcut: "Mod+," },
];

async function openProjectDialog() {
  const path = await dialogs.pickFolder("Open AVR Studio Visual project folder");
  if (path) await useProject.getState().open(path);
}

async function duplicateProject() {
  const { root, project } = useProject.getState();
  if (!root || !project) return;
  const name = await askText({
    title: "Duplicate project",
    label: "Name for the copy",
    initial: `${project.manifest.name} copy`,
  });
  if (!name) return;
  if (useProject.getState().isDirty() && !(await useProject.getState().save())) return;
  try {
    const loaded = await api.duplicateProject(root, name.trim());
    await useProject.getState().load(loaded);
    notify("success", `Duplicated as “${loaded.project.manifest.name}”`);
  } catch (e) {
    notify("error", `Duplicate failed: ${toAppError(e).message}`);
  }
}

async function renameProject() {
  const p = useProject.getState().project;
  if (!p) return;
  const name = await askText({
    title: "Rename project",
    label: "Project name",
    hint: "Changes the display name in project.json. The folder on disk keeps its name.",
    initial: p.manifest.name,
  });
  if (!name || name === p.manifest.name) return;
  useProject.getState().update((d) => {
    d.manifest.name = name;
  });
  await useProject.getState().save();
}

async function exportProject() {
  const { root, project } = useProject.getState();
  if (!root || !project) return;
  if (useProject.getState().isDirty() && !(await useProject.getState().save())) return;
  const parent = await dialogs.pickFolder("Choose a folder to export the project into");
  if (!parent) return;
  const dest = joinPath(parent, `${basename(root)}-export`);
  try {
    await api.exportProject(root, dest);
    notify("success", `Exported to ${dest}`);
  } catch (e) {
    notify("error", `Export failed: ${toAppError(e).message}`);
  }
}

export const COMMANDS: Command[] = [
  // File
  {
    id: "project.new",
    label: "New Project…",
    menu: "file",
    group: 0,
    shortcut: "Mod+N",
    enabled: () => true,
    run: () => useApp.getState().setNewProjectOpen(true),
  },
  {
    id: "project.open",
    label: "Open Project…",
    menu: "file",
    group: 0,
    shortcut: "Mod+O",
    enabled: () => true,
    run: openProjectDialog,
  },
  {
    id: "project.save",
    label: "Save",
    menu: "file",
    group: 1,
    shortcut: "Mod+S",
    enabled: () => hasProject() && useProject.getState().isDirty(),
    run: async () => void (await useProject.getState().save()),
  },
  {
    id: "project.rename",
    label: "Rename Project…",
    menu: "file",
    group: 1,
    enabled: hasProject,
    run: renameProject,
  },
  {
    id: "project.duplicate",
    label: "Duplicate Project…",
    menu: "file",
    group: 1,
    enabled: hasProject,
    run: duplicateProject,
  },
  {
    id: "project.export",
    label: "Export Portable Folder…",
    menu: "file",
    group: 1,
    enabled: hasProject,
    run: exportProject,
  },
  {
    id: "project.close",
    label: "Close Project",
    menu: "file",
    group: 2,
    shortcut: "Mod+W",
    enabled: hasProject,
    run: async () => void (await useProject.getState().close()),
  },
  {
    id: "app.settings",
    label: "Settings",
    menu: "file",
    group: 3,
    shortcut: "Mod+,",
    enabled: () => true,
    run: go("settings"),
  },

  // Edit
  {
    id: "edit.undo",
    label: "Undo",
    menu: "edit",
    group: 0,
    shortcut: "Mod+Z",
    enabled: () => useProject.getState().past.length > 0,
    run: () => useProject.getState().undo(),
  },
  {
    id: "edit.redo",
    label: "Redo",
    menu: "edit",
    group: 0,
    shortcut: "Mod+Shift+Z",
    enabled: () => useProject.getState().future.length > 0,
    run: () => useProject.getState().redo(),
  },
  {
    id: "app.palette",
    label: "Command Palette…",
    menu: "edit",
    group: 1,
    shortcut: "Mod+Shift+P",
    enabled: () => true,
    run: () => useApp.getState().setPaletteOpen(true),
  },

  // View
  ...WORKSPACES.filter((w) => w.id !== "settings").map<Command>((w) => ({
    id: `view.${w.id}`,
    label: w.label,
    menu: "view",
    group: 0,
    shortcut: w.shortcut,
    enabled: () => !w.needsProject || hasProject(),
    run: go(w.id),
  })),
  {
    id: "view.explorer",
    label: "Toggle Project Explorer",
    menu: "view",
    group: 1,
    shortcut: "Mod+B",
    enabled: () => true,
    run: () => useApp.getState().toggle("showExplorer"),
  },
  {
    id: "view.inspector",
    label: "Toggle Properties Panel",
    menu: "view",
    group: 1,
    shortcut: "Mod+Alt+B",
    enabled: () => true,
    run: () => useApp.getState().toggle("showInspector"),
  },
  {
    id: "view.bottom",
    label: "Toggle Bottom Panel",
    menu: "view",
    group: 1,
    shortcut: "Mod+J",
    enabled: () => true,
    run: () => useApp.getState().toggle("showBottom"),
  },
  {
    id: "view.problems",
    label: "Show Problems",
    menu: "view",
    group: 1,
    shortcut: "Mod+Shift+M",
    enabled: () => true,
    run: () => useApp.getState().showBottomTab("problems"),
  },
  {
    id: "view.theme",
    label: "Toggle Light / Dark Theme",
    menu: "view",
    group: 2,
    enabled: () => true,
    run: () => {
      const a = useApp.getState();
      a.setTheme(a.theme === "dark" ? "light" : "dark");
    },
  },

  // Project
  {
    id: "build.start",
    label: "Build",
    menu: "project",
    group: 0,
    shortcut: "Mod+Shift+B",
    enabled: () => hasProject() && !useBuild.getState().running,
    run: async () => void (await useBuild.getState().start()),
  },
  {
    id: "build.cancel",
    label: "Cancel Build",
    menu: "project",
    group: 0,
    enabled: () => useBuild.getState().running,
    run: () => useBuild.getState().cancel(),
  },
  {
    id: "build.preview",
    label: "Show Generated Code",
    menu: "project",
    group: 1,
    enabled: hasProject,
    run: go("code"),
  },

  // Device
  {
    id: "device.identify",
    label: "Detect Device Signature",
    menu: "device",
    group: 0,
    enabled: hwIdle,
    run: () => useProgrammer.getState().runIdentify(),
  },
  {
    id: "device.upload",
    label: "Build and Upload",
    menu: "device",
    group: 0,
    shortcut: "Mod+U",
    enabled: hwIdle,
    run: async () => {
      const report = await useBuild.getState().start();
      if (report?.success) await useProgrammer.getState().runUpload();
    },
  },
  {
    id: "device.uploadOnly",
    label: "Upload Last Build",
    menu: "device",
    group: 0,
    enabled: hwIdle,
    run: () => useProgrammer.getState().runUpload(),
  },
  {
    id: "device.cancel",
    label: "Cancel Programmer Operation",
    menu: "device",
    group: 0,
    enabled: () => useProgrammer.getState().busy !== null,
    run: () => useProgrammer.getState().cancel(),
  },
  {
    id: "device.fuses",
    label: "Read Fuses",
    menu: "device",
    group: 1,
    enabled: hwIdle,
    run: async () => {
      useApp.getState().setWorkspace("fuses");
      await useProgrammer.getState().readFuses();
    },
  },
  {
    id: "device.programmer",
    label: "Programmer Settings",
    menu: "device",
    group: 1,
    enabled: hasProject,
    run: go("programmer"),
  },
  {
    id: "device.serial",
    label: "Serial Monitor",
    menu: "device",
    group: 2,
    enabled: () => true,
    run: go("serial"),
  },
  {
    id: "device.toolchains",
    label: "Re-detect Toolchains",
    menu: "device",
    group: 3,
    enabled: () => !useApp.getState().detecting,
    run: () => useApp.getState().detectToolchains(),
  },

  // Help
  { id: "help.welcome", label: "Welcome", menu: "help", group: 0, enabled: () => true, run: go("welcome") },
  {
    id: "help.about",
    label: "About AVR Studio Visual",
    menu: "help",
    group: 1,
    enabled: () => true,
    run: () => {
      const info = useApp.getState().info;
      notify(
        "info",
        `AVR Studio Visual ${info?.version ?? ""} · ${info?.devices.length ?? 0} device definitions · ${info?.os ?? ""}`,
      );
    },
  },
];

export const MENUS: { id: MenuId; label: string }[] = [
  { id: "file", label: "File" },
  { id: "edit", label: "Edit" },
  { id: "view", label: "View" },
  { id: "project", label: "Project" },
  { id: "device", label: "Device" },
  { id: "help", label: "Help" },
];

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

export function shortcutLabel(s: string): string {
  return s
    .split("+")
    .map((k) =>
      k === "Mod"
        ? isMac
          ? "⌘"
          : "Ctrl"
        : k === "Alt"
          ? isMac
            ? "⌥"
            : "Alt"
          : k === "Shift"
            ? isMac
              ? "⇧"
              : "Shift"
            : k,
    )
    .join(isMac ? "" : "+");
}

export function matchesShortcut(e: KeyboardEvent, s: string): boolean {
  const parts = s.split("+");
  const key = parts[parts.length - 1]!.toLowerCase();
  const mod = parts.includes("Mod");
  const shift = parts.includes("Shift");
  const alt = parts.includes("Alt");
  const modDown = isMac ? e.metaKey : e.ctrlKey;
  return modDown === mod && e.shiftKey === shift && e.altKey === alt && e.key.toLowerCase() === key;
}

export function runCommand(id: string) {
  const c = COMMANDS.find((x) => x.id === id);
  if (c && c.enabled()) void c.run();
}
