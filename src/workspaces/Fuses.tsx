import clsx from "clsx";
import { Binary, Download, RotateCcw, ShieldAlert, Upload } from "lucide-react";
import { useState } from "react";
import { useSyncedState } from "@/lib/hooks";
import { Badge, Button, Callout, Card, Dialog, EmptyState, Input, Select } from "@/components/ui";
import { DiagnosticRow } from "@/components/shell/BottomPanel";
import {
  bitString,
  defaultFuses,
  extractField,
  fieldOption,
  hex2,
  insertField,
  parseByte,
} from "@/lib/fuses";
import type { FuseByteDef, FuseRisk, FuseWritePlan } from "@/lib/types";
import { useApp } from "@/store/app";
import { useProgrammer } from "@/store/programmer";
import { useProject } from "@/store/project";

const RISK_TONE: Record<FuseRisk, "warn" | "danger"> = {
  caution: "warn",
  danger: "danger",
  blocked: "danger",
};

function HexField({
  value,
  onCommit,
  label,
}: {
  value: number;
  onCommit: (v: number) => void;
  label: string;
}) {
  const [text, setText] = useSyncedState(hex2(value));
  const parsed = parseByte(text);
  const commit = () => (parsed === null ? setText(hex2(value)) : parsed !== value && onCommit(parsed));
  return (
    <Input
      aria-label={label}
      className="w-20 font-mono"
      value={text}
      aria-invalid={parsed === null}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}

function FuseByteCard({ byte, desired, device }: { byte: FuseByteDef; desired: number; device: string }) {
  const update = useProject((s) => s.update);
  const read = useProgrammer((s) => s.fuseRead);
  const readRoot = useProgrammer((s) => s.fuseReadRoot);
  const root = useProject((s) => s.root);
  const onDevice = read?.success && readRoot === root ? read.values[byte.id] : undefined;
  const setByte = (v: number) =>
    update((d) => void (d.fuses.desired[byte.id] = v), { coalesce: `fuse:${byte.id}` });
  const differs = onDevice !== undefined && (onDevice & byte.usedMask) !== (desired & byte.usedMask);

  return (
    <Card
      title={
        <span className="flex items-center gap-2">
          {byte.label} <span className="font-mono text-xs text-muted">({byte.id})</span>
        </span>
      }
      actions={
        <div className="flex items-center gap-2 text-xs">
          <span className="font-mono text-faint" title="Bits 7…0, active-low (0 = programmed). '-' = unused">
            {bitString(byte, desired)}
          </span>
          <HexField value={desired} onCommit={setByte} label={`${byte.label} desired value`} />
        </div>
      }
    >
      {onDevice !== undefined && (
        <div className={clsx("mb-2 text-xs", differs ? "text-warn" : "text-ok")}>
          On {device}: <span className="font-mono">{hex2(onDevice)}</span>{" "}
          {differs ? "— differs from desired" : "— matches desired"}
        </div>
      )}
      <table className="w-full text-xs">
        <tbody>
          {byte.fields.map((f) => {
            const raw = extractField(f, desired);
            const opt = fieldOption(f, raw);
            const devRaw = onDevice !== undefined ? extractField(f, onDevice) : undefined;
            return (
              <tr key={f.id} className="border-t border-line align-top first:border-t-0">
                <td className="w-28 py-1.5 pr-2">
                  <div className="font-mono font-semibold">{f.id}</div>
                  <div className="text-faint">{f.label}</div>
                </td>
                <td className="py-1.5">
                  <Select
                    aria-label={f.label}
                    value={String(raw)}
                    onChange={(v) => setByte(insertField(f, desired, Number(v)))}
                    options={[
                      ...f.options.map((o) => ({
                        value: String(o.value),
                        label: `${o.label}${o.risk === "blocked" ? " — not allowed" : o.risk === "danger" ? " ⚠" : ""}`,
                        disabled: o.risk === "blocked" && o.value !== raw,
                      })),
                      ...(opt ? [] : [{ value: String(raw), label: `Undefined value ${raw}` }]),
                    ]}
                  />
                  {opt?.risk && (
                    <div className="mt-1 flex items-start gap-1">
                      <Badge tone={RISK_TONE[opt.risk]}>{opt.risk}</Badge>
                      <span className="text-muted">{opt.warning}</span>
                    </div>
                  )}
                  {f.description && <div className="mt-1 text-faint">{f.description}</div>}
                </td>
                <td className="w-24 py-1.5 pl-2 text-right font-mono">
                  {devRaw !== undefined && (
                    <span
                      className={devRaw !== raw ? "text-warn" : "text-faint"}
                      title={fieldOption(f, devRaw)?.label}
                    >
                      chip: {devRaw}
                    </span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </Card>
  );
}

function PlanDialog() {
  const plan = useProgrammer((s) => s.plan);
  // A new plan remounts the body, so confirmations never carry over.
  return plan ? <PlanDialogBody key={plan.token} plan={plan} /> : null;
}

function PlanDialogBody({ plan }: { plan: FuseWritePlan }) {
  const discard = useProgrammer((s) => s.discardPlan);
  const write = useProgrammer((s) => s.writeFuses);
  const busy = useProgrammer((s) => s.busy);
  const [ack, setAck] = useState(false);
  const [typed, setTyped] = useState("");
  const danger = plan.warnings.some((w) => w.startsWith("DANGER"));
  const changed = plan.bytes.filter((b) => b.changed);
  const blocked = plan.blocked.length > 0;
  const canWrite =
    !blocked &&
    changed.length > 0 &&
    ack &&
    (!danger || typed.trim().toLowerCase() === plan.device.toLowerCase()) &&
    !busy;

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && discard()}
      title="Review fuse write"
      description="Fuse changes take effect immediately and some settings can stop the chip from accepting ISP programming."
      width={640}
      footer={
        <>
          <Button onClick={discard}>Cancel</Button>
          <Button
            variant="danger"
            icon={Upload}
            disabled={!canWrite}
            busy={busy === "fuses-write"}
            onClick={() => void write()}
          >
            Write fuses and verify
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <div className="text-xs text-muted">
          Target <b className="text-text">{plan.device}</b> (expected signature{" "}
          <span className="font-mono">{plan.signature}</span>) via{" "}
          <b className="text-text">{plan.programmer}</b>
        </div>
        <table className="w-full text-xs">
          <thead className="text-left text-muted">
            <tr>
              <th className="py-1 font-medium">Byte</th>
              <th className="font-medium">On chip</th>
              <th className="font-medium">New value</th>
              <th className="font-medium">Action</th>
            </tr>
          </thead>
          <tbody>
            {plan.bytes.map((b) => (
              <tr key={b.id} className="border-t border-line">
                <td className="py-1">{b.label}</td>
                <td className="font-mono">{b.current === null ? "not read" : hex2(b.current)}</td>
                <td className="font-mono">{hex2(b.desired)}</td>
                <td>{b.changed ? <Badge tone="warn">write</Badge> : <Badge>unchanged</Badge>}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {blocked && (
          <Callout tone="danger" title="Write blocked">
            <ul className="list-disc pl-4">
              {plan.blocked.map((b) => (
                <li key={b}>{b}</li>
              ))}
            </ul>
          </Callout>
        )}
        {plan.warnings.length > 0 && (
          <Callout tone={danger ? "danger" : "warn"} title="Warnings">
            <ul className="list-disc pl-4">
              {plan.warnings.map((w) => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          </Callout>
        )}
        <div>
          <div className="mb-1 text-xs font-medium text-muted">Command</div>
          <pre className="selectable overflow-x-auto rounded border border-line bg-panel-2 p-2 font-mono text-[11px] whitespace-pre-wrap">
            {plan.command}
          </pre>
        </div>
        {!blocked && changed.length > 0 && (
          <>
            <label className="flex items-start gap-2 text-xs">
              <input
                type="checkbox"
                className="mt-0.5 accent-[var(--accent)]"
                checked={ack}
                onChange={(e) => setAck(e.target.checked)}
              />
              <span>
                I have reviewed the values above. The target is powered, connected to an ISP programmer, and
                uses the clock source these fuses select.
              </span>
            </label>
            {danger && (
              <label className="flex flex-col gap-1 text-xs">
                <span>
                  This write includes <b className="text-danger">DANGER</b> settings. Type{" "}
                  <b className="font-mono">{plan.device}</b> to confirm.
                </span>
                <Input
                  value={typed}
                  onChange={(e) => setTyped(e.target.value)}
                  aria-label="Type the device name to confirm"
                />
              </label>
            )}
          </>
        )}
        {!blocked && changed.length === 0 && (
          <Callout tone="info">The chip already has these values; nothing will be written.</Callout>
        )}
      </div>
    </Dialog>
  );
}

export default function Fuses() {
  const project = useProject((s) => s.project);
  const device = useProject((s) => s.device);
  const root = useProject((s) => s.root);
  const update = useProject((s) => s.update);
  const diags = useProject((s) => s.diagnostics).filter(
    (d) => d.section === "fuses" || d.code.startsWith("fuses."),
  );
  const programmers = useApp((s) => s.programmers);
  const { busy, fuseRead, fuseReadRoot, fuseWrite, readFuses, planFuses } = useProgrammer();
  if (!project || !device) return <EmptyState icon={Binary} title="No project open" />;
  const prog = programmers.find((p) => p.id === project.manifest.programmer.programmer);
  const isp = prog?.kind === "isp";
  const read = fuseRead && fuseReadRoot === root ? fuseRead : null;

  return (
    <div className="h-full overflow-auto">
      <div className="flex max-w-5xl flex-col gap-4 p-4">
        <Callout tone="warn" title="Fuses change how the chip starts">
          Values here are the project's <b>desired</b> fuses. They are only written to a chip through “Review
          write…”, which reads the chip first, shows every change, and verifies by reading back. Settings that
          disable ISP (SPIEN, RSTDISBL, DWEN) are refused. {device.fuses.notes}
        </Callout>

        <div className="flex flex-wrap items-center gap-2">
          <Button
            icon={Download}
            disabled={!isp || busy !== null}
            busy={busy === "fuses-read"}
            onClick={() => void readFuses()}
          >
            Read from chip
          </Button>
          <Button
            variant="danger"
            icon={ShieldAlert}
            disabled={!isp || busy !== null}
            onClick={() =>
              void planFuses({ desired: project.fuses.desired, current: read?.success ? read.values : null })
            }
          >
            Review write…
          </Button>
          <Button
            variant="ghost"
            icon={RotateCcw}
            onClick={() => update((d) => void (d.fuses.desired = defaultFuses(device)))}
          >
            Factory defaults
          </Button>
          {read?.success && (
            <Button
              variant="ghost"
              onClick={() => update((d) => void (d.fuses.desired = { ...d.fuses.desired, ...read.values }))}
              title="Copy the values read from the chip into the desired values"
            >
              Use chip values as desired
            </Button>
          )}
          <div className="flex-1" />
          <span className="text-xs text-muted">
            Programmer: {prog?.label ?? project.manifest.programmer.programmer}
            {!isp && <span className="text-warn"> — fuses need an ISP programmer</span>}
          </span>
        </div>

        {read && !read.success && (
          <Callout tone="danger" title={read.message}>
            <ul className="list-disc pl-4">
              {read.hints.map((h) => (
                <li key={h}>{h}</li>
              ))}
            </ul>
          </Callout>
        )}
        {read?.success && <Callout tone="ok">Read from chip: {read.message}</Callout>}
        {fuseWrite && (
          <Callout
            tone={fuseWrite.success ? "ok" : "danger"}
            title={fuseWrite.success ? "Fuses written and verified" : "Fuse write not verified"}
          >
            {fuseWrite.message}
            {fuseWrite.hints.length > 0 && (
              <ul className="mt-1 list-disc pl-4">
                {fuseWrite.hints.map((h) => (
                  <li key={h}>{h}</li>
                ))}
              </ul>
            )}
          </Callout>
        )}

        {diags.length > 0 && (
          <Card title="Problems">
            <div className="-m-3">
              {diags.map((d, i) => (
                <DiagnosticRow key={i} d={d} />
              ))}
            </div>
          </Card>
        )}

        {device.fuses.bytes.map((b) => (
          <FuseByteCard
            key={b.id}
            byte={b}
            desired={project.fuses.desired[b.id] ?? b.defaultValue}
            device={device.name}
          />
        ))}
      </div>
      <PlanDialog />
    </div>
  );
}
