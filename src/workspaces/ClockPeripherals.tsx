import { Clock, Wand2 } from "lucide-react";
import { useEffect, useRef } from "react";
import { Badge, Button, Callout, Card, EmptyState, Field, IntInput, Select, Switch } from "@/components/ui";
import { DiagnosticRow } from "@/components/shell/BottomPanel";
import { extractField, insertField } from "@/lib/fuses";
import { formatHz } from "@/lib/format";
import type { DeviceDefinition, Diagnostic, Project } from "@/lib/types";
import { useApp } from "@/store/app";
import { useNav } from "@/store/nav";
import { useProject } from "@/store/project";

/** Mirrors the backend's UBRR search (normal and double speed, best error wins). */
export function computeBaud(fCpu: number, baud: number) {
  if (!baud || !fCpu) return null;
  let best: { ubrr: number; doubleSpeed: boolean; actual: number; error: number } | null = null;
  for (const [doubleSpeed, div] of [
    [false, 16],
    [true, 8],
  ] as const) {
    const ubrr = Math.round(fCpu / (div * baud) - 1);
    if (ubrr < 0 || ubrr > 4095) continue;
    const actual = fCpu / (div * (ubrr + 1));
    const error = (actual / baud - 1) * 100;
    if (!best || Math.abs(error) + 1e-9 < Math.abs(best.error)) best = { ubrr, doubleSpeed, actual, error };
  }
  return best;
}

const BAUDS = [
  1200, 2400, 4800, 9600, 14400, 19200, 28800, 38400, 57600, 76800, 115200, 230400, 250000, 500000, 1000000,
];

/** Desired-fuse changes that make CKSEL/CKDIV8 agree with the clock configuration. */
export function fusesForClock(
  device: DeviceDefinition,
  project: Project,
): { desired: Record<string, number>; note: string | null } {
  const desired = { ...project.fuses.desired };
  let note: string | null = null;
  for (const b of device.fuses.bytes) {
    for (const f of b.fields) {
      const cur = desired[b.id];
      if (cur === undefined) continue;
      if (f.id === "CKDIV8") desired[b.id] = insertField(f, cur, project.clock.ckdiv8 ? 0 : 1);
      if (f.id === "CKSEL") {
        const matches = f.options.filter(
          (o) =>
            o.risk !== "blocked" &&
            o.clockSource === project.clock.source &&
            (o.frequencyHz === undefined || o.frequencyHz === project.clock.sourceFrequencyHz),
        );
        const current = f.options.find((o) => o.value === extractField(f, cur));
        if (current && matches.includes(current)) continue;
        if (matches.length === 1) desired[b.id] = insertField(f, desired[b.id]!, matches[0]!.value);
        else if (matches.length > 1)
          note = `Several CKSEL values match “${project.clock.source}”. Choose the one for your crystal's frequency range on the Fuses page.`;
        else
          note = `No CKSEL value selects “${project.clock.source}” at ${formatHz(project.clock.sourceFrequencyHz)}.`;
      }
    }
  }
  return { desired, note };
}

function SectionDiagnostics({ section, codes }: { section: string; codes?: (c: string) => boolean }) {
  const diags = useProject((s) => s.diagnostics).filter(
    (d: Diagnostic) => d.section === section && (!codes || codes(d.code)),
  );
  if (!diags.length) return null;
  return (
    <div className="-mx-3 mt-3 -mb-3 border-t border-line py-1">
      {diags.map((d, i) => (
        <DiagnosticRow key={i} d={d} />
      ))}
    </div>
  );
}

