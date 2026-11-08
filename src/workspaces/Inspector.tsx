import { Info } from "lucide-react";
import { EmptyState, SectionTitle } from "@/components/ui";
import { formatBytes, formatHz } from "@/lib/format";
import { useApp } from "@/store/app";
import { useProject } from "@/store/project";
import { PinInspector } from "./PinConfig";
import { BlockInspector, VariablesPanel } from "./visual/BlockInspector";

function ProjectSummary() {
  const project = useProject((s) => s.project);
  const device = useProject((s) => s.device);
  const root = useProject((s) => s.root);
  if (!project || !device) return <EmptyState icon={Info} title="Nothing selected" />;
  const fcpu = project.clock.ckdiv8 ? project.clock.sourceFrequencyHz / 8 : project.clock.sourceFrequencyHz;
  const rows: [string, string][] = [
    ["Device", device.name],
    ["Package", project.manifest.device.package],
    ["Signature", device.signature.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join(" ")],
    ["Flash", formatBytes(device.memory.flashBytes)],
    ["SRAM", formatBytes(device.memory.sramBytes)],
    ["EEPROM", formatBytes(device.memory.eepromBytes)],
    [
      "Clock source",
      device.clock.sources.find((s) => s.id === project.clock.source)?.label ?? project.clock.source,
    ],
    ["F_CPU", formatHz(fcpu)],
    ["Toolchain profile", project.manifest.toolchain.profile],
    ["Optimization", `-O${project.manifest.toolchain.optimization}`],
    ["Programmer", project.manifest.programmer.programmer],
  ];
  return (
    <div className="p-3">
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1.5 text-xs">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-muted">{k}</dt>
            <dd className="selectable truncate font-mono" title={v}>
              {v}
            </dd>
          </div>
        ))}
      </dl>
      <div className="selectable mt-3 text-[11px] break-all text-faint">{root}</div>
      <div className="mt-3 text-[11px] text-faint">Device data: {device.provenance.datasheet}</div>
    </div>
  );
}

export function Inspector() {
  const workspace = useApp((s) => s.workspace);
  const selection = useProject((s) => s.selection);
  const hasProject = useProject((s) => s.project !== null);

  let title = "Properties";
  let body: React.ReactNode;
  if (!hasProject)
    body = (
      <EmptyState icon={Info} title="No project open">
        Properties of the selected item appear here.
      </EmptyState>
    );
  else if (workspace === "visual" && selection.kind === "node") {
    title = "Block";
    body = <BlockInspector nodeId={selection.id} />;
  } else if (workspace === "visual") {
    title = "Program";
    body = (
      <>
        <VariablesPanel />
        <div className="border-t border-line">
          <SectionTitle>Project</SectionTitle>
          <ProjectSummary />
        </div>
      </>
    );
  } else if (workspace === "pins" && selection.kind === "pin") {
    title = "Pin";
    body = <PinInspector pin={selection.pin} />;
  } else {
    title = "Project";
    body = <ProjectSummary />;
  }

  return (
    <aside className="flex h-full flex-col bg-panel" aria-label="Properties">
      <SectionTitle>{title}</SectionTitle>
      <div className="min-h-0 flex-1 overflow-auto">{body}</div>
    </aside>
  );
}
