import { describe, expect, it } from "vitest";
import { catalog, portsCompatible } from "@/lib/catalog";
import { bitString, extractField, hex2, insertField, parseByte } from "@/lib/fuses";
import { ProjectSchema } from "@/lib/schemas";
import type { FuseByteDef, Project } from "@/lib/types";
import { computeBaud } from "@/workspaces/ClockPeripherals";
import { projectNameError } from "@/workspaces/NewProjectDialog";
import { toLines } from "@/workspaces/SerialMonitor";
import { identifierError } from "@/workspaces/visual/BlockInspector";
import {
  addNode,
  connect,
  connectionError,
  duplicateNodes,
  removeNodes,
  renameVariable,
} from "@/workspaces/visual/graph";

function project(): Project {
  return {
    manifest: {
      schemaVersion: 1,
      name: "t",
      device: { mcu: "atmega328p", package: "PDIP-28" },
      toolchain: { profile: "auto", optimization: "s" },
      programmer: { programmer: "usbasp", port: "usb", baud: null, bitclock: null },
    },
    clock: { source: "internal_rc", sourceFrequencyHz: 8_000_000, ckdiv8: true },
    pins: { assignments: [{ pin: "PB5", label: "LED", mode: "output", initialHigh: false }] },
    peripherals: {
      usart: { enabled: false, baud: 9600, dataBits: 8, parity: "none", stopBits: 1 },
      adc: { enabled: false, reference: "avcc", prescaler: 64 },
      disableJtagAtStartup: false,
    },
    fuses: { desired: { lfuse: 0x62, hfuse: 0xd9, efuse: 0xff } },
    graph: {
      schemaVersion: 1,
      nodes: [{ id: "start", type: "program_start", position: { x: 0, y: 0 }, properties: {} }],
      edges: [],
      variables: [{ name: "flag", type: "bool", initial: 0 }],
    },
  };
}

describe("catalog", () => {
  it("parses the shared block catalog", () => {
    expect(catalog.blocks.length).toBeGreaterThan(20);
    expect(catalog.blocks.find((b) => b.type === "program_start")?.unique).toBe(true);
  });
  it("uses the backend's exact-match port rule", () => {
    expect(portsCompatible("exec", "exec")).toBe(true);
    expect(portsCompatible("int", "int")).toBe(true);
    expect(portsCompatible("bool", "int")).toBe(false);
    expect(portsCompatible(null, "int")).toBe(true);
    expect(portsCompatible(null, "exec")).toBe(false);
  });
});

describe("graph editing", () => {
  it("adds blocks with default properties and refuses a second unique block", () => {
    const p = project();
    const id = addNode(p, "delay_ms", { x: 10.4, y: 20.6 })!;
    expect(p.graph.nodes.find((n) => n.id === id)).toMatchObject({
      position: { x: 10, y: 21 },
      properties: { ms: 500 },
    });
    expect(addNode(p, "program_start", { x: 0, y: 0 })).toBeNull();
  });

  it("validates connections and keeps one link per exec output and data input", () => {
    const p = project();
    const a = addNode(p, "pin_toggle", { x: 0, y: 0 })!;
    const b = addNode(p, "delay_ms", { x: 0, y: 0 })!;
    expect(
      connectionError(p.graph, { source: "start", sourceHandle: "loop", target: a, targetHandle: "in" }),
    ).toBeNull();
    connect(p, { source: "start", sourceHandle: "loop", target: a, targetHandle: "in" });
    connect(p, { source: "start", sourceHandle: "loop", target: b, targetHandle: "in" });
    expect(p.graph.edges.filter((e) => e.source === "start" && e.sourceHandle === "loop")).toHaveLength(1);
    expect(p.graph.edges[0]!.target).toBe(b);
    // bool constant into an int input is rejected
    const c = addNode(p, "const_bool", { x: 0, y: 0 })!;
    expect(
      connectionError(p.graph, { source: c, sourceHandle: "value", target: b, targetHandle: "ms" }),
    ).toMatch(/Cannot connect/);
    expect(
      connectionError(p.graph, { source: b, sourceHandle: "next", target: b, targetHandle: "in" }),
    ).toMatch(/itself/);
  });

  it("resolves variable ports through the selected variable", () => {
    const p = project();
    const get = addNode(p, "var_get", { x: 0, y: 0 })!;
    const delay = addNode(p, "delay_ms", { x: 0, y: 0 })!;
    p.graph.nodes.find((n) => n.id === get)!.properties.variable = "flag";
    expect(
      connectionError(p.graph, { source: get, sourceHandle: "value", target: delay, targetHandle: "ms" }),
    ).toMatch(/bool/);
  });

  it("never deletes Program Start, drops dangling edges and duplicates internal links", () => {
    const p = project();
    const a = addNode(p, "pin_toggle", { x: 0, y: 0 })!;
    const b = addNode(p, "delay_ms", { x: 0, y: 0 })!;
    connect(p, { source: a, sourceHandle: "next", target: b, targetHandle: "in" });
    const copies = duplicateNodes(p, [a, b, "start"]);
    expect(copies).toHaveLength(2);
    expect(p.graph.edges).toHaveLength(2);
    removeNodes(p, new Set(["start", a]));
    expect(p.graph.nodes.some((n) => n.id === "start")).toBe(true);
    expect(p.graph.edges.every((e) => e.source !== a)).toBe(true);
  });

  it("renames variables everywhere they are referenced", () => {
    const p = project();
    const set = addNode(p, "var_set", { x: 0, y: 0 })!;
    p.graph.nodes.find((n) => n.id === set)!.properties.variable = "flag";
    renameVariable(p, "flag", "ready");
    expect(p.graph.variables[0]!.name).toBe("ready");
    expect(p.graph.nodes.find((n) => n.id === set)!.properties.variable).toBe("ready");
  });
});

