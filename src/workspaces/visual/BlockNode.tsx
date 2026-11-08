import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import clsx from "clsx";
import { AlertCircle, AlertTriangle } from "lucide-react";
import { memo } from "react";
import { blockDef, PORT_COLORS, type BlockDef, type PortDef } from "@/lib/catalog";
import type { Diagnostic } from "@/lib/types";

export type BlockNodeData = {
  blockType: string;
  properties: Record<string, string | number | boolean | null>;
  diagnostics: Diagnostic[];
  pinLabels: Record<string, string>;
  /** Input ports that already have a connection (their fallback property is ignored). */
  connectedInputs: string[];
};

export type BlockFlowNode = Node<BlockNodeData, "block">;

const CATEGORY_COLORS: Record<string, string> = {
  flow: "#f59e0b",
  io: "#2dd4bf",
  timing: "#38bdf8",
  analog: "#a3e635",
  comm: "#c084fc",
  data: "#60a5fa",
  events: "#f87171",
};

function formatValue(def: BlockDef, data: BlockNodeData, id: string): string | null {
  const p = def.properties.find((x) => x.id === id);
  if (!p) return null;
  const v = data.properties[id] ?? p.default;
  if (v === "" || v === null || v === undefined) return p.optional ? null : "—";
  switch (p.kind) {
    case "bool":
      return v ? (p.trueLabel ?? "yes") : (p.falseLabel ?? "no");
    case "enum":
      return p.options?.find((o) => o.value === v)?.label ?? String(v);
    case "pin": {
      const label = data.pinLabels[String(v)];
      return label ? `${v} (${label})` : String(v);
    }
    case "string":
      return `“${String(v)}”`;
    case "adc_channel":
      return `ADC${v}`;
    default:
      return String(v);
  }
}

function PortRow({ port, side, connected }: { port: PortDef; side: "in" | "out"; connected?: boolean }) {
  const exec = port.type === "exec";
  return (
    <div
      className={clsx(
        "relative flex h-5 items-center text-[11px]",
        side === "out" ? "justify-end pr-3" : "pl-3",
      )}
    >
      <Handle
        id={port.id}
        type={side === "in" ? "target" : "source"}
        position={side === "in" ? Position.Left : Position.Right}
        className={clsx(exec && "exec")}
        style={{ background: PORT_COLORS[port.type], borderRadius: exec ? 2 : "50%" }}
        aria-label={`${side === "in" ? "Input" : "Output"} ${port.label || port.id} (${port.type})`}
      />
      <span className={clsx("text-muted", connected && "text-text")}>{port.label}</span>
    </div>
  );
}

function BlockNodeImpl({ data, selected }: NodeProps<BlockFlowNode>) {
  const def = blockDef(data.blockType);
  if (!def) {
    return (
      <div className="rounded border border-danger bg-panel px-3 py-2 text-xs text-danger">
        Unknown block “{data.blockType}”
      </div>
    );
  }
  const errors = data.diagnostics.filter((d) => d.severity === "error");
  const warnings = data.diagnostics.filter((d) => d.severity === "warning");
  const rows = Math.max(def.inputs.length, def.outputs.length);
  const summary = def.properties
    .filter((p) => {
      // Hide properties that are overridden by a connected input.
      const input = def.inputs.find((i) => i.fallbackProperty === p.id);
      return !(input && data.connectedInputs.includes(input.id));
    })
    .map((p) => [p.label, formatValue(def, data, p.id)] as const)
    .filter(([, v]) => v !== null);

  return (
    <div
      className={clsx(
        "min-w-[170px] max-w-[260px] rounded-md border bg-panel text-text shadow-lg",
        selected
          ? "border-accent ring-1 ring-accent"
          : errors.length
            ? "border-danger"
            : warnings.length
              ? "border-warn"
              : "border-line-strong",
      )}
      title={data.diagnostics.map((d) => d.message).join("\n") || def.description}
    >
      <div
        className="flex items-center gap-1.5 rounded-t-md border-b border-line px-2.5 py-1.5"
        style={{ boxShadow: `inset 3px 0 0 ${CATEGORY_COLORS[def.category] ?? "var(--accent)"}` }}
      >
        <span className="flex-1 truncate text-xs font-semibold">{def.label}</span>
        {errors.length > 0 && (
          <AlertCircle size={13} className="text-danger" aria-label={`${errors.length} errors`} />
        )}
        {!errors.length && warnings.length > 0 && (
          <AlertTriangle size={13} className="text-warn" aria-label={`${warnings.length} warnings`} />
        )}
      </div>
      {rows > 0 && (
        <div className="grid grid-cols-2 gap-x-2 py-1">
          <div>
            {def.inputs.map((p) => (
              <PortRow key={p.id} port={p} side="in" connected={data.connectedInputs.includes(p.id)} />
            ))}
          </div>
          <div>
            {def.outputs.map((p) => (
              <PortRow key={p.id} port={p} side="out" />
            ))}
          </div>
        </div>
      )}
      {summary.length > 0 && (
        <div className="border-t border-line px-2.5 py-1 font-mono text-[11px] text-muted">
          {summary.map(([label, v]) => (
            <div key={label} className="truncate">
              <span className="text-faint">{label}: </span>
              {v}
            </div>
          ))}
        </div>
      )}
      {errors[0] && (
        <div className="border-t border-danger/40 bg-danger/10 px-2.5 py-1 text-[11px] text-danger">
          {errors[0].message}
        </div>
      )}
    </div>
  );
}

export const BlockNode = memo(BlockNodeImpl);
