import { Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Badge, Button, Field, IconButton, Input, IntInput, Select, Switch } from "@/components/ui";
import { DiagnosticRow } from "@/components/shell/BottomPanel";
import { blockDef, type PropertyDef } from "@/lib/catalog";
import type { GraphNode, Project, VarType } from "@/lib/types";
import { useApp } from "@/store/app";
import { useProject } from "@/store/project";
import { askText } from "@/store/prompt";
import { renameVariable, variableUsage } from "./graph";

const VAR_TYPES: { value: VarType; label: string; min: number; max: number }[] = [
  { value: "bool", label: "bool", min: 0, max: 1 },
  { value: "uint8", label: "uint8_t", min: 0, max: 255 },
  { value: "int8", label: "int8_t", min: -128, max: 127 },
  { value: "uint16", label: "uint16_t", min: 0, max: 65535 },
  { value: "int16", label: "int16_t", min: -32768, max: 32767 },
  { value: "uint32", label: "uint32_t", min: 0, max: 4294967295 },
  { value: "int32", label: "int32_t", min: -2147483648, max: 2147483647 },
];

const C_KEYWORDS = new Set(
  "auto break case char const continue default do double else enum extern float for goto if inline int long register restrict return short signed sizeof static struct switch typedef union unsigned void volatile while bool true false main setup loop".split(
    " ",
  ),
);

export function identifierError(name: string, taken: string[] = []): string | null {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) return "Use letters, digits and _ (not starting with a digit)";
  if (name.length > 32) return "At most 32 characters";
  if (name.startsWith("__")) return "Names starting with __ are reserved";
  if (C_KEYWORDS.has(name)) return `“${name}” is a reserved word`;
  if (taken.includes(name)) return "Name already in use";
  return null;
}

function PropertyEditor({ node, prop, project }: { node: GraphNode; prop: PropertyDef; project: Project }) {
  const update = useProject((s) => s.update);
  const device = useProject((s) => s.device);
  const setWorkspace = useApp((s) => s.setWorkspace);
  const value = node.properties[prop.id] ?? prop.default;
  const set = (v: string | number | boolean) =>
    update(
      (d) => {
        const n = d.graph.nodes.find((x) => x.id === node.id);
        if (n) n.properties[prop.id] = v;
      },
      { coalesce: `prop:${node.id}:${prop.id}` },
    );
  const pkg = project.manifest.device.package;

  switch (prop.kind) {
    case "bool":
      return (
        <Switch
          checked={!!value}
          onChange={set}
          label={value ? (prop.trueLabel ?? "Yes") : (prop.falseLabel ?? "No")}
        />
      );
    case "int":
      return <IntInput value={Number(value) || 0} min={prop.min} max={prop.max} onCommit={set} />;
    case "string":
      return <Input value={String(value)} maxLength={prop.maxLength} onChange={(e) => set(e.target.value)} />;
    case "identifier": {
      const s = String(value);
      const err = identifierError(s);
      return (
        <>
          <Input value={s} aria-invalid={!!err} className="font-mono" onChange={(e) => set(e.target.value)} />
          {err && <span className="text-xs text-danger">{err}</span>}
        </>
      );
    }
    case "enum":
      return (
        <Select
          value={String(value)}
          onChange={set}
          options={(prop.options ?? []).map((o) => ({ value: o.value, label: o.label }))}
        />
      );
    case "pin": {
      const assigned = project.pins.assignments;
      const cur = String(value);
      const options = [
        { value: "", label: "Select a configured pin…" },
        ...assigned.map((a) => ({
          value: a.pin,
          label: `${a.pin} — ${a.label} (${a.mode.replace("_", " ")})`,
        })),
      ];
      if (cur && !assigned.some((a) => a.pin === cur))
        options.push({ value: cur, label: `${cur} (not configured)` });
      return (
        <>
          <Select value={cur} onChange={set} options={options} />
          <button
            type="button"
            className="self-start text-xs text-accent hover:underline"
            onClick={() => setWorkspace("pins")}
          >
            {assigned.length ? "Configure pins…" : "No pins configured yet — open Pin Configuration"}
          </button>
        </>
      );
    }
    case "variable": {
      const vars = project.graph.variables;
      return (
        <Select
          value={String(value)}
          onChange={set}
          options={[
            { value: "", label: prop.optional ? "(none)" : "Select a variable…" },
            ...vars.map((v) => ({ value: v.name, label: `${v.name} : ${v.type}` })),
          ]}
        />
      );
    }
    case "adc_channel": {
      const chans = (device?.peripherals.adc.channels ?? []).filter(
        (c) => !c.packages || c.packages.includes(pkg),
      );
      return (
        <Select
          value={String(value)}
          onChange={(v) => set(Number(v))}
          options={chans.map((c) => ({ value: String(c.channel), label: `ADC${c.channel} (${c.pin})` }))}
        />
      );
    }
    case "ext_interrupt":
      return (
        <Select
          value={String(value)}
          onChange={set}
          options={(device?.peripherals.externalInterrupts ?? []).map((i) => ({
            value: i.id,
            label: `${i.id} (${i.pin})`,
          }))}
        />
      );
  }
}

