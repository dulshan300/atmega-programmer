import { Hammer, Square } from "lucide-react";
import { Badge, Button, Callout, Card, EmptyState, Field, Meter, Select } from "@/components/ui";
import { DiagnosticRow, LogView } from "@/components/shell/BottomPanel";
import { formatDuration } from "@/lib/format";
import type { BuildStage, Project } from "@/lib/types";
import { useApp } from "@/store/app";
import { useBuild } from "@/store/build";
import { useProject } from "@/store/project";

const STAGES: { id: BuildStage; label: string }[] = [
  { id: "validate", label: "Validate" },
  { id: "toolchain", label: "Toolchain" },
  { id: "generate", label: "Generate C" },
  { id: "compile", label: "Compile & link" },
  { id: "size", label: "Memory" },
  { id: "hex", label: "HEX" },
  { id: "done", label: "Done" },
];

function Pipeline() {
  const { running, stage, report } = useBuild();
  const idx = STAGES.findIndex((s) => s.id === stage);
  return (
    <ol className="flex flex-wrap items-center gap-1 text-xs" aria-label="Build stages">
      {STAGES.map((s, i) => {
        let tone: "neutral" | "accent" | "ok" | "danger" = "neutral";
        if (idx >= 0 && i < idx) tone = "ok";
        if (i === idx) tone = running ? "accent" : report?.success ? "ok" : report ? "danger" : "neutral";
        return (
          <li key={s.id} className="flex items-center gap-1">
            <Badge tone={tone}>{s.label}</Badge>
            {i < STAGES.length - 1 && <span className="text-faint">›</span>}
          </li>
        );
      })}
    </ol>
  );
}

