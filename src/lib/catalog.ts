// The block catalog is shared with the Rust backend (blocks/catalog.json),
// so the editor and the code generator always agree on ports and properties.

import { z } from "zod";
import raw from "@blocks/catalog.json";

const PortSchema = z.object({
  id: z.string(),
  label: z.string(),
  type: z.enum(["exec", "bool", "int", "variable"]),
  fallbackProperty: z.string().optional(),
});

const OptionSchema = z.object({ value: z.string(), label: z.string() });

const PropertySchema = z.object({
  id: z.string(),
  label: z.string(),
  kind: z.enum([
    "pin",
    "bool",
    "int",
    "string",
    "enum",
    "variable",
    "identifier",
    "adc_channel",
    "ext_interrupt",
  ]),
  default: z.union([z.string(), z.number(), z.boolean()]),
  min: z.number().optional(),
  max: z.number().optional(),
  maxLength: z.number().optional(),
  optional: z.boolean().optional(),
  trueLabel: z.string().optional(),
  falseLabel: z.string().optional(),
  options: z.array(OptionSchema).optional(),
});

const BlockSchema = z.object({
  type: z.string(),
  category: z.string(),
  label: z.string(),
  description: z.string(),
  unique: z.boolean().optional(),
  inputs: z.array(PortSchema),
  outputs: z.array(PortSchema),
  properties: z.array(PropertySchema),
});

const CatalogSchema = z.object({
  schemaVersion: z.number(),
  categories: z.array(z.object({ id: z.string(), label: z.string() })),
  blocks: z.array(BlockSchema),
});

export type PortDef = z.infer<typeof PortSchema>;
export type PortType = PortDef["type"];
export type PropertyDef = z.infer<typeof PropertySchema>;
export type BlockDef = z.infer<typeof BlockSchema>;
export type Catalog = z.infer<typeof CatalogSchema>;

export const catalog: Catalog = CatalogSchema.parse(raw);

const byType = new Map(catalog.blocks.map((b) => [b.type, b]));

export function blockDef(type: string): BlockDef | undefined {
  return byType.get(type);
}

export function defaultProperties(def: BlockDef): Record<string, string | number | boolean> {
  return Object.fromEntries(def.properties.map((p) => [p.id, p.default]));
}

/**
 * Port types that may be connected (output → input), using the same rule as
 * the backend: types must match exactly after `variable` ports are resolved
 * to the selected variable's type. `null` means "variable not chosen yet";
 * such links are allowed and the backend reports the missing variable.
 */
export function portsCompatible(out: PortType | null, input: PortType | null): boolean {
  if (out === null || input === null) return out !== "exec" && input !== "exec";
  return out === input;
}

export const PORT_COLORS: Record<PortType, string> = {
  exec: "var(--port-exec)",
  bool: "var(--port-bool)",
  int: "var(--port-int)",
  variable: "var(--port-var)",
};
