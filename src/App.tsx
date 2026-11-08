import clsx from "clsx";
import { AlertOctagon } from "lucide-react";
import { lazy, Suspense, useEffect } from "react";
import { Group, Panel, Separator, useDefaultLayout } from "react-resizable-panels";
import { COMMANDS, WORKSPACES, matchesShortcut } from "@/commands";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { BottomPanel } from "@/components/shell/BottomPanel";
import { Explorer } from "@/components/shell/Explorer";
import { CommandPalette, PromptDialog, Toasts } from "@/components/shell/Overlays";
import { StatusBar } from "@/components/shell/StatusBar";
import { TopBar } from "@/components/shell/TopBar";
import { EmptyState } from "@/components/ui";
import { isTauri } from "@/lib/ipc";
import { useApp, type WorkspaceId } from "@/store/app";
import { useProject } from "@/store/project";
import { Inspector } from "@/workspaces/Inspector";
import { NewProjectDialog } from "@/workspaces/NewProjectDialog";
import { Welcome } from "@/workspaces/Welcome";

const VisualEditor = lazy(() => import("@/workspaces/visual/VisualEditor"));
const PinConfig = lazy(() => import("@/workspaces/PinConfig"));
const ClockPeripherals = lazy(() => import("@/workspaces/ClockPeripherals"));
const Fuses = lazy(() => import("@/workspaces/Fuses"));
const CodeView = lazy(() => import("@/workspaces/CodeView"));
const BuildMemory = lazy(() => import("@/workspaces/BuildMemory"));
const Programmer = lazy(() => import("@/workspaces/Programmer"));
const SerialMonitor = lazy(() => import("@/workspaces/SerialMonitor"));
const Settings = lazy(() => import("@/workspaces/Settings"));

function WorkspaceView({ id }: { id: WorkspaceId }) {
  switch (id) {
    case "welcome":
      return <Welcome />;
    case "visual":
      return <VisualEditor />;
    case "pins":
      return <PinConfig />;
    case "clock":
      return <ClockPeripherals />;
    case "fuses":
      return <Fuses />;
    case "code":
      return <CodeView />;
    case "build":
      return <BuildMemory />;
    case "programmer":
      return <Programmer />;
    case "serial":
      return <SerialMonitor />;
    case "settings":
      return <Settings />;
  }
}

function WorkspaceTabs() {
  const current = useApp((s) => s.workspace);
  const setWorkspace = useApp((s) => s.setWorkspace);
  const hasProject = useProject((s) => s.project !== null);
  return (
    <div
      className="flex h-9 shrink-0 items-end gap-px overflow-x-auto border-b border-line bg-panel px-1"
      role="tablist"
      aria-label="Workspaces"
    >
      {WORKSPACES.filter((w) => (hasProject ? w.id !== "welcome" : !w.needsProject)).map((w) => (
        <button
          key={w.id}
          type="button"
          role="tab"
          aria-selected={current === w.id}
          onClick={() => setWorkspace(w.id)}
          className={clsx(
            "h-8 shrink-0 rounded-t border border-b-0 px-3 text-xs",
            current === w.id
              ? "border-line bg-bg text-text"
              : "border-transparent text-muted hover:text-text",
          )}
        >
          {w.label}
        </button>
      ))}
    </div>
  );
}

function useGlobalShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Let text fields and the code editor handle their own undo/redo.
      const t = e.target as HTMLElement | null;
      const editing = t?.closest("input, textarea, select, .monaco-editor, [contenteditable=true]");
      for (const c of COMMANDS) {
        if (!c.shortcut || !matchesShortcut(e, c.shortcut)) continue;
        if (editing && (c.id === "edit.undo" || c.id === "edit.redo")) return;
        e.preventDefault();
        if (c.enabled()) void c.run();
        return;
      }
      // Ctrl+Y as an alternative redo on Windows/Linux.
      if (!editing && e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "y") {
        e.preventDefault();
        useProject.getState().redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

function useCloseGuard() {
  useEffect(() => {
    if (!isTauri()) {
      const onBefore = (e: BeforeUnloadEvent) => {
        if (useProject.getState().isDirty()) e.preventDefault();
      };
      window.addEventListener("beforeunload", onBefore);
      return () => window.removeEventListener("beforeunload", onBefore);
    }
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/window").then(async ({ getCurrentWindow }) => {
      const win = getCurrentWindow();
      unlisten = await win.onCloseRequested(async (event) => {
        if (!useProject.getState().isDirty()) return;
        event.preventDefault();
        if (await useProject.getState().close()) await win.destroy();
      });
    });
    return () => unlisten?.();
  }, []);
}

export default function App() {
  const init = useApp((s) => s.init);
  const workspace = useApp((s) => s.workspace);
  const backendError = useApp((s) => s.backendError);
  const showExplorer = useApp((s) => s.showExplorer);
  const showInspector = useApp((s) => s.showInspector);
  const showBottom = useApp((s) => s.showBottom);
  const hasProject = useProject((s) => s.project !== null);
  const setWorkspace = useApp((s) => s.setWorkspace);

  useEffect(() => void init(), [init]);
  useGlobalShortcuts();
  useCloseGuard();

  // Never show a project workspace without a project.
  useEffect(() => {
    const def = WORKSPACES.find((w) => w.id === workspace);
    if (!hasProject && def?.needsProject) setWorkspace("welcome");
  }, [hasProject, workspace, setWorkspace]);

  const outer = useDefaultLayout({
    id: "avsv-main",
    panelIds: ["explorer", "center", "inspector"].filter((id) =>
      id === "explorer" ? showExplorer : id === "inspector" ? showInspector : true,
    ),
  });
  const inner = useDefaultLayout({
    id: "avsv-center",
    panelIds: showBottom ? ["workspace", "bottom"] : ["workspace"],
  });

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      {backendError && (
        <div
          className="flex items-center gap-2 border-b border-danger/40 bg-danger/10 px-3 py-1.5 text-xs"
          role="alert"
        >
          <AlertOctagon size={14} className="text-danger" />
          {backendError}
        </div>
      )}
      <Group orientation="horizontal" className="min-h-0 flex-1" {...outer}>
        {showExplorer && (
          <>
            <Panel id="explorer" defaultSize="18%" minSize="160px" maxSize="35%">
              <Explorer />
            </Panel>
            <Separator className="resize-handle w-px" />
          </>
        )}
        <Panel id="center" minSize="30%">
          <Group orientation="vertical" {...inner}>
            <Panel id="workspace" minSize="25%">
              <div className="flex h-full flex-col">
                <WorkspaceTabs />
                <main className="min-h-0 flex-1 overflow-hidden bg-bg">
                  <Suspense fallback={<EmptyState icon={AlertOctagon} title="Loading…" />}>
                    <ErrorBoundary resetKey={workspace}>
                      <WorkspaceView id={workspace} />
                    </ErrorBoundary>
                  </Suspense>
                </main>
              </div>
            </Panel>
            {showBottom && (
              <>
                <Separator className="resize-handle h-px" />
                <Panel id="bottom" defaultSize="28%" minSize="80px" maxSize="70%">
                  <BottomPanel />
                </Panel>
              </>
            )}
          </Group>
        </Panel>
        {showInspector && (
          <>
            <Separator className="resize-handle w-px" />
            <Panel id="inspector" defaultSize="22%" minSize="220px" maxSize="40%">
              <Inspector />
            </Panel>
          </>
        )}
      </Group>
      <StatusBar />
      <NewProjectDialog />
      <CommandPalette />
      <PromptDialog />
      <Toasts />
    </div>
  );
}