function ClockCard() {
  const project = useProject((s) => s.project)!;
  const device = useProject((s) => s.device)!;
  const update = useProject((s) => s.update);
  const setWorkspace = useApp((s) => s.setWorkspace);
  const notify = useApp((s) => s.notify);
  const clock = project.clock;
  const src = device.clock.sources.find((s) => s.id === clock.source);
  const fcpu = clock.ckdiv8 ? clock.sourceFrequencyHz / 8 : clock.sourceFrequencyHz;

  const applyFuses = () => {
    const { desired, note } = fusesForClock(device, project);
    update((d) => void (d.fuses.desired = desired));
    notify(
      note ? "warning" : "success",
      note ??
        "Desired fuse values updated to match the clock. Nothing is written to the chip until you review and write fuses.",
    );
    if (note) setWorkspace("fuses");
  };

  return (
    <Card
      title="System clock"
      actions={
        <Button
          size="sm"
          icon={Wand2}
          onClick={applyFuses}
          title="Updates the desired CKSEL/CKDIV8 values in this project only"
        >
          Match desired fuses
        </Button>
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Field label="Clock source" className="col-span-2">
          <Select
            value={clock.source}
            onChange={(source) =>
              update((d) => {
                d.clock.source = source;
                const s = device.clock.sources.find((x) => x.id === source);
                if (s?.frequenciesHz?.length && !s.frequenciesHz.includes(d.clock.sourceFrequencyHz))
                  d.clock.sourceFrequencyHz = s.frequenciesHz[0]!;
                if (s?.minHz !== undefined && d.clock.sourceFrequencyHz < s.minHz)
                  d.clock.sourceFrequencyHz = s.minHz;
                if (s?.maxHz !== undefined && d.clock.sourceFrequencyHz > s.maxHz)
                  d.clock.sourceFrequencyHz = Math.min(s.maxHz, 16_000_000);
              })
            }
            options={device.clock.sources.map((s) => ({ value: s.id, label: s.label }))}
          />
        </Field>
        <Field
          label="Oscillator frequency (Hz)"
          hint={
            src?.minHz !== undefined
              ? `${formatHz(src.minHz)} – ${formatHz(src.maxHz ?? device.clock.maxFrequencyHz)}`
              : undefined
          }
        >
          {src?.frequenciesHz ? (
            <Select
              value={String(clock.sourceFrequencyHz)}
              onChange={(v) => update((d) => void (d.clock.sourceFrequencyHz = Number(v)))}
              options={src.frequenciesHz.map((f) => ({ value: String(f), label: formatHz(f) }))}
            />
          ) : (
            <IntInput
              value={clock.sourceFrequencyHz}
              min={src?.minHz ?? 1}
              max={src?.maxHz ?? device.clock.maxFrequencyHz}
              onCommit={(v) => update((d) => void (d.clock.sourceFrequencyHz = v))}
            />
          )}
        </Field>
        <Field label="CPU clock (F_CPU)">
          <div className="flex h-7 items-center gap-2 font-mono text-sm">
            {formatHz(fcpu)}
            {fcpu > device.clock.maxFrequencyHz && <Badge tone="danger">over max</Badge>}
          </div>
        </Field>
        {device.clock.hasClockPrescalerFuse && (
          <div className="col-span-2">
            <Switch
              checked={clock.ckdiv8}
              onChange={(ckdiv8) => update((d) => void (d.clock.ckdiv8 = ckdiv8))}
              label="Divide by 8 at startup (CKDIV8)"
              description="Factory default on the ATmega328P. Must match the CKDIV8 fuse actually programmed in the chip."
            />
          </div>
        )}
      </div>
      <div className="mt-3">
        <Callout tone="info">
          The firmware is compiled for F_CPU = {formatHz(fcpu)}. The chip runs from whatever its fuses select;
          delays and baud rates are only correct when the programmed fuses agree with this configuration.
        </Callout>
      </div>
      <SectionDiagnostics section="clock" />
    </Card>
  );
}

function UsartCard() {
  const project = useProject((s) => s.project)!;
  const device = useProject((s) => s.device)!;
  const update = useProject((s) => s.update);
  const u = project.peripherals.usart;
  const hw = device.peripherals.usart[0];
  const fcpu = project.clock.ckdiv8 ? project.clock.sourceFrequencyHz / 8 : project.clock.sourceFrequencyHz;
  const baud = computeBaud(fcpu, u.baud);
  const set = (patch: Partial<typeof u>) => update((d) => Object.assign(d.peripherals.usart, patch));
  const errTone = !baud
    ? "danger"
    : Math.abs(baud.error) > 2
      ? "danger"
      : Math.abs(baud.error) > 1
        ? "warn"
        : "ok";
  if (!hw) return <Card title="USART">This device has no USART.</Card>;
  return (
    <Card
      title={`${hw.id} (UART)`}
      actions={<Switch checked={u.enabled} onChange={(enabled) => set({ enabled })} label="Enabled" />}
    >
      <div
        className={
          u.enabled ? "grid grid-cols-4 gap-3" : "pointer-events-none grid grid-cols-4 gap-3 opacity-50"
        }
      >
        <Field label="Baud rate" className="col-span-2">
          <div className="flex gap-2">
            <Select
              value={BAUDS.includes(u.baud) ? String(u.baud) : "custom"}
              onChange={(v) => v !== "custom" && set({ baud: Number(v) })}
              options={[
                ...BAUDS.map((b) => ({ value: String(b), label: String(b) })),
                { value: "custom", label: "Custom…" },
              ]}
            />
            <IntInput
              value={u.baud}
              min={1}
              max={4_000_000}
              onCommit={(b) => set({ baud: b })}
              aria-label="Custom baud rate"
            />
          </div>
        </Field>
        <Field label="Data bits">
          <Select
            value={String(u.dataBits)}
            onChange={(v) => set({ dataBits: Number(v) })}
            options={[5, 6, 7, 8].map((n) => ({ value: String(n), label: String(n) }))}
          />
        </Field>
        <Field label="Stop bits">
          <Select
            value={String(u.stopBits)}
            onChange={(v) => set({ stopBits: Number(v) })}
            options={[1, 2].map((n) => ({ value: String(n), label: String(n) }))}
          />
        </Field>
        <Field label="Parity">
          <Select
            value={u.parity}
            onChange={(parity) => set({ parity })}
            options={[
              { value: "none", label: "None" },
              { value: "even", label: "Even" },
              { value: "odd", label: "Odd" },
            ]}
          />
        </Field>
        <div className="col-span-3 flex items-end gap-2 pb-1 text-xs">
          {baud ? (
            <>
              <span className="text-muted">UBRR</span> <span className="font-mono">{baud.ubrr}</span>
              <span className="text-muted">{baud.doubleSpeed ? "U2X on" : "U2X off"}</span>
              <span className="text-muted">actual</span>{" "}
              <span className="font-mono">{baud.actual.toFixed(0)}</span>
              <Badge tone={errTone}>
                {baud.error >= 0 ? "+" : ""}
                {baud.error.toFixed(2)} %
              </Badge>
            </>
          ) : (
            <Badge tone="danger">Not achievable at {formatHz(fcpu)}</Badge>
          )}
        </div>
      </div>
      <div className="mt-2 text-xs text-faint">
        Uses RXD {hw.rx} and TXD {hw.tx}. Errors above ±2 % are unreliable.
      </div>
      <SectionDiagnostics section="peripherals" codes={(c) => c.startsWith("usart")} />
    </Card>
  );
}

function AdcCard() {
  const project = useProject((s) => s.project)!;
  const device = useProject((s) => s.device)!;
  const update = useProject((s) => s.update);
  const a = project.peripherals.adc;
  const adc = device.peripherals.adc;
  const fcpu = project.clock.ckdiv8 ? project.clock.sourceFrequencyHz / 8 : project.clock.sourceFrequencyHz;
  const adcClock = a.prescaler ? fcpu / a.prescaler : 0;
  const set = (patch: Partial<typeof a>) => update((d) => Object.assign(d.peripherals.adc, patch));
  const pkg = project.manifest.device.package;
  return (
    <Card
      title={`ADC (${adc.resolutionBits}-bit)`}
      actions={<Switch checked={a.enabled} onChange={(enabled) => set({ enabled })} label="Enabled" />}
    >
      <div
        className={
          a.enabled ? "grid grid-cols-2 gap-3" : "pointer-events-none grid grid-cols-2 gap-3 opacity-50"
        }
      >
        <Field label="Reference">
          <Select
            value={a.reference}
            onChange={(reference) => set({ reference })}
            options={adc.references.map((r) => ({ value: r.id, label: r.label }))}
          />
        </Field>
        <Field
          label="Prescaler"
          hint={`ADC clock ${formatHz(Math.round(adcClock))} (50–${adc.maxClockHz / 1000} kHz for full resolution)`}
        >
          <Select
            value={String(a.prescaler)}
            onChange={(v) => set({ prescaler: Number(v) })}
            options={adc.prescalers.map((p) => ({ value: String(p), label: `÷ ${p}` }))}
          />
        </Field>
      </div>
      <div className="mt-2 text-xs text-faint">
        Channels in {pkg}:{" "}
        {adc.channels
          .filter((c) => !c.packages || c.packages.includes(pkg))
          .map((c) => `ADC${c.channel} (${c.pin})`)
          .join(", ")}
      </div>
      <SectionDiagnostics section="peripherals" codes={(c) => c.startsWith("adc")} />
    </Card>
  );
}

function OtherCard() {
  const project = useProject((s) => s.project)!;
  const device = useProject((s) => s.device)!;
  const update = useProject((s) => s.update);
  const p = device.peripherals;
  return (
    <Card title="Other peripherals">
      {device.jtagPins && (
        <div className="mb-3">
          <Switch
            checked={project.peripherals.disableJtagAtStartup}
            onChange={(v) => update((d) => void (d.peripherals.disableJtagAtStartup = v))}
            label="Disable JTAG at startup"
            description={`Writes JTD twice in MCUCSR so ${device.jtagPins.pins.join(", ")} work as GPIO while the ${device.jtagPins.fuseField} fuse stays programmed.`}
          />
        </div>
      )}
      <div className="text-xs text-muted">
        Available on {device.name} (configured through blocks where supported):{" "}
        {p.timers.map((t) => `${t.id} (${t.bits}-bit)`).join(", ")}; SPI; TWI (I²C); external interrupts{" "}
        {p.externalInterrupts.map((i) => `${i.id}/${i.pin}`).join(", ")}.
      </div>
      <SectionDiagnostics
        section="peripherals"
        codes={(c) => !c.startsWith("usart") && !c.startsWith("adc")}
      />
    </Card>
  );
}

export default function ClockPeripherals() {
  const hasProject = useProject((s) => s.project !== null && s.device !== null);
  const focus = useNav((s) => s.focusSection);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (focus)
      ref.current
        ?.querySelector(`[data-section="${focus.section}"]`)
        ?.scrollIntoView({ block: "start", behavior: "smooth" });
  }, [focus]);
  if (!hasProject) return <EmptyState icon={Clock} title="No project open" />;
  return (
    <div ref={ref} className="h-full overflow-auto">
      <div className="grid max-w-5xl gap-4 p-4 xl:grid-cols-2">
        <div data-section="clock" className="xl:col-span-2">
          <ClockCard />
        </div>
        <div data-section="peripherals" className="xl:col-span-2">
          <UsartCard />
        </div>
        <AdcCard />
        <OtherCard />
      </div>
    </div>
  );
}
