import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, toAppError } from "@/lib/ipc";
import type { AppInfo, AppSettings, ProgrammerDef, ToolchainSettings, ToolchainStatus } from "@/lib/types";

export type WorkspaceId =
  "welcome" | "visual" | "pins" | "clock" | "fuses" | "code" | "build" | "programmer" | "serial" | "settings";

export type BottomTab = "output" | "problems" | "programmer" | "serial";

export interface Toast {
  id: number;
  kind: "info" | "success" | "warning" | "error";
  message: string;
}

interface Layout {
  showExplorer: boolean;
  showInspector: boolean;
  showBottom: boolean;
  bottomTab: BottomTab;
}

interface AppState extends Layout {
  info: AppInfo | null;
  settings: AppSettings | null;
  settingsWarning: string | null;
  toolchain: ToolchainStatus | null;
  detecting: boolean;
  programmers: ProgrammerDef[];
  backendError: string | null;
  theme: "dark" | "light";
  workspace: WorkspaceId;
  paletteOpen: boolean;
  newProjectOpen: boolean;
  toasts: Toast[];

  init(): Promise<void>;
  setWorkspace(w: WorkspaceId): void;
  setTheme(t: "dark" | "light"): void;
  toggle(panel: "showExplorer" | "showInspector" | "showBottom"): void;
  showBottomTab(tab: BottomTab): void;
  setPaletteOpen(open: boolean): void;
  setNewProjectOpen(open: boolean): void;
  detectToolchains(): Promise<void>;
  saveToolchainSettings(t: ToolchainSettings): Promise<void>;
  removeRecent(path: string): Promise<void>;
  setSettings(s: AppSettings): void;
  notify(kind: Toast["kind"], message: string): void;
  dismiss(id: number): void;
}

let toastSeq = 0;

export function applyTheme(theme: "dark" | "light") {
  document.documentElement.dataset.theme = theme;
  document.documentElement.classList.toggle("dark", theme === "dark");
}

export const useApp = create<AppState>()(
  persist(
    (set, get) => ({
      info: null,
      settings: null,
      settingsWarning: null,
      toolchain: null,
      detecting: false,
      programmers: [],
      backendError: null,
      theme: "dark",
      workspace: "welcome",
      paletteOpen: false,
      newProjectOpen: false,
      toasts: [],
      showExplorer: true,
      showInspector: true,
      showBottom: true,
      bottomTab: "output",

      async init() {
        applyTheme(get().theme);
        try {
          const [info, s, programmers] = await Promise.all([
            api.appInfo(),
            api.getSettings(),
            api.listProgrammers(),
          ]);
          set({
            info,
            settings: s.settings,
            settingsWarning: s.warning,
            programmers,
            theme: s.settings.theme,
          });
          applyTheme(s.settings.theme);
          if (s.warning) get().notify("warning", s.warning);
          void get().detectToolchains();
        } catch (e) {
          set({ backendError: toAppError(e).message });
        }
      },

      setWorkspace: (workspace) => set({ workspace }),

      setTheme(theme) {
        set({ theme });
        applyTheme(theme);
        api.setTheme(theme).catch(() => {});
      },

      toggle: (panel) => set((s) => ({ [panel]: !s[panel] }) as Partial<AppState>),
      showBottomTab: (bottomTab) => set({ bottomTab, showBottom: true }),
      setPaletteOpen: (paletteOpen) => set({ paletteOpen }),
      setNewProjectOpen: (newProjectOpen) => set({ newProjectOpen }),

      async detectToolchains() {
        set({ detecting: true });
        try {
          set({ toolchain: await api.detectToolchains() });
        } catch (e) {
          get().notify("error", `Toolchain detection failed: ${toAppError(e).message}`);
        } finally {
          set({ detecting: false });
        }
      },

      async saveToolchainSettings(t) {
        const settings = await api.saveToolchainSettings(t);
        set({ settings });
        await get().detectToolchains();
      },

      async removeRecent(path) {
        try {
          set({ settings: await api.removeRecent(path) });
        } catch (e) {
          get().notify("error", toAppError(e).message);
        }
      },

      setSettings: (settings) => set({ settings }),

      notify(kind, message) {
        const id = ++toastSeq;
        set((s) => ({ toasts: [...s.toasts.slice(-4), { id, kind, message }] }));
        if (kind !== "error") setTimeout(() => get().dismiss(id), 6000);
      },

      dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
    }),
    {
      name: "avsv.layout",
      version: 1,
      partialize: (s): Layout & { theme: "dark" | "light" } => ({
        showExplorer: s.showExplorer,
        showInspector: s.showInspector,
        showBottom: s.showBottom,
        bottomTab: s.bottomTab,
        theme: s.theme,
      }),
    },
  ),
);
