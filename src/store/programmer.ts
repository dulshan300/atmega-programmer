import { create } from "zustand";
import { api, events, toAppError } from "@/lib/ipc";
import type {
  FusePlanInput,
  FuseReadResponse,
  FuseWritePlan,
  FuseWriteResult,
  IdentifyResult,
  ProgrammerPreview,
  UploadResult,
} from "@/lib/types";
import { useApp } from "./app";
import { useProject } from "./project";
import type { LogLine } from "./build";

export type HardwareOp = "identify" | "upload" | "fuses-read" | "fuses-write";

interface ProgrammerState {
  busy: HardwareOp | null;
  lines: LogLine[];
  preview: ProgrammerPreview | null;
  previewError: string | null;
  identify: IdentifyResult | null;
  upload: UploadResult | null;
  fuseRead: FuseReadResponse | null;
  /** Project root the fuse read belongs to; a read is never reused for another project. */
  fuseReadRoot: string | null;
  plan: FuseWritePlan | null;
  planInput: FusePlanInput | null;
  fuseWrite: FuseWriteResult | null;

  refreshPreview(): Promise<void>;
  runIdentify(): Promise<void>;
  runUpload(): Promise<void>;
  readFuses(): Promise<void>;
  planFuses(input: FusePlanInput): Promise<FuseWritePlan | null>;
  writeFuses(): Promise<void>;
  discardPlan(): void;
  cancel(): Promise<void>;
  clearLog(): void;
}

const notify = (k: "info" | "success" | "warning" | "error", m: string) => useApp.getState().notify(k, m);

/** Saves pending edits (hardware operations use the saved project) and returns the root. */
async function savedRoot(): Promise<string | null> {
  const p = useProject.getState();
  if (!p.root) return null;
  if (p.isDirty() && !(await p.save())) return null;
  return p.root;
}

export const useProgrammer = create<ProgrammerState>()((set, get) => {
  async function run<T>(op: HardwareOp, fn: (root: string) => Promise<T>): Promise<T | null> {
    if (get().busy) return null;
    const root = await savedRoot();
    if (!root) return null;
    set({ busy: op, lines: [] });
    useApp.getState().showBottomTab("programmer");
    const unlisten = await events.programmerLine((l) =>
      set((s) => ({ lines: [...s.lines.slice(-2000), { stream: l.stream, text: l.text }] })),
    );
    try {
      return await fn(root);
    } catch (e) {
      const msg = toAppError(e).message;
      set((s) => ({ lines: [...s.lines, { stream: "stderr", text: msg }] }));
      notify("error", msg);
      return null;
    } finally {
      unlisten();
      set({ busy: null });
      void get().refreshPreview();
    }
  }

  return {
    busy: null,
    lines: [],
    preview: null,
    previewError: null,
    identify: null,
    upload: null,
    fuseRead: null,
    fuseReadRoot: null,
    plan: null,
    planInput: null,
    fuseWrite: null,

    async refreshPreview() {
      const root = useProject.getState().root;
      if (!root) return set({ preview: null, previewError: null });
      try {
        set({ preview: await api.programmerPreview(root), previewError: null });
      } catch (e) {
        set({ preview: null, previewError: toAppError(e).message });
      }
    },

    async runIdentify() {
      const r = await run("identify", api.programmerIdentify);
      if (!r) return;
      set({ identify: r });
      notify(r.success ? "success" : "error", r.message);
    },

    async runUpload() {
      const r = await run("upload", api.programmerUpload);
      if (!r) return;
      set({ upload: r });
      notify(r.success ? "success" : "error", r.message);
    },

    async readFuses() {
      const r = await run("fuses-read", api.fusesRead);
      if (!r) return;
      set({ fuseRead: r, fuseReadRoot: useProject.getState().root, plan: null, planInput: null });
      notify(r.success ? "success" : "error", r.success ? `Fuses read: ${r.message}` : r.message);
    },

    async planFuses(input) {
      const root = await savedRoot();
      if (!root) return null;
      try {
        const plan = await api.fusesPlan(root, input);
        set({ plan, planInput: input, fuseWrite: null });
        return plan;
      } catch (e) {
        notify("error", toAppError(e).message);
        return null;
      }
    },

    async writeFuses() {
      const { plan, planInput } = get();
      if (!plan || !planInput) return;
      const r = await run("fuses-write", (root) => api.fusesWrite(root, planInput, plan.token));
      set({ plan: null, planInput: null });
      if (!r) return;
      set({ fuseWrite: r });
      // The previous read is stale after any write attempt.
      set({ fuseRead: null, fuseReadRoot: null });
      notify(r.success ? "success" : "error", r.message);
    },

    discardPlan: () => set({ plan: null, planInput: null }),

    async cancel() {
      await api.cancelHardwareOperation().catch(() => {});
    },

    clearLog: () => set({ lines: [] }),
  };
});
