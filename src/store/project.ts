import { create } from "zustand";
import { api, toAppError } from "@/lib/ipc";
import { ProjectSchema, describeIssues } from "@/lib/schemas";
import type { DeviceDefinition, Diagnostic, LoadedProject, Project, Template } from "@/lib/types";
import { useApp } from "./app";

const HISTORY_LIMIT = 100;

export type Selection =
  | { kind: "none" }
  | { kind: "node"; id: string }
  | { kind: "pin"; pin: string }
  | { kind: "edge"; id: string };

interface ProjectState {
  root: string | null;
  project: Project | null;
  /** JSON of the project as last saved/loaded; used for the dirty flag. */
  saved: string | null;
  device: DeviceDefinition | null;
  diagnostics: Diagnostic[];
  validating: boolean;
  notes: string[];
  selection: Selection;
  past: Project[];
  future: Project[];
  saving: boolean;

  isDirty(): boolean;
  load(loaded: LoadedProject): Promise<void>;
  open(path: string): Promise<boolean>;
  create(req: {
    parentDir: string;
    name: string;
    mcu: string;
    package: string;
    template: Template;
  }): Promise<boolean>;
  save(): Promise<boolean>;
  close(): Promise<boolean>;
  /** Applies an edit. `recipe` mutates a deep copy. `coalesce` merges with the previous history step. */
  update(recipe: (draft: Project) => void, opts?: { coalesce?: string }): void;
  undo(): void;
  redo(): void;
  select(s: Selection): void;
  revalidate(): void;
}

let validateTimer: ReturnType<typeof setTimeout> | undefined;
let validateSeq = 0;
let lastCoalesce: { key: string; at: number } | null = null;

const notify = (kind: "info" | "success" | "warning" | "error", msg: string) =>
  useApp.getState().notify(kind, msg);

export const useProject = create<ProjectState>()((set, get) => ({
  root: null,
  project: null,
  saved: null,
  device: null,
  diagnostics: [],
  validating: false,
  notes: [],
  selection: { kind: "none" },
  past: [],
  future: [],
  saving: false,

  isDirty() {
    const { project, saved } = get();
    return project !== null && JSON.stringify(project) !== saved;
  },

  async load(loaded) {
    const device = await api.getDevice(loaded.project.manifest.device.mcu);
    set({
      root: loaded.root,
      project: loaded.project,
      saved: JSON.stringify(loaded.project),
      device,
      notes: loaded.notes,
      diagnostics: [],
      selection: { kind: "none" },
      past: [],
      future: [],
    });
    for (const n of loaded.notes) notify("info", n);
    const app = useApp.getState();
    void api.getSettings().then((s) => app.setSettings(s.settings));
    app.setWorkspace("visual");
    get().revalidate();
  },

  async open(path) {
    if (!(await get().close())) return false;
    try {
      await get().load(await api.openProject(path));
      return true;
    } catch (e) {
      notify("error", `Could not open project: ${toAppError(e).message}`);
      return false;
    }
  },

  async create(req) {
    if (!(await get().close())) return false;
    try {
      await get().load(await api.createProject(req));
      notify("success", `Created project “${req.name}”`);
      return true;
    } catch (e) {
      notify("error", `Could not create project: ${toAppError(e).message}`);
      return false;
    }
  },

  async save() {
    const { root, project } = get();
    if (!root || !project) return false;
    const check = ProjectSchema.safeParse(project);
    if (!check.success) {
      notify("error", `Not saved — project data is invalid: ${describeIssues(check.error)}`);
      return false;
    }
    set({ saving: true });
    try {
      await api.saveProject(root, project);
      set({ saved: JSON.stringify(project) });
      return true;
    } catch (e) {
      notify("error", `Save failed: ${toAppError(e).message}`);
      return false;
    } finally {
      set({ saving: false });
    }
  },

  async close() {
    const s = get();
    if (!s.project) return true;
    if (s.isDirty()) {
      const { dialogs } = await import("@/lib/ipc");
      const discard = await dialogs.confirm(
        `“${s.project.manifest.name}” has unsaved changes. Close it and discard them?`,
        "Unsaved changes",
      );
      if (!discard) return false;
    }
    set({
      root: null,
      project: null,
      saved: null,
      device: null,
      diagnostics: [],
      past: [],
      future: [],
      selection: { kind: "none" },
    });
    useApp.getState().setWorkspace("welcome");
    return true;
  },

  update(recipe, opts) {
    const { project, past } = get();
    if (!project) return;
    const next = structuredClone(project);
    recipe(next);
    if (JSON.stringify(next) === JSON.stringify(project)) return;
    const now = Date.now();
    const merge = opts?.coalesce && lastCoalesce?.key === opts.coalesce && now - lastCoalesce.at < 1500;
    lastCoalesce = opts?.coalesce ? { key: opts.coalesce, at: now } : null;
    set({
      project: next,
      past: merge ? past : [...past.slice(-(HISTORY_LIMIT - 1)), project],
      future: [],
    });
    get().revalidate();
  },

  undo() {
    const { past, project, future } = get();
    const prev = past[past.length - 1];
    if (!prev || !project) return;
    lastCoalesce = null;
    set({ project: prev, past: past.slice(0, -1), future: [project, ...future] });
    get().revalidate();
  },

  redo() {
    const { past, project, future } = get();
    const [next, ...rest] = future;
    if (!next || !project) return;
    lastCoalesce = null;
    set({ project: next, past: [...past, project], future: rest });
    get().revalidate();
  },

  select: (selection) => set({ selection }),

  revalidate() {
    clearTimeout(validateTimer);
    set({ validating: true });
    validateTimer = setTimeout(async () => {
      const project = get().project;
      if (!project) return;
      const seq = ++validateSeq;
      try {
        const diagnostics = await api.validateProject(project);
        if (seq === validateSeq) set({ diagnostics, validating: false });
      } catch (e) {
        if (seq === validateSeq)
          set({
            diagnostics: [{ severity: "error", code: "validate.failed", message: toAppError(e).message }],
            validating: false,
          });
      }
    }, 250);
  },
}));

export function diagnosticCounts(d: Diagnostic[]) {
  return {
    errors: d.filter((x) => x.severity === "error").length,
    warnings: d.filter((x) => x.severity === "warning").length,
  };
}
