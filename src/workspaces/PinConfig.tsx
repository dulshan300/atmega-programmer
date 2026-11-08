import clsx from "clsx";
import { Microchip, Plus, Trash2 } from "lucide-react";
import {
  Badge,
  Button,
  Callout,
  Card,
  EmptyState,
  Field,
  IconButton,
  Input,
  Select,
  Switch,
} from "@/components/ui";
import { DiagnosticRow } from "@/components/shell/BottomPanel";
import { blockDef } from "@/lib/catalog";
import type { DeviceDefinition, PinAssignment, PinMode, Project } from "@/lib/types";
import { useProject } from "@/store/project";
import { identifierError } from "./visual/BlockInspector";

const POWER = /^(VCC|AVCC|GND|AGND|AREF)$/;

/** Pins reserved by the current configuration; mirrors the backend's `claimed_pins`. */
export function claimedPins(device: DeviceDefinition, project: Project): Map<string, string> {
  const m = new Map<string, string>();
  const u = device.peripherals.usart[0];
  if (project.peripherals.usart.enabled && u) {
    m.set(u.rx, `${u.id} RXD`);
    m.set(u.tx, `${u.id} TXD`);
  }
  if (project.clock.source === "crystal" || project.clock.source === "low_freq_crystal")
    for (const p of device.clock.crystalPins) m.set(p, "crystal oscillator");
  else if (project.clock.source === "external_clock")
    for (const p of device.clock.externalClockPins) m.set(p, "external clock input");
  if (device.resetPin) m.set(device.resetPin.pin, "RESET (RSTDISBL unprogrammed)");
  return m;
}

type PinState = "gpio" | "claimed" | "power" | "fixed" | "free";

function pinState(
  name: string,
  device: DeviceDefinition,
  project: Project,
  claims: Map<string, string>,
): PinState {
  if (POWER.test(name)) return "power";
  if (claims.has(name)) return "claimed";
  if (project.pins.assignments.some((a) => a.pin === name)) return "gpio";
  if (!device.pins.some((p) => p.name === name)) return "fixed";
  return "free";
}

const STATE_STYLE: Record<PinState, string> = {
  gpio: "bg-[var(--pin-gpio)] text-black border-transparent",
  claimed: "bg-[var(--pin-periph)] text-white border-transparent",
  power: "bg-[var(--pin-power)]/80 text-white border-transparent",
  fixed: "bg-raised text-muted border-line",
  free: "bg-panel-2 text-text border-line-strong hover:border-accent",
};

function PinBox({
  name,
  num,
  side,
}: {
  name: string;
  num: number;
  side: "left" | "right" | "top" | "bottom";
}) {
  const project = useProject((s) => s.project)!;
  const device = useProject((s) => s.device)!;
  const selection = useProject((s) => s.selection);
  const select = useProject((s) => s.select);
  const diags = useProject((s) => s.diagnostics);
  const claims = claimedPins(device, project);
  const state = pinState(name, device, project, claims);
  const a = project.pins.assignments.find((x) => x.pin === name);
  const selected = selection.kind === "pin" && selection.pin === name;
  const hasErr = diags.some((d) => d.pin === name && d.severity === "error");
  const vertical = side === "top" || side === "bottom";
  const selectable = state !== "power";
  const title = [
    `Pin ${num}: ${name}`,
    a && `${a.label} (${a.mode})`,
    claims.get(name) && `Used by ${claims.get(name)}`,
  ]
    .filter(Boolean)
    .join("\n");
  return (
    <button
      type="button"
      disabled={!selectable}
      onClick={() => select({ kind: "pin", pin: name })}
      title={title}
      aria-label={title}
      aria-pressed={selected}
      className={clsx(
        "relative flex items-center gap-1 rounded-sm border font-mono text-[10px] leading-none",
        vertical ? "h-16 w-5 flex-col justify-center [writing-mode:vertical-rl]" : "h-5 w-24 px-1",
        side === "right" && "flex-row-reverse",
        STATE_STYLE[state],
        selected && "ring-2 ring-accent ring-offset-1 ring-offset-bg",
        hasErr && "outline outline-2 outline-danger",
        !selectable && "cursor-default",
      )}
    >
      <span className="opacity-60">{num}</span>
      <span className="truncate font-semibold">{a ? a.label : name}</span>
    </button>
  );
}

