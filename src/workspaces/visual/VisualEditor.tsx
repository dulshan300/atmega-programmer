import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  applyNodeChanges,
  useReactFlow,
  type Connection,
  type Edge,
  type EdgeChange,
  type IsValidConnection,
  type NodeChange,
  type OnSelectionChangeParams,
} from "@xyflow/react";
import { Copy, Search, Trash2, Workflow } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { EmptyState, IconButton, Input } from "@/components/ui";
import { useSyncedState } from "@/lib/hooks";
import { blockDef, catalog, PORT_COLORS } from "@/lib/catalog";
import type { Diagnostic } from "@/lib/types";
import { useApp } from "@/store/app";
import { useProject } from "@/store/project";
import { BlockNode, type BlockFlowNode } from "./BlockNode";
import { addNode, connect, connectionError, duplicateNodes, removeNodes } from "./graph";

const nodeTypes = { block: BlockNode };
const DND_TYPE = "application/x-avsv-block";

/** Adds a block at a flow position, or at the centre of the visible canvas. */
function useAddBlock() {
  const rf = useReactFlow();
  const update = useProject((s) => s.update);
  const select = useProject((s) => s.select);
  return useCallback(
    (type: string, pos?: { x: number; y: number }) => {
      let p = pos;
      if (!p) {
        const r = document.querySelector(".react-flow")?.getBoundingClientRect();
        p = rf.screenToFlowPosition({
          x: (r?.left ?? 0) + (r?.width ?? 600) / 2 - 80,
          y: (r?.top ?? 0) + (r?.height ?? 400) / 2 - 40,
        });
      }
      let id: string | null = null;
      const at = p;
      update((d) => void (id = addNode(d, type, at)));
      if (id) select({ kind: "node", id });
    },
    [rf, update, select],
  );
}

function Palette() {
  const onAdd = useAddBlock();
  const [q, setQ] = useState("");
  const nodes = useProject((s) => s.project?.graph.nodes ?? []);
  const query = q.trim().toLowerCase();
  return (
    <aside className="flex w-56 shrink-0 flex-col border-r border-line bg-panel" aria-label="Block palette">
      <div className="p-2">
        <div className="relative">
          <Search size={13} className="absolute top-2 left-2 text-faint" />
          <Input
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search blocks"
            className="pl-7"
            aria-label="Search blocks"
          />
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto pb-2">
        {catalog.categories.map((cat) => {
          const blocks = catalog.blocks.filter(
            (b) =>
              b.category === cat.id &&
              (!query ||
                b.label.toLowerCase().includes(query) ||
                b.description.toLowerCase().includes(query)),
          );
          if (!blocks.length) return null;
          return (
            <div key={cat.id} className="mb-2">
              <div className="px-3 py-1 text-[11px] font-semibold tracking-wider text-faint uppercase">
                {cat.label}
              </div>
              {blocks.map((b) => {
                const disabled = !!b.unique && nodes.some((n) => n.type === b.type);
                return (
                  <button
                    key={b.type}
                    type="button"
                    draggable={!disabled}
                    disabled={disabled}
                    onDragStart={(e) => {
                      e.dataTransfer.setData(DND_TYPE, b.type);
                      e.dataTransfer.effectAllowed = "copy";
                    }}
                    onClick={() => onAdd(b.type)}
                    title={
                      disabled
                        ? "Only one per program"
                        : `${b.description}\n\nClick to add, or drag onto the canvas.`
                    }
                    className="mx-1.5 flex w-[calc(100%-12px)] cursor-grab items-center rounded px-2 py-1 text-left text-[13px] hover:bg-raised disabled:cursor-not-allowed disabled:opacity-40"
                  >
                    {b.label}
                  </button>
                );
              })}
            </div>
          );
        })}
      </div>
      <div className="border-t border-line p-2 text-[11px] text-faint">
        <div className="flex flex-wrap gap-x-3 gap-y-1">
          {(["exec", "bool", "int", "variable"] as const).map((t) => (
            <span key={t} className="flex items-center gap-1">
              <span
                className="inline-block h-2 w-2"
                style={{ background: PORT_COLORS[t], borderRadius: t === "exec" ? 1 : 99 }}
              />
              {t === "exec" ? "execution" : t}
            </span>
          ))}
        </div>
      </div>
    </aside>
  );
}

