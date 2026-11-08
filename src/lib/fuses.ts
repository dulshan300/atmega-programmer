// Fuse bit-field helpers. AVR fuse bits are active-low (0 = programmed).
// These are used for editing *desired* values in the UI; anything written to
// hardware is re-checked by the backend's plan → confirm → write workflow.

import type { DeviceDefinition, FuseByteDef, FuseFieldDef, FuseOptionDef } from "./types";

function shift(mask: number): number {
  let s = 0;
  while (s < 8 && ((mask >> s) & 1) === 0) s++;
  return s;
}

export function extractField(field: FuseFieldDef, byte: number): number {
  return (byte & field.mask) >> shift(field.mask);
}

export function insertField(field: FuseFieldDef, byte: number, value: number): number {
  const s = shift(field.mask);
  return ((byte & ~field.mask) | ((value << s) & field.mask)) & 0xff;
}

export function fieldOption(field: FuseFieldDef, raw: number): FuseOptionDef | undefined {
  return field.options.find((o) => o.value === raw);
}

export function defaultFuses(device: DeviceDefinition): Record<string, number> {
  return Object.fromEntries(device.fuses.bytes.map((b) => [b.id, b.defaultValue]));
}

export interface FieldState {
  byte: FuseByteDef;
  field: FuseFieldDef;
  raw: number;
  option: FuseOptionDef | undefined;
}

export function decodeAll(device: DeviceDefinition, values: Record<string, number>): FieldState[] {
  const out: FieldState[] = [];
  for (const byte of device.fuses.bytes) {
    const v = values[byte.id];
    if (v === undefined) continue;
    for (const field of byte.fields) {
      const raw = extractField(field, v);
      out.push({ byte, field, raw, option: fieldOption(field, raw) });
    }
  }
  return out;
}

/** Bits of a value rendered MSB-first, with `-` for unused bits. */
export function bitString(byte: FuseByteDef, value: number): string {
  let s = "";
  for (let i = 7; i >= 0; i--) s += (byte.usedMask >> i) & 1 ? String((value >> i) & 1) : "-";
  return s;
}

export function hex2(n: number): string {
  return "0x" + n.toString(16).toUpperCase().padStart(2, "0");
}

export function parseByte(text: string): number | null {
  const t = text.trim().toLowerCase();
  const v = t.startsWith("0x")
    ? Number.parseInt(t.slice(2), 16)
    : /^[0-9a-f]{1,2}$/.test(t)
      ? Number.parseInt(t, 16)
      : NaN;
  return Number.isInteger(v) && v >= 0 && v <= 255 ? v : null;
}
