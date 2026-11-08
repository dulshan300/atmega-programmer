import { RefreshCw, ScanSearch, Square, Upload } from "lucide-react";
import { useEffect } from "react";
import { Badge, Button, Callout, Card, EmptyState, Field, Input, Select } from "@/components/ui";
import { LogView } from "@/components/shell/BottomPanel";
import { timeAgo } from "@/lib/format";
import type { ProgrammerConfig } from "@/lib/types";
import { useApp } from "@/store/app";
import { useBuild } from "@/store/build";
import { useProgrammer } from "@/store/programmer";
import { useProject } from "@/store/project";
import { useSerial } from "@/store/serial";

function Hints({ hints }: { hints: string[] }) {
  if (!hints.length) return null;
  return (
    <ul className="mt-1 list-disc pl-4">
      {hints.map((h) => (
        <li key={h}>{h}</li>
      ))}
    </ul>
  );
}

export default function Programmer() {
  const project = useProject((s) => s.project);
  const saved = useProject((s) => s.saved);
  const update = useProject((s) => s.update);
  const programmers = useApp((s) => s.programmers);
  const ports = useSerial((s) => s.ports);
  const refreshPorts = useSerial((s) => s.refreshPorts);
  const building = useBuild((s) => s.running);
  const startBuild = useBuild((s) => s.start);
  const p = useProgrammer();

  useEffect(() => {
    void refreshPorts();
  }, [refreshPorts]);
  // The preview reflects the saved project; refresh it after each save.
  useEffect(() => {
    void useProgrammer.getState().refreshPreview();
  }, [saved]);

  if (!project) return <EmptyState icon={Upload} title="No project open" />;
  const cfg = project.manifest.programmer;
  const def = programmers.find((x) => x.id === cfg.programmer);
  const set = (patch: Partial<ProgrammerConfig>) =>
    update((d) => Object.assign(d.manifest.programmer, patch), { coalesce: "programmer" });
  const serial = def?.connection === "serial";
  const idle = p.busy === null && !building;

  return (
    <div className="h-full overflow-auto">
      <div className="flex max-w-5xl flex-col gap-4 p-4">
        <Card title="Programmer">
          <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
            <Field label="Type" className="col-span-2">
              <Select
                value={cfg.programmer}
                onChange={(id) => {
                  const nd = programmers.find((x) => x.id === id);
                  set({
                    programmer: id,
                    port: nd?.connection === "usb" ? "usb" : cfg.port === "usb" ? "" : cfg.port,
                    baud: nd?.defaultBaud ?? null,
                  });
                }}
                options={programmers.map((x) => ({
                  value: x.id,
                  label: `${x.label} (${x.kind === "isp" ? "ISP" : "bootloader"})`,
                }))}
              />
            </Field>
            {serial ? (
              <Field label="Serial port">
                <div className="flex gap-1">
                  <Select
                    value={cfg.port}
                    onChange={(port) => set({ port })}
                    options={[
                      { value: "", label: "Select a port…" },
                      ...ports.map((x) => ({
                        value: x.name,
                        label: `${x.name}${x.product ? ` — ${x.product}` : ""}${x.inUseBy ? ` (in use: ${x.inUseBy})` : ""}`,
                      })),
                      ...(cfg.port && !ports.some((x) => x.name === cfg.port)
                        ? [{ value: cfg.port, label: `${cfg.port} (not present)` }]
                        : []),
                    ]}
                  />
                  <Button icon={RefreshCw} aria-label="Refresh ports" onClick={() => void refreshPorts()} />
                </div>
              </Field>
            ) : (
              <Field label="Connection">
                <Input readOnly value="USB" />
              </Field>
            )}
            {serial ? (
              <Field label="Baud rate">
                <Select
                  value={String(cfg.baud ?? def?.defaultBaud ?? 115200)}
                  onChange={(v) => set({ baud: Number(v) })}
                  options={[19200, 38400, 57600, 115200].map((b) => ({ value: String(b), label: String(b) }))}
                />
              </Field>
            ) : (
              <Field label="Bit clock -B (µs)" hint="Blank = programmer default. Use 32+ for 1 MHz targets.">
                <Input
                  value={cfg.bitclock ?? ""}
                  placeholder="default"
                  onChange={(e) =>
                    set({ bitclock: e.target.value.trim() === "" ? null : e.target.value.trim() })
                  }
                />
              </Field>
            )}
          </div>
          {def && <p className="mt-2 text-xs text-muted">{def.notes}</p>}
        </Card>

        <Card title="Commands">
          {p.previewError ? (
            <Callout tone="danger">{p.previewError}</Callout>
          ) : p.preview ? (
            <div className="flex flex-col gap-2 text-xs">
              <div>
                <div className="mb-0.5 text-muted">Detect signature</div>
                <pre className="selectable overflow-x-auto rounded border border-line bg-panel-2 p-2 font-mono text-[11px] whitespace-pre-wrap">
                  {p.preview.identify}
                </pre>
              </div>
              <div>
                <div className="mb-0.5 text-muted">
                  Upload{" "}
                  {p.preview.hexExists ? (
                    <Badge tone="ok">
                      firmware.hex built {p.preview.hexModifiedMs ? timeAgo(p.preview.hexModifiedMs) : ""}
                    </Badge>
                  ) : (
                    <Badge tone="warn">no firmware built yet</Badge>
                  )}
                </div>
                <pre className="selectable overflow-x-auto rounded border border-line bg-panel-2 p-2 font-mono text-[11px] whitespace-pre-wrap">
                  {p.preview.upload ?? "Build the project to create firmware.hex"}
                </pre>
              </div>
              {useProject.getState().isDirty() && (
                <div className="text-warn">Unsaved changes are saved before any device operation.</div>
              )}
            </div>
          ) : (
            <div className="text-xs text-muted">Loading…</div>
          )}
          <div className="mt-3 flex flex-wrap gap-2">
            <Button
              icon={ScanSearch}
              disabled={!idle}
              busy={p.busy === "identify"}
              onClick={() => void p.runIdentify()}
            >
              Detect device
            </Button>
            <Button
              variant="primary"
              icon={Upload}
              disabled={!idle}
              busy={p.busy === "upload" || building}
              onClick={async () => {
                const r = await startBuild();
                if (r?.success) await p.runUpload();
              }}
            >
              Build &amp; upload
            </Button>
            <Button
              icon={Upload}
              disabled={!idle || !p.preview?.hexExists}
              onClick={() => void p.runUpload()}
            >
              Upload last build
            </Button>
            {p.busy && (
              <Button variant="danger" icon={Square} onClick={() => void p.cancel()}>
                Cancel
              </Button>
            )}
          </div>
        </Card>

        {p.identify && (
          <Callout
            tone={p.identify.success ? "ok" : "danger"}
            title={p.identify.success ? "Device detected" : "Device not detected"}
          >
            {p.identify.message}
            <Hints hints={p.identify.hints} />
          </Callout>
        )}
        {p.upload && (
          <Callout
            tone={p.upload.success ? "ok" : "danger"}
            title={p.upload.success ? "Upload verified" : "Upload failed"}
          >
            {p.upload.message}
            <Hints hints={p.upload.hints} />
          </Callout>
        )}

        <Card title="AVRDUDE output">
          <div className="-m-3 h-64">
            <LogView lines={p.lines} empty="Output from the last programmer operation appears here." />
          </div>
        </Card>
      </div>
    </div>
  );
}
