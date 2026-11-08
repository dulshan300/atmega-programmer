import clsx from "clsx";
import { EMPTY } from "@/lib/hooks";
import {
  Binary,
  Cable,
  Clock,
  FileCode2,
  FolderOpen,
  Hammer,
  Microchip,
  Plus,
  Settings2,
  Terminal,
  Upload,
  Workflow,
  type LucideIcon,
} from "lucide-react";
import { runCommand } from "@/commands";
import { EmptyState, IconButton, SectionTitle } from "@/components/ui";
import { diagnosticWorkspace, worstSeverity } from "@/lib/diagnostics";
import { basename, timeAgo } from "@/lib/format";
import { useApp, type WorkspaceId } from "@/store/app";
import { useBuild } from "@/store/build";
import { useProject } from "@/store/project";

function Item({
  icon: Icon,
  label,
  detail,
  workspace,
  depth = 0,
  tone,
}: {
  icon: LucideIcon;
  label: string;
  detail?: string;
  workspace: WorkspaceId;
  depth?: number;
  tone?: "error" | "warning";
}) {
  const current = useApp((s) => s.workspace);
  const setWorkspace = useApp((s) => s.setWorkspace);
  const active = current === workspace;
  return (
    <button
      type="button"
      onClick={() => setWorkspace(workspace)}
      aria-current={active ? "page" : undefined}
      className={clsx(
        "flex h-7 w-full items-center gap-2 rounded px-2 text-left text-[13px]",
        active ? "bg-accent/15 text-text" : "text-muted hover:bg-raised hover:text-text",
      )}
      style={{ paddingLeft: 8 + depth * 14 }}
    >
      <Icon size={14} className={active ? "text-accent" : undefined} aria-hidden />
      <span className="flex-1 truncate">{label}</span>
      {tone && (
        <span className={clsx("h-1.5 w-1.5 rounded-full", tone === "error" ? "bg-danger" : "bg-warn")} />
      )}
      {detail && <span className="font-mono text-[11px] text-faint">{detail}</span>}
    </button>
  );
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-2">
      <div className="px-3 py-1 text-[11px] font-semibold tracking-wider text-faint uppercase">{title}</div>
      <div className="px-1.5">{children}</div>
    </div>
  );
}

function tone(workspace: WorkspaceId): "error" | "warning" | undefined {
  return worstSeverity(useProject.getState().diagnostics.filter((d) => diagnosticWorkspace(d) === workspace));
}

function RecentList() {
  const recent = useApp((s) => s.settings?.recentProjects ?? EMPTY);
  const open = useProject((s) => s.open);
  if (!recent.length)
    return (
      <EmptyState icon={FolderOpen} title="No project open">
        Create a project or open an existing project folder.
      </EmptyState>
    );
  return (
    <Group title="Recent projects">
      {recent.slice(0, 8).map((r) => (
        <button
          key={r.path}
          type="button"
          onClick={() => void open(r.path)}
          title={r.path}
          className="flex w-full flex-col rounded px-2 py-1 text-left hover:bg-raised"
        >
          <span className="truncate text-[13px]">{r.name}</span>
          <span className="truncate text-[11px] text-faint">
            {r.mcu} · {timeAgo(r.openedAtMs)}
          </span>
        </button>
      ))}
    </Group>
  );
}

export function Explorer() {
  const project = useProject((s) => s.project);
  const root = useProject((s) => s.root);
  const device = useProject((s) => s.device);
  // Subscribe so section markers refresh after each validation.
  useProject((s) => s.diagnostics);
  const report = useBuild((s) => s.report);

  return (
    <div className="flex h-full flex-col bg-panel">
      <SectionTitle
        actions={
          <>
            <IconButton icon={Plus} label="New project" onClick={() => runCommand("project.new")} />
            <IconButton icon={FolderOpen} label="Open project" onClick={() => runCommand("project.open")} />
          </>
        }
      >
        Explorer
      </SectionTitle>
      <div className="min-h-0 flex-1 overflow-auto pb-2">
        {!project || !root ? (
          <RecentList />
        ) : (
          <>
            <div className="px-3 pb-2">
              <div className="truncate text-[13px] font-semibold" title={root}>
                {project.manifest.name}
              </div>
              <div className="truncate text-[11px] text-faint" title={root}>
                {basename(root)}/ · {device?.name ?? project.manifest.device.mcu} (
                {project.manifest.device.package})
              </div>
            </div>
            <Group title="Logic">
              <Item
                icon={Workflow}
                label="main.flow"
                workspace="visual"
                detail={`${project.graph.nodes.length}`}
                tone={tone("visual")}
              />
            </Group>
            <Group title="Configuration">
              <Item
                icon={Microchip}
                label="MCU & pins"
                workspace="pins"
                detail={`${project.pins.assignments.length}`}
                tone={tone("pins")}
              />
              <Item icon={Clock} label="Clock & peripherals" workspace="clock" tone={tone("clock")} />
              <Item icon={Binary} label="Fuses" workspace="fuses" tone={tone("fuses")} />
            </Group>
            <Group title="Firmware">
              <Item icon={FileCode2} label="Generated C" workspace="code" />
              <Item
                icon={Hammer}
                label="Build & memory"
                workspace="build"
                detail={report ? (report.success ? "ok" : "failed") : undefined}
              />
            </Group>
            <Group title="Hardware">
              <Item
                icon={Upload}
                label="Programmer"
                workspace="programmer"
                detail={project.manifest.programmer.programmer}
              />
              <Item icon={Terminal} label="Serial monitor" workspace="serial" />
              <Item icon={Cable} label="Toolchains" workspace="settings" />
            </Group>
          </>
        )}
      </div>
      <div className="border-t border-line p-1.5">
        <Item icon={Settings2} label="Settings" workspace="settings" />
      </div>
    </div>
  );
}