describe("fuses", () => {
  const byte: FuseByteDef = {
    id: "efuse",
    label: "Extended",
    defaultValue: 0xff,
    usedMask: 0x07,
    fields: [{ id: "BODLEVEL", mask: 0x07, label: "BOD", defaultValue: 7, options: [] }],
  };
  it("extracts and inserts bit fields", () => {
    const f = { id: "SUT", mask: 0x30, label: "", defaultValue: 0, options: [] };
    expect(extractField(f, 0x62)).toBe(2);
    expect(insertField(f, 0x62, 3)).toBe(0x72);
    expect(bitString(byte, 0xfd)).toBe("-----101");
  });
  it("parses and formats bytes", () => {
    expect(parseByte("0xE2")).toBe(0xe2);
    expect(parseByte("ff")).toBe(255);
    expect(parseByte("0x100")).toBeNull();
    expect(parseByte("zz")).toBeNull();
    expect(hex2(9)).toBe("0x09");
  });
});

describe("validation helpers mirror the backend", () => {
  it("computes UBRR like the backend", () => {
    // 16 MHz, 9600: UBRR 103 normal speed, +0.16 %
    expect(computeBaud(16_000_000, 9600)).toMatchObject({ ubrr: 103, doubleSpeed: false });
    // 1 MHz, 9600: only double speed is usable (UBRR 12, +0.16 %)
    const b = computeBaud(1_000_000, 9600)!;
    expect(b).toMatchObject({ ubrr: 12, doubleSpeed: true });
    expect(Math.abs(b.error)).toBeLessThan(0.2);
  });
  it("checks identifiers and project names", () => {
    expect(identifierError("led_1")).toBeNull();
    expect(identifierError("1led")).not.toBeNull();
    expect(identifierError("int")).not.toBeNull();
    expect(identifierError("x", ["x"])).toMatch(/in use/);
    expect(projectNameError("Blink")).toBeNull();
    expect(projectNameError("a/b")).not.toBeNull();
    expect(projectNameError("x".repeat(65))).not.toBeNull();
  });
  it("accepts a valid project and rejects unknown fields", () => {
    expect(ProjectSchema.safeParse(project()).success).toBe(true);
    expect(ProjectSchema.safeParse({ ...project(), extra: 1 }).success).toBe(false);
  });
});

describe("serial line splitting", () => {
  it("joins chunks into lines and keeps TX separate", () => {
    const enc = (s: string) => Array.from(new TextEncoder().encode(s));
    const lines = toLines(
      [
        { dir: "rx", bytes: enc("hel"), ts: 1 },
        { dir: "rx", bytes: enc("lo\r\nwor"), ts: 2 },
        { dir: "tx", bytes: enc("ping\r\n"), ts: 3 },
        { dir: "rx", bytes: enc("ld"), ts: 4 },
      ],
      "text",
    );
    expect(lines.map((l) => [l.dir, l.text])).toEqual([
      ["rx", "hello"],
      ["rx", "wor"], // partial line received before the send is shown first
      ["tx", "ping"],
      ["rx", "ld"],
    ]);
    expect(toLines([{ dir: "rx", bytes: [0x48, 0x0a], ts: 0 }], "hex")[0]!.text).toBe("48 0A");
  });
});