function Canvas() {
  const project = useProject((s) => s.project);
  const diagnostics = useProject((s) => s.diagnostics);
  const selection = useProject((s) => s.selection);
  const update = useProject((s) => s.update);
  const select = useProject((s) => s.select);
  const notify = useApp((s) => s.notify);
  const rf = useReactFlow();
  const wrapper = useRef<HTMLDivElement>(null);

  const graph = project?.graph;
  const pinLabels = useMemo(
    () => Object.fromEntries((project?.pins.assignments ?? []).map((a) => [a.pin, a.label])),
    [project?.pins.assignments],
  );
  const diagByNode = useMemo(() => {
    const m = new Map<string, Diagnostic[]>();
    for (const d of diagnostics) if (d.nodeId) m.set(d.nodeId, [...(m.get(d.nodeId) ?? []), d]);
    return m;
  }, [diagnostics]);

  const derivedNodes = useMemo<BlockFlowNode[]>(() => {
    if (!graph) return [];
    return graph.nodes.map((n) => ({
      id: n.id,
      type: "block",
      position: n.position,
      deletable: n.type !== "program_start",
      selected: selection.kind === "node" && selection.id === n.id,
      data: {
        blockType: n.type,
        properties: n.properties,
        diagnostics: diagByNode.get(n.id) ?? [],
        pinLabels,
        connectedInputs: graph.edges.filter((e) => e.target === n.id).map((e) => e.targetHandle),
      },
    }));
  }, [graph, diagByNode, pinLabels, selection]);

  // Local node state for smooth dragging; positions are committed on drag stop.
  const [nodes, setNodes] = useSyncedState<BlockFlowNode[]>(derivedNodes);

  const edges = useMemo<Edge[]>(() => {
    if (!graph) return [];
    return graph.edges.map((e) => {
      const sn = graph.nodes.find((n) => n.id === e.source);
      const port = sn ? blockDef(sn.type)?.outputs.find((p) => p.id === e.sourceHandle) : undefined;
      const exec = port?.type === "exec";
      return {
        id: e.id,
        source: e.source,
        sourceHandle: e.sourceHandle,
        target: e.target,
        targetHandle: e.targetHandle,
        className: exec ? "exec" : undefined,
        animated: false,
        selected: selection.kind === "edge" && selection.id === e.id,
        style: {
          stroke: port ? PORT_COLORS[port.type] : undefined,
          strokeDasharray: exec ? undefined : "5 4",
        },
      };
    });
  }, [graph, selection]);

  const onNodesChange = useCallback(
    (changes: NodeChange<BlockFlowNode>[]) => {
      const removed = changes.filter((c) => c.type === "remove").map((c) => c.id);
      if (removed.length) update((d) => removeNodes(d, new Set(removed)));
      setNodes((ns) =>
        applyNodeChanges(
          changes.filter((c) => c.type !== "remove"),
          ns,
        ),
      );
    },
    [update, setNodes],
  );

  const onNodeDragStop = useCallback(
    (_: unknown, __: unknown, dragged: BlockFlowNode[]) => {
      const pos = new Map(dragged.map((n) => [n.id, n.position]));
      update(
        (d) => {
          for (const n of d.graph.nodes) {
            const p = pos.get(n.id);
            if (p) n.position = { x: Math.round(p.x), y: Math.round(p.y) };
          }
        },
        { coalesce: `move:${[...pos.keys()].join(",")}` },
      );
    },
    [update],
  );

  const onEdgesChange = useCallback(
    (changes: EdgeChange[]) => {
      const removed = new Set(changes.filter((c) => c.type === "remove").map((c) => c.id));
      if (removed.size) update((d) => void (d.graph.edges = d.graph.edges.filter((e) => !removed.has(e.id))));
    },
    [update],
  );

  const isValidConnection: IsValidConnection = useCallback((c) => {
    const g = useProject.getState().project?.graph;
    if (!g || !c.sourceHandle || !c.targetHandle) return false;
    return (
      connectionError(g, {
        source: c.source,
        sourceHandle: c.sourceHandle,
        target: c.target,
        targetHandle: c.targetHandle,
      }) === null
    );
  }, []);

  const onConnect = useCallback(
    (c: Connection) => {
      if (!c.sourceHandle || !c.targetHandle) return;
      const req = {
        source: c.source,
        sourceHandle: c.sourceHandle,
        target: c.target,
        targetHandle: c.targetHandle,
      };
      const g = useProject.getState().project?.graph;
      const err = g && connectionError(g, req);
      if (err) return notify("warning", err);
      update((d) => connect(d, req));
    },
    [update, notify],
  );

  const onSelectionChange = useCallback(
    ({ nodes: sn, edges: se }: OnSelectionChangeParams) => {
      const cur = useProject.getState().selection;
      if (sn.length === 1) {
        if (cur.kind !== "node" || cur.id !== sn[0]!.id) select({ kind: "node", id: sn[0]!.id });
      } else if (se.length === 1 && sn.length === 0) {
        if (cur.kind !== "edge" || cur.id !== se[0]!.id) select({ kind: "edge", id: se[0]!.id });
      } else if (sn.length === 0 && se.length === 0 && (cur.kind === "node" || cur.kind === "edge")) {
        select({ kind: "none" });
      }
    },
    [select],
  );

  const addAt = useAddBlock();

  const duplicateSelected = useCallback(() => {
    const ids = nodes.filter((n) => n.selected).map((n) => n.id);
    if (!ids.length) return;
    let created: string[] = [];
    update((d) => void (created = duplicateNodes(d, ids)));
    if (created.length === 1) select({ kind: "node", id: created[0]! });
  }, [nodes, update, select]);

  const deleteSelected = useCallback(() => {
    const ids = new Set(nodes.filter((n) => n.selected).map((n) => n.id));
    const sel = useProject.getState().selection;
    if (ids.size) update((d) => removeNodes(d, ids));
    else if (sel.kind === "edge")
      update((d) => void (d.graph.edges = d.graph.edges.filter((e) => e.id !== sel.id)));
  }, [nodes, update]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input, textarea, select")) return;
      if (
        (e.ctrlKey || e.metaKey) &&
        e.key.toLowerCase() === "d" &&
        wrapper.current?.contains(document.activeElement)
      ) {
        e.preventDefault();
        duplicateSelected();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [duplicateSelected]);

  if (!project) return null;
  const anySelected = nodes.some((n) => n.selected) || selection.kind === "edge";

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div className="flex h-8 shrink-0 items-center gap-1 border-b border-line bg-panel px-2 text-xs text-muted">
        <span>
          {project.graph.nodes.length} blocks · {project.graph.edges.length} connections ·{" "}
          {project.graph.variables.length} variables
        </span>
        <div className="flex-1" />
        <IconButton
          icon={Copy}
          label="Duplicate selection (Ctrl+D)"
          disabled={!nodes.some((n) => n.selected)}
          onClick={duplicateSelected}
        />
        <IconButton
          icon={Trash2}
          label="Delete selection (Del)"
          disabled={!anySelected}
          onClick={deleteSelected}
        />
      </div>
      <div
        ref={wrapper}
        className="relative min-h-0 flex-1"
        onDragOver={(e) => {
          if (e.dataTransfer.types.includes(DND_TYPE)) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "copy";
          }
        }}
        onDrop={(e) => {
          const type = e.dataTransfer.getData(DND_TYPE);
          if (!type) return;
          e.preventDefault();
          addAt(type, rf.screenToFlowPosition({ x: e.clientX, y: e.clientY }));
        }}
      >
        <ReactFlow<BlockFlowNode, Edge>
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onNodeDragStop={onNodeDragStop}
          onConnect={onConnect}
          isValidConnection={isValidConnection}
          onSelectionChange={onSelectionChange}
          onPaneClick={() => select({ kind: "none" })}
          deleteKeyCode={["Delete", "Backspace"]}
          multiSelectionKeyCode={["Shift", "Meta", "Control"]}
          fitView
          fitViewOptions={{ padding: 0.2, maxZoom: 1.1 }}
          minZoom={0.2}
          maxZoom={2}
          snapToGrid
          snapGrid={[10, 10]}
          proOptions={{ hideAttribution: true }}
          colorMode={useApp.getState().theme}
        >
          <Background variant={BackgroundVariant.Dots} gap={20} size={1.2} color="var(--grid)" />
          <Controls showInteractive={false} />
          <MiniMap pannable zoomable nodeColor="var(--line-strong)" maskColor="rgb(0 0 0 / 0.25)" />
        </ReactFlow>
        {project.graph.nodes.length <= 1 && (
          <div className="pointer-events-none absolute inset-x-0 bottom-16 flex justify-center">
            <div className="rounded border border-line bg-panel/90 px-3 py-2 text-xs text-muted">
              Drag blocks from the palette, then connect <b>setup</b> / <b>loop</b> of Program Start to their{" "}
              <b>in</b> ports.
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function Editor() {
  return (
    <div className="flex h-full">
      <Palette />
      <Canvas />
    </div>
  );
}

export default function VisualEditor() {
  const hasProject = useProject((s) => s.project !== null);
  if (!hasProject) return <EmptyState icon={Workflow} title="No project open" />;
  return (
    <ReactFlowProvider>
      <Editor />
    </ReactFlowProvider>
  );
}