function ChipDiagram() {
  const project = useProject((s) => s.project)!;
  const device = useProject((s) => s.device)!;
  const pkg = device.packages.find((p) => p.id === project.manifest.device.package);
  if (!pkg)
    return (
      <Callout tone="danger">
        Package {project.manifest.device.package} is not defined for {device.name}.
      </Callout>
    );
  const n = pkg.pins.length;
  const body = (
    <div className="flex flex-col items-center justify-center gap-1 rounded-md border border-line-strong bg-raised px-6 py-4 text-center">
      <div className="h-2 w-2 rounded-full bg-faint" aria-hidden />
      <div className="font-mono text-sm font-semibold">{device.name}</div>
      <div className="text-[11px] text-muted">{pkg.label}</div>
    </div>
  );
  if (pkg.layout === "dip") {
    const half = n / 2;
    const left = pkg.pins.slice(0, half).map((p, i) => ({ p, num: i + 1 }));
    const right = pkg.pins
      .slice(half)
      .map((p, i) => ({ p, num: half + i + 1 }))
      .reverse();
    return (
      <div className="flex items-stretch justify-center gap-1">
        <div className="flex flex-col gap-1">
          {left.map(({ p, num }) => (
            <PinBox key={num} name={p} num={num} side="left" />
          ))}
        </div>
        <div className="flex w-36">{body}</div>
        <div className="flex flex-col gap-1">
          {right.map(({ p, num }) => (
            <PinBox key={num} name={p} num={num} side="right" />
          ))}
        </div>
      </div>
    );
  }
  // QFP: pin 1 top of the left side, numbering counter-clockwise.
  const k = n / 4;
  const at = (from: number) => pkg.pins.slice(from, from + k).map((p, i) => ({ p, num: from + i + 1 }));
  const left = at(0);
  const bottom = at(k);
  const right = at(2 * k).reverse();
  const top = at(3 * k).reverse();
  return (
    <div className="inline-grid grid-cols-[auto_auto_auto] grid-rows-[auto_auto_auto] gap-1">
      <div />
      <div className="flex justify-center gap-1">
        {top.map(({ p, num }) => (
          <PinBox key={num} name={p} num={num} side="top" />
        ))}
      </div>
      <div />
      <div className="flex flex-col justify-center gap-1">
        {left.map(({ p, num }) => (
          <PinBox key={num} name={p} num={num} side="left" />
        ))}
      </div>
      <div className="flex">{body}</div>
      <div className="flex flex-col justify-center gap-1">
        {right.map(({ p, num }) => (
          <PinBox key={num} name={p} num={num} side="right" />
        ))}
      </div>
      <div />
      <div className="flex justify-center gap-1">
        {bottom.map(({ p, num }) => (
          <PinBox key={num} name={p} num={num} side="bottom" />
        ))}
      </div>
      <div />
    </div>
  );
}

function blocksUsingPin(project: Project, pin: string) {
  return project.graph.nodes.filter((n) =>
    blockDef(n.type)?.properties.some((p) => p.kind === "pin" && n.properties[p.id] === pin),
  );
}