export default function BuildMemory() {
  const project = useProject((s) => s.project);
  const update = useProject((s) => s.update);
  const toolchain = useApp((s) => s.toolchain);
  const { running, report, lines, conflict, start, cancel } = useBuild();
  if (!project) return <EmptyState icon={Hammer} title="No project open" />;
  const mem = report?.memory;
  const tc = project.manifest.toolchain;
  const setTc = (patch: Partial<Project["manifest"]["toolchain"]>) =>
    update((d) => Object.assign(d.manifest.toolchain, patch));

  return (
    <div className="h-full overflow-auto">
      <div className="flex max-w-6xl flex-col gap-4 p-4">
        <Card
          title="Build"
          actions={
            running ? (
              <Button size="sm" icon={Square} onClick={() => void cancel()}>
                Cancel
              </Button>
            ) : (
              <Button size="sm" variant="primary" icon={Hammer} onClick={() => void start()}>
                Build
              </Button>
            )
          }
        >
          <div className="flex flex-col gap-3">
            <div className="flex flex-wrap items-end gap-3">
              <Field label="Toolchain profile">
                <Select
                  className="w-56"
                  value={tc.profile}
                  onChange={(profile) => setTc({ profile })}
                  options={[
                    {
                      value: "auto",
                      label: `Auto-detect${toolchain?.gcc ? ` (${toolchain.gcc.detectedProfile.replace("_", " ")})` : ""}`,
                    },
                    { value: "winavr_legacy", label: "WinAVR / legacy AVR-GCC (C99)" },
                    { value: "modern", label: "Modern AVR-GCC (C11)" },
                  ]}
                />
              </Field>
              <Field label="Optimization">
                <Select
                  className="w-40"
                  value={tc.optimization}
                  onChange={(optimization) => setTc({ optimization })}
                  options={[
                    { value: "s", label: "-Os (size)" },
                    { value: "1", label: "-O1" },
                    { value: "2", label: "-O2" },
                    { value: "3", label: "-O3" },
                    { value: "0", label: "-O0 (debug; delays inaccurate)" },
                  ]}
                />
              </Field>
              <div className="pb-1 text-xs text-muted">
                {report?.toolchain ??
                  (toolchain?.gcc ? `avr-gcc ${toolchain.gcc.version}` : "avr-gcc not found")}
              </div>
            </div>
            <Pipeline />
            {conflict && (
              <Callout tone="warn" title="Generated files were edited by hand">
                <div>{conflict}</div>
                <Button
                  className="mt-2"
                  size="sm"
                  variant="danger"
                  onClick={() => void start({ force: true })}
                >
                  Overwrite my edits and build
                </Button>
              </Callout>
            )}
            {report && !conflict && (
              <Callout
                tone={report.success ? "ok" : report.cancelled ? "info" : "danger"}
                title={report.message}
              >
                Finished in {formatDuration(report.durationMs)}
                {report.artifacts && (
                  <div className="selectable mt-1 font-mono break-all">{report.artifacts.hex}</div>
                )}
                {report.logPath && (
                  <div className="selectable font-mono break-all">Log: {report.logPath}</div>
                )}
              </Callout>
            )}
          </div>
        </Card>

        {report && report.diagnostics.length > 0 && (
          <Card title={`Diagnostics (${report.diagnostics.length})`}>
            <div className="-m-3">
              {report.diagnostics.map((d, i) => (
                <DiagnosticRow key={i} d={d} />
              ))}
            </div>
          </Card>
        )}

        {mem ? (
          <div className="grid gap-4 lg:grid-cols-2">
            <Card title="Memory usage">
              <div className="flex flex-col gap-3">
                <Meter
                  label="Flash (program)"
                  value={mem.flash.used}
                  max={mem.flash.total}
                  detail={`${mem.flash.used} / ${mem.flash.total} B`}
                />
                <Meter
                  label="SRAM (static: .data + .bss)"
                  value={mem.sramStatic.used}
                  max={mem.sramStatic.total}
                  detail={`${mem.sramStatic.used} / ${mem.sramStatic.total} B`}
                />
                <Meter
                  label="EEPROM"
                  value={mem.eeprom.used}
                  max={mem.eeprom.total}
                  detail={`${mem.eeprom.used} / ${mem.eeprom.total} B`}
                />
                {mem.notes.map((n) => (
                  <div key={n} className="text-xs text-muted">
                    {n}
                  </div>
                ))}
                <div className="text-xs text-faint">
                  Stack and heap use at run time are not included in the static SRAM figure.
                </div>
              </div>
            </Card>
            <Card title="Sections">
              <table className="w-full font-mono text-xs">
                <thead className="text-left text-muted">
                  <tr>
                    <th className="py-1 font-medium">Section</th>
                    <th className="text-right font-medium">Size</th>
                    <th className="text-right font-medium">Address</th>
                  </tr>
                </thead>
                <tbody>
                  {mem.sections.map((s) => (
                    <tr key={s.name} className="border-t border-line">
                      <td className="py-0.5">{s.name}</td>
                      <td className="text-right">{s.size}</td>
                      <td className="text-right text-muted">0x{s.address.toString(16).padStart(4, "0")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </Card>
            {mem.symbols.length > 0 && (
              <Card title="Largest symbols" className="lg:col-span-2">
                <table className="w-full font-mono text-xs">
                  <thead className="text-left text-muted">
                    <tr>
                      <th className="py-1 font-medium">Symbol</th>
                      <th className="font-medium">Region</th>
                      <th className="text-right font-medium">Size</th>
                    </tr>
                  </thead>
                  <tbody>
                    {[...mem.symbols]
                      .sort((a, b) => b.size - a.size)
                      .slice(0, 25)
                      .map((s) => (
                        <tr key={`${s.name}@${s.address}`} className="border-t border-line">
                          <td className="py-0.5">{s.name}</td>
                          <td className="text-muted">{s.region}</td>
                          <td className="text-right">{s.size}</td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </Card>
            )}
          </div>
        ) : (
          !running && (
            <div className="text-xs text-muted">
              Memory figures come from the linked ELF file and appear after a successful build.
            </div>
          )
        )}

        <Card title="Output">
          <div className="-m-3 h-72">
            <LogView lines={lines} empty="No build has run yet." />
          </div>
        </Card>
      </div>
    </div>
  );
}
