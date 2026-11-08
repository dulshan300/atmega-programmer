// Typed wrappers around the Tauri commands. All privileged work happens in
// the Rust backend; the frontend only calls these functions.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { LoadedProjectSchema, describeIssues } from "./schemas";
import type {
  AppInfo,
  AppSettings,
  BuildEvent,
  BuildReport,
  DeviceDefinition,
  Diagnostic,
  FusePlanInput,
  FuseReadResponse,
  FuseWritePlan,
  FuseWriteResult,
  IdentifyResult,
  LoadedProject,
  PortInfo,
  PreviewResponse,
  ProgrammerDef,
  ProgrammerPreview,
  Project,
  SerialConfig,
  SerialPayload,
  SerialStatus,
  Stream,
  Template,
  ToolchainSettings,
  ToolchainStatus,
  UploadResult,
} from "./types";

export type ErrorKind =
  | "io"
  | "invalidProject"
  | "invalidRequest"
  | "unknownDevice"
  | "toolchain"
  | "process"
  | "serial"
  | "busy"
  | "safety"
  | "conflict"
  | "unavailable"
  | "unknown";

/** Error raised by any backend call. `kind` mirrors the Rust `CoreError`. */
export class AppError extends Error {
  constructor(
    public readonly kind: ErrorKind,
    message: string,
  ) {
    super(message);
    this.name = "AppError";
  }
}

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function toAppError(e: unknown): AppError {
  if (e instanceof AppError) return e;
  if (e && typeof e === "object" && "kind" in e && "message" in e) {
    const { kind, message } = e as { kind: unknown; message: unknown };
    return new AppError(String(kind) as ErrorKind, String(message));
  }
  if (e instanceof Error) return new AppError("unknown", e.message);
  return new AppError("unknown", String(e));
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new AppError(
      "unavailable",
      "The desktop backend is not available (running outside the app window).",
    );
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

function checkLoaded(raw: unknown): LoadedProject {
  const r = LoadedProjectSchema.safeParse(raw);
  if (!r.success)
    throw new AppError("invalidProject", `Project data is malformed: ${describeIssues(r.error)}`);
  return r.data;
}

export const api = {
  // system
  appInfo: () => call<AppInfo>("app_info"),
  getDevice: (id: string) => call<DeviceDefinition>("get_device", { id }),
  getSettings: () => call<{ settings: AppSettings; warning: string | null }>("get_settings"),
  saveToolchainSettings: (toolchain: ToolchainSettings) =>
    call<AppSettings>("save_toolchain_settings", { toolchain }),
  setTheme: (theme: "dark" | "light") => call<void>("set_theme", { theme }),
  detectToolchains: () => call<ToolchainStatus>("detect_toolchains"),
  removeRecent: (path: string) => call<AppSettings>("remove_recent", { path }),
  saveTextFile: (path: string, contents: string) => call<void>("save_text_file", { path, contents }),

  // projects
  createProject: async (request: {
    parentDir: string;
    name: string;
    mcu: string;
    package: string;
    template: Template;
  }) => checkLoaded(await call<unknown>("create_project", { request })),
  openProject: async (path: string) => checkLoaded(await call<unknown>("open_project", { path })),
  saveProject: (root: string, project: Project) => call<void>("save_project", { root, project }),
  duplicateProject: async (root: string, newName: string) =>
    checkLoaded(await call<unknown>("duplicate_project", { root, newName })),
  exportProject: (root: string, destination: string) => call<void>("export_project", { root, destination }),
  validateProject: (project: Project) => call<Diagnostic[]>("validate_project", { project }),
  generatePreview: (project: Project) => call<PreviewResponse>("generate_preview", { project }),

  // build
  startBuild: (root: string, forceRegenerate: boolean) =>
    call<BuildReport>("start_build", { root, forceRegenerate }),
  cancelBuild: (root: string) => call<boolean>("cancel_build", { root }),

  // programmer
  listProgrammers: () => call<ProgrammerDef[]>("list_programmers"),
  programmerPreview: (root: string) => call<ProgrammerPreview>("programmer_preview", { root }),
  programmerIdentify: (root: string) => call<IdentifyResult>("programmer_identify", { root }),
  programmerUpload: (root: string) => call<UploadResult>("programmer_upload", { root }),
  fusesRead: (root: string) => call<FuseReadResponse>("fuses_read", { root }),
  fusesPlan: (root: string, input: FusePlanInput) => call<FuseWritePlan>("fuses_plan", { root, input }),
  fusesWrite: (root: string, input: FusePlanInput, token: string) =>
    call<FuseWriteResult>("fuses_write", { root, input, token }),
  cancelHardwareOperation: () => call<boolean>("cancel_hardware_operation"),

  // serial
  serialListPorts: () => call<PortInfo[]>("serial_list_ports"),
  serialOpen: (config: SerialConfig) => call<void>("serial_open", { config }),
  serialWrite: (payload: SerialPayload) => call<number>("serial_write", { payload }),
  serialClose: () => call<boolean>("serial_close"),
};

async function on<T>(event: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<T>(event, (e) => handler(e.payload));
}

export const events = {
  build: (h: (e: BuildEvent) => void) => on<BuildEvent>("build://event", h),
  programmerLine: (h: (e: { stream: Stream; text: string }) => void) => on("programmer://line", h),
  serialData: (h: (e: { bytes: number[]; timestampMs: number }) => void) => on("serial://data", h),
  serialStatus: (h: (e: SerialStatus) => void) => on<SerialStatus>("serial://status", h),
};

/** Native folder/file pickers. Return null when cancelled. */
export const dialogs = {
  async pickFolder(title: string): Promise<string | null> {
    if (!isTauri()) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const r = await open({ directory: true, multiple: false, title });
    return typeof r === "string" ? r : null;
  },
  async pickFile(title: string, extensions?: string[]): Promise<string | null> {
    if (!isTauri()) return null;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const filters = extensions ? [{ name: extensions.join(", "), extensions }] : undefined;
    const r = await open({ directory: false, multiple: false, title, filters });
    return typeof r === "string" ? r : null;
  },
  async saveFile(title: string, defaultPath: string, extensions: string[]): Promise<string | null> {
    if (!isTauri()) return null;
    const { save } = await import("@tauri-apps/plugin-dialog");
    return save({ title, defaultPath, filters: [{ name: extensions.join(", "), extensions }] });
  },
  async confirm(message: string, title: string, kind: "warning" | "info" = "warning"): Promise<boolean> {
    if (!isTauri()) return window.confirm(message);
    const { ask } = await import("@tauri-apps/plugin-dialog");
    return ask(message, { title, kind });
  },
};