export function PinInspector({ pin }: { pin: string }) {
  const project = useProject((s) => s.project);
  const device = useProject((s) => s.device);
  const update = useProject((s) => s.update);
  const diags = useProject((s) => s.diagnostics);
  if (!project || !device) return null;
  const def = device.pins.find((p) => p.name === pin);
  const pkg = device.packages.find((p) => p.id === project.manifest.device.package);
  const num = pkg ? pkg.pins.indexOf(pin) + 1 : 0;
  const claims = claimedPins(device, project);
  const idx = project.pins.assignments.findIndex((a) => a.pin === pin);
  const a = idx >= 0 ? project.pins.assignments[idx] : undefined;
  const users = blocksUsingPin(project, pin);
  const pinDiags = diags.filter((d) => d.pin === pin);
  const otherLabels = project.pins.assignments.filter((x) => x.pin !== pin).map((x) => x.label);
  const set = (patch: Partial<PinAssignment>) =>
    update(
      (d) =>
        Object.assign(
          d.pins.assignments.find((x) => x.pin === pin)!,
          patch,
        ),
      { coalesce: `pin:${pin}` },
    );

  return (
    <div className="flex flex-col gap-3 p-3">
      <div>
        <div className="flex items-center gap-2">
          <h3 className="font-mono text-sm font-semibold">{pin}</h3>
          {num > 0 && <Badge>pin {num}</Badge>}
        </div>
        {def ? (
          <div className="mt-1 text-xs text-muted">
            Port {def.port}, bit {def.bit}
            {def.functions.length > 0 && <> · alternate: {def.functions.join(", ")}</>}
          </div>
        ) : (
          <div className="mt-1 text-xs text-muted">Dedicated pin; not usable as GPIO.</div>
        )}
      </div>
      {claims.get(pin) && (
        <Callout tone="info" title="Reserved">
          Used by {claims.get(pin)}. Change the clock or peripheral settings to free it.
        </Callout>
      )}
      {def && !a && (
        <Button
          icon={Plus}
          disabled={claims.has(pin)}
          onClick={() =>
            update((d) => {
              let label = `${pin}`;
              for (let i = 2; d.pins.assignments.some((x) => x.label === label); i++) label = `${pin}_${i}`;
              d.pins.assignments.push({ pin, label, mode: "output", initialHigh: false });
            })
          }
        >
          Use as GPIO
        </Button>
      )}
      {a && (
        <>
          <Field
            label="Label (C identifier)"
            error={identifierError(a.label, otherLabels)}
            hint="Generated as IO_<label>_PORT / _DDR / _PIN / _BIT"
          >
            <Input
              value={a.label}
              className="font-mono"
              aria-invalid={!!identifierError(a.label, otherLabels)}
              onChange={(e) => set({ label: e.target.value })}
            />
          </Field>
          <Field label="Mode">
            <Select<PinMode>
              value={a.mode}
              onChange={(mode) => set({ mode })}
              options={[
                { value: "output", label: "Output" },
                { value: "input", label: "Input (high impedance)" },
                { value: "input_pullup", label: "Input with pull-up" },
              ]}
            />
          </Field>
          {a.mode === "output" && (
            <Switch
              checked={a.initialHigh}
              onChange={(initialHigh) => set({ initialHigh })}
              label="Initial level high"
              description="Level driven right after reset initialisation."
            />
          )}
          <Button
            variant="ghost"
            icon={Trash2}
            onClick={() => update((d) => void d.pins.assignments.splice(idx, 1))}
            title={users.length ? "Blocks using this pin will report an error" : undefined}
          >
            Remove GPIO assignment
          </Button>
        </>
      )}
      {users.length > 0 && (
        <div className="text-xs">
          <div className="mb-1 font-medium text-muted">Used by blocks</div>
          <ul className="flex flex-col gap-0.5">
            {users.map((n) => (
              <li key={n.id} className="font-mono text-[11px]">
                {blockDef(n.type)?.label} <span className="text-faint">({n.id})</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {pinDiags.length > 0 && (
        <div className="-mx-3 border-t border-line pt-2">
          {pinDiags.map((d, i) => (
            <DiagnosticRow key={i} d={d} />
          ))}
        </div>
      )}
    </div>
  );
}

export default function PinConfig() {
  const project = useProject((s) => s.project);
  const device = useProject((s) => s.device);
  const update = useProject((s) => s.update);
  const select = useProject((s) => s.select);
  if (!project || !device) return <EmptyState icon={Microchip} title="No project open" />;
  const claims = claimedPins(device, project);
  const jtag = device.jtagPins;

  return (
    <div className="h-full overflow-auto">
      <div className="flex flex-col gap-4 p-4">
        <div className="flex flex-wrap items-end gap-3">
          <Field label="Microcontroller">
            <Input
              readOnly
              value={device.name}
              className="w-48"
              title="The MCU is chosen when the project is created."
            />
          </Field>
          <Field label="Package">
            <Select
              className="w-40"
              value={project.manifest.device.package}
              onChange={(pkg) => update((d) => void (d.manifest.device.package = pkg))}
              options={device.packages.map((p) => ({ value: p.id, label: p.label }))}
            />
          </Field>
          <div className="flex flex-wrap gap-3 pb-1 text-[11px] text-muted">
            {(
              [
                ["gpio", "GPIO assigned"],
                ["claimed", "Peripheral / clock"],
                ["power", "Power"],
                ["fixed", "Dedicated"],
                ["free", "Free"],
              ] as const
            ).map(([s, l]) => (
              <span key={s} className="flex items-center gap-1">
                <span className={clsx("inline-block h-3 w-3 rounded-sm border", STATE_STYLE[s])} /> {l}
              </span>
            ))}
          </div>
        </div>

        {jtag && (
          <Callout tone="warn" title="JTAG pins">
            {jtag.pins.join(", ")} carry JTAG while the {jtag.fuseField} fuse is programmed (factory default).
            To use them as GPIO, enable “Disable JTAG at startup” in Clock &amp; Peripherals or unprogram{" "}
            {jtag.fuseField}.
          </Callout>
        )}

        <div className="flex justify-center overflow-auto rounded-md border border-line bg-panel p-6">
          <ChipDiagram />
        </div>

        <Card title={`GPIO assignments (${project.pins.assignments.length})`}>
          {project.pins.assignments.length === 0 ? (
            <div className="text-xs text-muted">
              Click a free pin in the diagram to assign it as an input or output.
            </div>
          ) : (
            <table className="w-full text-xs">
              <thead className="text-left text-muted">
                <tr>
                  <th className="py-1 font-medium">Pin</th>
                  <th className="font-medium">Label</th>
                  <th className="font-medium">Mode</th>
                  <th className="font-medium">Initial</th>
                  <th className="font-medium">Blocks</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {[...project.pins.assignments]
                  .sort((x, y) => x.pin.localeCompare(y.pin))
                  .map((a) => (
                    <tr
                      key={a.pin}
                      className="cursor-pointer border-t border-line hover:bg-raised"
                      onClick={() => select({ kind: "pin", pin: a.pin })}
                    >
                      <td className="py-1 font-mono">{a.pin}</td>
                      <td className="font-mono">{a.label}</td>
                      <td>{a.mode.replace("_", " ")}</td>
                      <td>{a.mode === "output" ? (a.initialHigh ? "high" : "low") : "—"}</td>
                      <td>{blocksUsingPin(project, a.pin).length}</td>
                      <td className="text-right">
                        {claims.has(a.pin) && <Badge tone="danger">conflict: {claims.get(a.pin)}</Badge>}
                        <IconButton
                          icon={Trash2}
                          label={`Remove ${a.pin}`}
                          onClick={(e) => {
                            e.stopPropagation();
                            update(
                              (d) =>
                                void (d.pins.assignments = d.pins.assignments.filter((x) => x.pin !== a.pin)),
                            );
                          }}
                        />
                      </td>
                    </tr>
                  ))}
              </tbody>
            </table>
          )}
        </Card>
      </div>
    </div>
  );
}
