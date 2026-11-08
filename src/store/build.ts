import { create } from "zustand";
import { api, events, toAppError } from "@/lib/ipc";
import type { BuildReport, BuildStage, Stream } from "@/lib/types";
import { useApp } from "./app";
import { useProject } from "./project";

const MAX_LINES = 5000;

export interface LogLine {
  stream: Stream | "info";
  text: string;
}

interface BuildState {
  running: boolean;
  stage: BuildStage | null;
  lines: LogLine[];
  report: BuildReport | null;
  /** Generated files edited by hand block the build until the user confirms. */
  conflict: string | null;
  start(opts?: { force?: boolean }): Promise<BuildReport | null>;
  cancel(): Promise<void>;
  clear(): void;
}

function push(lines: LogLine[], add: LogLine[]): LogLine[] {
  const next = lines.concat(add);
  return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
}

export const useBuild = create<BuildState>()((set, get) => ({
  running: false,
  stage: null,
  lines: [],
  report: null,
  conflict: null,

  async start(opts) {
    const app = useApp.getState();
    const proj = useProject.getState();
    if (!proj.root || get().running) return null;
    // The backend builds from disk, so unsaved edits are saved first.
    if (proj.isDirty() && !(await proj.save())) return null;
    set({ running: true, stage: "validate", lines: [], report: null, conflict: null });
    app.showBottomTab("output");
    const unlisten = await events.build((e) => {
      if (e.kind === "stage") set({ stage: e.stage });
      else set((s) => ({ lines: push(s.lines, [{ stream: e.stream, text: e.text }]) }));
    });
    try {
      const report = await api.startBuild(proj.root, opts?.force ?? false);
      const conflict = !report.success && report.message.includes("edited by hand") ? report.message : null;
      set({ report, stage: report.stage, conflict });
      if (report.success)
        app.notify("success", `Build succeeded in ${(report.durationMs / 1000).toFixed(1)} s`);
      else if (report.cancelled) app.notify("info", "Build cancelled");
      else if (!conflict) {
        app.notify("error", report.message);
        if (report.diagnostics.some((d) => d.severity === "error")) app.showBottomTab("problems");
      }
      return report;
    } catch (e) {
      const msg = toAppError(e).message;
      set((s) => ({ lines: push(s.lines, [{ stream: "stderr", text: msg }]) }));
      app.notify("error", msg);
      return null;
    } finally {
      unlisten();
      set({ running: false });
    }
  },

  async cancel() {
    const root = useProject.getState().root;
    if (root) await api.cancelBuild(root).catch(() => {});
  },

  clear: () => set({ lines: [], report: null, stage: null, conflict: null }),
}));
