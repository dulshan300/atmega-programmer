// Runtime schemas for project data. They mirror the Rust structs (which use
// `deny_unknown_fields`), so objects are strict: a project that would be
// rejected by the backend is rejected here first, with a readable message.

import { z } from "zod";

const u8 = z.number().int().min(0).max(255);
const u32 = z.number().int().min(0).max(0xffff_ffff);

export const ProgrammerConfigSchema = z.strictObject({
  programmer: z.string().min(1),
  port: z.string(),
  baud: u32.nullable(),
  bitclock: z.string().nullable(),
});

export const ManifestSchema = z.strictObject({
  schemaVersion: z.number().int().positive(),
  name: z.string().min(1).max(64),
  device: z.strictObject({ mcu: z.string().min(1), package: z.string().min(1) }),
  toolchain: z.strictObject({
    profile: z.enum(["auto", "winavr_legacy", "modern"]),
    optimization: z.enum(["s", "0", "1", "2", "3"]),
  }),
  programmer: ProgrammerConfigSchema,
});

export const ClockConfigSchema = z.strictObject({
  source: z.string().min(1),
  sourceFrequencyHz: u32.min(1),
  ckdiv8: z.boolean(),
});

export const PinAssignmentSchema = z.strictObject({
  // Pin names and labels are checked by the backend validator, which reports
  // problems as navigable diagnostics instead of refusing to open the project.
  pin: z.string().min(1),
  label: z.string(),
  mode: z.enum(["output", "input", "input_pullup"]),
  initialHigh: z.boolean(),
});

export const PeripheralsConfigSchema = z.strictObject({
  usart: z.strictObject({
    enabled: z.boolean(),
    baud: u32.min(1),
    dataBits: z.number().int().min(5).max(9),
    parity: z.enum(["none", "even", "odd"]),
    stopBits: z.number().int().min(1).max(2),
  }),
  adc: z.strictObject({
    enabled: z.boolean(),
    reference: z.string(),
    prescaler: u32,
  }),
  disableJtagAtStartup: z.boolean(),
});

export const PropertyValueSchema = z.union([z.string(), z.number(), z.boolean(), z.null()]);

export const GraphNodeSchema = z.strictObject({
  id: z.string().min(1),
  type: z.string().min(1),
  position: z.strictObject({ x: z.number(), y: z.number() }),
  properties: z.record(z.string(), PropertyValueSchema).default({}),
});

export const GraphEdgeSchema = z.strictObject({
  id: z.string().min(1),
  source: z.string().min(1),
  sourceHandle: z.string().min(1),
  target: z.string().min(1),
  targetHandle: z.string().min(1),
});

export const VariableSchema = z.strictObject({
  name: z.string().regex(/^[A-Za-z_][A-Za-z0-9_]{0,31}$/, "must be a valid C identifier"),
  type: z.enum(["bool", "uint8", "int8", "uint16", "int16", "uint32", "int32"]),
  initial: z.number().int(),
});

export const GraphSchema = z.strictObject({
  schemaVersion: z.number().int().positive(),
  nodes: z.array(GraphNodeSchema),
  edges: z.array(GraphEdgeSchema),
  variables: z.array(VariableSchema),
});

export const ProjectSchema = z.strictObject({
  manifest: ManifestSchema,
  clock: ClockConfigSchema,
  pins: z.strictObject({ assignments: z.array(PinAssignmentSchema) }),
  peripherals: PeripheralsConfigSchema,
  fuses: z.strictObject({ desired: z.record(z.string(), u8) }),
  graph: GraphSchema,
});

export const LoadedProjectSchema = z.object({
  root: z.string().min(1),
  project: ProjectSchema,
  notes: z.array(z.string()),
});

/** Formats the first few Zod issues as one human-readable line. */
export function describeIssues(error: z.ZodError, max = 3): string {
  return error.issues
    .slice(0, max)
    .map((i) => `${i.path.join(".") || "(root)"}: ${i.message}`)
    .join("; ");
}
