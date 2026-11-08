// Pure helpers for editing the visual graph. Kept free of React so they can
// be unit-tested and reused by the editor, inspector and keyboard commands.

import { blockDef, defaultProperties, portsCompatible, type PortType } from "@/lib/catalog";
import type { Graph, GraphEdge, GraphNode, Project } from "@/lib/types";

export function newNodeId(type: string, existing: Set<string>): string {
  const base = type.replace(/[^a-z0-9]+/gi, "_").slice(0, 16);
  for (;;) {
    const id = `${base}_${Math.random().toString(36).slice(2, 7)}`;
    if (!existing.has(id)) return id;
  }
}

export function edgeId(e: Omit<GraphEdge, "id">): string {
  return `e-${e.source}-${e.sourceHandle}-${e.target}-${e.targetHandle}`;
}

/** Type of a port after resolving `variable` ports to the chosen variable's type. */
export function resolvedPortType(graph: Graph, node: GraphNode, t: PortType): PortType | null {
  if (t !== "variable") return t;
  const name = node.properties.variable;
  const v = typeof name === "string" ? graph.variables.find((x) => x.name === name) : undefined;
  if (!v) return null;
  return v.type === "bool" ? "bool" : "int";
}

export interface ConnectRequest {
  source: string;
  sourceHandle: string;
  target: string;
  targetHandle: string;
}

/** Why a connection is not allowed, or null if it is. */
export function connectionError(graph: Graph, c: ConnectRequest): string | null {
  if (c.source === c.target) return "A block cannot connect to itself.";
  const sn = graph.nodes.find((n) => n.id === c.source);
  const tn = graph.nodes.find((n) => n.id === c.target);
  if (!sn || !tn) return "Block not found.";
  const sp = blockDef(sn.type)?.outputs.find((p) => p.id === c.sourceHandle);
  const tp = blockDef(tn.type)?.inputs.find((p) => p.id === c.targetHandle);
  if (!sp || !tp) return "Port not found.";
  const st = resolvedPortType(graph, sn, sp.type);
  const tt = resolvedPortType(graph, tn, tp.type);
  if (!portsCompatible(st, tt))
    return `Cannot connect ${st ?? "unset variable"} output to ${tt ?? "unset variable"} input.`;
  return null;
}

/**
 * Adds a connection, replacing connections that the backend would reject as
 * ambiguous: a data input or an exec output has at most one connection, and
 * an exec input is reached from at most one place.
 */
export function connect(project: Project, c: ConnectRequest): void {
  const g = project.graph;
  const sn = g.nodes.find((n) => n.id === c.source);
  const sp = sn && blockDef(sn.type)?.outputs.find((p) => p.id === c.sourceHandle);
  const isExec = sp?.type === "exec";
  g.edges = g.edges.filter((e) => {
    if (e.target === c.target && e.targetHandle === c.targetHandle) return false; // one source per input
    if (isExec && e.source === c.source && e.sourceHandle === c.sourceHandle) return false; // one target per exec output
    return true;
  });
  g.edges.push({ id: edgeId(c), ...c });
}

export function addNode(project: Project, type: string, position: { x: number; y: number }): string | null {
  const def = blockDef(type);
  if (!def) return null;
  if (def.unique && project.graph.nodes.some((n) => n.type === type)) return null;
  const id = newNodeId(type, new Set(project.graph.nodes.map((n) => n.id)));
  project.graph.nodes.push({
    id,
    type,
    position: { x: Math.round(position.x), y: Math.round(position.y) },
    properties: defaultProperties(def),
  });
  return id;
}

export function removeNodes(project: Project, ids: Set<string>): void {
  project.graph.nodes = project.graph.nodes.filter((n) => !ids.has(n.id) || n.type === "program_start");
  const alive = new Set(project.graph.nodes.map((n) => n.id));
  project.graph.edges = project.graph.edges.filter((e) => alive.has(e.source) && alive.has(e.target));
}

export function duplicateNodes(project: Project, ids: string[]): string[] {
  const existing = new Set(project.graph.nodes.map((n) => n.id));
  const map = new Map<string, string>();
  for (const id of ids) {
    const n = project.graph.nodes.find((x) => x.id === id);
    if (!n || blockDef(n.type)?.unique) continue;
    const nid = newNodeId(n.type, existing);
    existing.add(nid);
    map.set(id, nid);
    project.graph.nodes.push({
      ...structuredClone(n),
      id: nid,
      position: { x: n.position.x + 40, y: n.position.y + 40 },
    });
  }
  // Keep connections between duplicated blocks.
  for (const e of [...project.graph.edges]) {
    const s = map.get(e.source);
    const t = map.get(e.target);
    if (s && t) {
      const ne = { source: s, sourceHandle: e.sourceHandle, target: t, targetHandle: e.targetHandle };
      project.graph.edges.push({ id: edgeId(ne), ...ne });
    }
  }
  return [...map.values()];
}

/** Renames a variable and every block property that references it. */
export function renameVariable(project: Project, from: string, to: string): void {
  for (const v of project.graph.variables) if (v.name === from) v.name = to;
  for (const n of project.graph.nodes) {
    const def = blockDef(n.type);
    for (const p of def?.properties ?? [])
      if (p.kind === "variable" && n.properties[p.id] === from) n.properties[p.id] = to;
  }
}

export function variableUsage(project: Project, name: string): number {
  let count = 0;
  for (const n of project.graph.nodes) {
    for (const p of blockDef(n.type)?.properties ?? [])
      if (p.kind === "variable" && n.properties[p.id] === name) count++;
  }
  return count;
}