export function BlockInspector({ nodeId }: { nodeId: string }) {
  const project = useProject((s) => s.project);
  const diagnostics = useProject((s) => s.diagnostics);
  const update = useProject((s) => s.update);
  const select = useProject((s) => s.select);
  const node = project?.graph.nodes.find((n) => n.id === nodeId);
  if (!project || !node) return null;
  const def = blockDef(node.type);
  if (!def) return <div className="p-3 text-xs text-danger">Unknown block type “{node.type}”.</div>;
  const connected = new Set(
    project.graph.edges.filter((e) => e.target === node.id).map((e) => e.targetHandle),
  );
  const nodeDiags = diagnostics.filter((d) => d.nodeId === node.id);

  return (
    <div className="flex flex-col gap-3 p-3">
      <div>
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-semibold">{def.label}</h3>
          {def.type !== "program_start" && (
            <IconButton
              icon={Trash2}
              label="Delete block"
              onClick={() => {
                update((d) => {
                  d.graph.nodes = d.graph.nodes.filter((n) => n.id !== node.id);
                  d.graph.edges = d.graph.edges.filter((e) => e.source !== node.id && e.target !== node.id);
                });
                select({ kind: "none" });
              }}
            />
          )}
        </div>
        <p className="mt-1 text-xs text-muted">{def.description}</p>
        <div className="mt-1 font-mono text-[11px] text-faint">id: {node.id}</div>
      </div>
      {def.properties.length === 0 && <div className="text-xs text-faint">This block has no properties.</div>}
      {def.properties.map((p) => {
        const input = def.inputs.find((i) => i.fallbackProperty === p.id);
        const overridden = input && connected.has(input.id);
        return (
          <Field
            key={p.id}
            label={p.label}
            hint={overridden ? `Ignored: the “${input.label}” input is connected.` : undefined}
          >
            <div
              className={
                overridden ? "pointer-events-none flex flex-col gap-1 opacity-50" : "flex flex-col gap-1"
              }
            >
              <PropertyEditor node={node} prop={p} project={project} />
            </div>
          </Field>
        );
      })}
      {nodeDiags.length > 0 && (
        <div className="-mx-3 border-t border-line pt-2">
          <div className="px-3 pb-1 text-[11px] font-semibold tracking-wider text-muted uppercase">
            Problems
          </div>
          {nodeDiags.map((d, i) => (
            <DiagnosticRow key={i} d={d} />
          ))}
        </div>
      )}
    </div>
  );
}

export function VariablesPanel() {
  const project = useProject((s) => s.project);
  const update = useProject((s) => s.update);
  const notify = useApp((s) => s.notify);
  const [adding, setAdding] = useState(false);
  if (!project) return null;
  const vars = project.graph.variables;
  const names = vars.map((v) => v.name);

  const add = async () => {
    setAdding(true);
    const name = await askText({
      title: "New variable",
      label: "Name (C identifier)",
      initial: `var${vars.length + 1}`,
      validate: (v) => identifierError(v, names),
    });
    setAdding(false);
    if (name) update((d) => void d.graph.variables.push({ name, type: "uint8", initial: 0 }));
  };

  const rename = async (from: string) => {
    const to = await askText({
      title: "Rename variable",
      label: "New name",
      initial: from,
      validate: (v) => (v === from ? null : identifierError(v, names)),
    });
    if (to && to !== from) update((d) => renameVariable(d, from, to));
  };

  return (
    <div className="flex flex-col gap-2 p-3">
      <div className="flex items-center justify-between">
        <h3 className="text-[11px] font-semibold tracking-wider text-muted uppercase">Variables</h3>
        <Button size="sm" icon={Plus} onClick={() => void add()} disabled={adding}>
          Add
        </Button>
      </div>
      {vars.length === 0 && (
        <div className="text-xs text-faint">
          Global variables can be read and written by blocks. None defined yet.
        </div>
      )}
      {vars.map((v, i) => {
        const t = VAR_TYPES.find((x) => x.value === v.type)!;
        const uses = variableUsage(project, v.name);
        return (
          <div key={v.name} className="rounded border border-line bg-panel-2 p-2">
            <div className="mb-1.5 flex items-center gap-2">
              <button
                type="button"
                className="font-mono text-[13px] hover:text-accent"
                title="Rename"
                onClick={() => void rename(v.name)}
              >
                {v.name}
              </button>
              <Badge>{uses} uses</Badge>
              <div className="flex-1" />
              <IconButton
                icon={Trash2}
                label={`Delete ${v.name}`}
                onClick={() => {
                  if (uses > 0)
                    notify(
                      "warning",
                      `“${v.name}” is still used by ${uses} block(s); those blocks will report an error.`,
                    );
                  update((d) => void d.graph.variables.splice(i, 1));
                }}
              />
            </div>
            <div className="grid grid-cols-2 gap-2">
              <Select
                aria-label="Type"
                value={v.type}
                onChange={(type) =>
                  update((d) => {
                    const x = d.graph.variables[i]!;
                    const r = VAR_TYPES.find((y) => y.value === type)!;
                    x.type = type;
                    x.initial = Math.min(r.max, Math.max(r.min, x.initial));
                  })
                }
                options={VAR_TYPES.map((x) => ({ value: x.value, label: x.label }))}
              />
              <IntInput
                aria-label="Initial value"
                value={v.initial}
                min={t.min}
                max={t.max}
                onCommit={(initial) => update((d) => void (d.graph.variables[i]!.initial = initial))}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
