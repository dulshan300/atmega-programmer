// TypeScript mirrors of the Rust (serde, camelCase) data model.
// Project files are validated at runtime with the Zod schemas in ./schemas.ts.

import type { z } from "zod";
import type {
  ProjectSchema,
  GraphSchema,
  GraphNodeSchema,
  GraphEdgeSchema,
  VariableSchema,
  PinAssignmentSchema,
  ClockConfigSchema,
  PeripheralsConfigSchema,
  ManifestSchema,
  ProgrammerConfigSchema,
} from "./schemas";

export type Project = z.infer<typeof ProjectSchema>;
export type Manifest = z.infer<typeof ManifestSchema>;
export type Graph = z.infer<typeof GraphSchema>;
export type GraphNode = z.infer<typeof GraphNodeSchema>;
export type GraphEdge = z.infer<typeof GraphEdgeSchema>;
export type Variable = z.infer<typeof VariableSchema>;
export type PinAssignment = z.infer<typeof PinAssignmentSchema>;
export type ClockConfig = z.infer<typeof ClockConfigSchema>;
export type PeripheralsConfig = z.infer<typeof PeripheralsConfigSchema>;
export type ProgrammerConfig = z.infer<typeof ProgrammerConfigSchema>;
export type VarType = Variable["type"];
export type PinMode = PinAssignment["mode"];
export type Template = "empty" | "blink" | "uart_echo" | "adc_reading";

export interface LoadedProject {
  root: string;
  project: Project;
  notes: string[];
}

export type Severity = "error" | "warning" | "info";

export interface Diagnostic {
  severity: Severity;
  code: string;
  message: string;
  nodeId?: string;
  pin?: string;
  section?: string;
  file?: string;
  line?: number;
  column?: number;
}

// ---------------------------------------------------------------- devices

export interface DeviceSummary {
  id: string;
  name: string;
  packages: string[];
  flashBytes: number;
  sramBytes: number;
  eepromBytes: number;
  maxFrequencyHz: number;
}

export interface ClockSourceDef {
  id: string;
  label: string;
  frequenciesHz?: number[];
  minHz?: number;
  maxHz?: number;
}

export interface PinDef {
  name: string;
  port: string;
  bit: number;
  functions: string[];
}

export interface PackageDef {
  id: string;
  label: string;
  layout: "dip" | "qfp";
  pins: string[];
}

export type FuseRisk = "caution" | "danger" | "blocked";

export interface FuseOptionDef {
  value: number;
  label: string;
  risk?: FuseRisk;
  warning?: string;
  /** For CKSEL options: the clock source this value selects. */
  clockSource?: string;
  frequencyHz?: number;
}

export interface FuseFieldDef {
  id: string;
  mask: number;
  label: string;
  defaultValue: number;
  datasheetRef?: string;
  description?: string;
  options: FuseOptionDef[];
}

export interface FuseByteDef {
  id: string;
  label: string;
  defaultValue: number;
  usedMask: number;
  fields: FuseFieldDef[];
}

export interface AdcChannelDef {
  channel: number;
  pin: string;
  packages?: string[];
}

export interface DeviceDefinition {
  id: string;
  name: string;
  family: string;
  registerProfile: string;
  provenance: { datasheet: string; header: string; revision: string; verification: string };
  compiler: { mmcu: string; header: string };
  avrdude: { partId: string };
  signature: [number, number, number];
  memory: {
    flashBytes: number;
    flashPageBytes: number;
    sramBytes: number;
    sramStart: number;
    eepromBytes: number;
    eepromPageBytes: number;
  };
  clock: {
    maxFrequencyHz: number;
    hasClockPrescalerFuse: boolean;
    sources: ClockSourceDef[];
    crystalPins: string[];
    externalClockPins: string[];
  };
  resetPin: { pin: string; disableFuseField?: string } | null;
  jtagPins: { pins: string[]; fuseField: string } | null;
  ports: Record<string, number[]>;
  pins: PinDef[];
  packages: PackageDef[];
  peripherals: {
    usart: { id: string; rx: string; tx: string; xck?: string }[];
    spi: { id: string; ss: string; mosi: string; miso: string; sck: string }[];
    twi: { id: string; sda: string; scl: string }[];
    adc: {
      resolutionBits: number;
      channels: AdcChannelDef[];
      references: { id: string; label: string; refsBits: number; volts?: number }[];
      prescalers: number[];
      maxClockHz: number;
    };
    timers: { id: string; bits: number; outputs: { id: string; pin: string }[]; prescalers: number[] }[];
    externalInterrupts: { id: string; pin: string }[];
  };
  interruptVectors: { number: number; name: string; description?: string }[];
  fuses: { notes: string; bytes: FuseByteDef[] };
}

// ---------------------------------------------------------------- settings / toolchains

export interface ToolchainSettings {
  avrGccPath: string | null;
  avrdudePath: string | null;
  avrdudeConfPath: string | null;
  extraSearchDirs: string[];
}

export interface RecentProject {
  path: string;
  name: string;
  mcu: string;
  openedAtMs: number;
}

export interface AppSettings {
  schemaVersion: number;
  recentProjects: RecentProject[];
  toolchain: ToolchainSettings;
  theme: "dark" | "light";
}

export type ToolchainProfile = "winavr_legacy" | "modern_windows" | "modern_linux" | "modern_other";

export interface ToolchainStatus {
  gcc: {
    avrGcc: string;
    version: string;
    objcopy: string;
    size: string;
    objdump: string | null;
    nm: string | null;
    detectedProfile: ToolchainProfile;
  } | null;
  avrdude: { path: string; version: string | null; conf: string | null } | null;
  problems: string[];
  searchedDirs: string[];
}

export interface AppInfo {
  version: string;
  os: string;
  devices: DeviceSummary[];
}

// ---------------------------------------------------------------- code / build

export interface GeneratedFile {
  path: string;
  content: string;
}

export interface PreviewResponse {
  files: GeneratedFile[];
  diagnostics: Diagnostic[];
  legacy: boolean;
}

export type BuildStage = "validate" | "toolchain" | "generate" | "compile" | "size" | "hex" | "done";
export type Stream = "stdout" | "stderr";

export interface MemoryRegionUsage {
  used: number;
  total: number;
}

export interface MemoryReport {
  flash: MemoryRegionUsage;
  sramStatic: MemoryRegionUsage;
  eeprom: MemoryRegionUsage;
  sections: { name: string; size: number; address: number }[];
  symbols: { name: string; address: number; size: number; kind: string; region: string }[];
  notes: string[];
}

export interface BuildReport {
  success: boolean;
  cancelled: boolean;
  stage: BuildStage;
  message: string;
  diagnostics: Diagnostic[];
  artifacts: { elf: string; hex: string; eep: string | null; map: string } | null;
  memory: MemoryReport | null;
  commands: string[];
  toolchain: string | null;
  logPath: string | null;
  durationMs: number;
}

export type BuildEvent =
  { kind: "stage"; stage: BuildStage } | { kind: "line"; stream: Stream; text: string };

// ---------------------------------------------------------------- programmer / fuses

export interface ProgrammerDef {
  id: string;
  label: string;
  kind: "isp" | "bootloader";
  connection: "usb" | "serial";
  defaultBaud: number | null;
  notes: string;
}

export interface ProcessOutput {
  command: string;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  timedOut: boolean;
  cancelled: boolean;
  durationMs: number;
}

export interface ProgrammerPreview {
  identify: string;
  upload: string | null;
  hexExists: boolean;
  hexModifiedMs: number | null;
  avrdudeVersion: string | null;
}

export interface IdentifyResult {
  success: boolean;
  signature: string | null;
  detectedDevice: string | null;
  matchesProject: boolean;
  message: string;
  hints: string[];
  output: ProcessOutput;
}

export interface UploadResult {
  success: boolean;
  verified: boolean;
  bytesVerified: number | null;
  message: string;
  hints: string[];
  output: ProcessOutput;
}

export interface DecodedField {
  byte: string;
  field: string;
  label: string;
  raw: number;
  meaning: string;
  risk: FuseRisk | null;
  warning: string | null;
}

export interface FuseReadResponse {
  success: boolean;
  values: Record<string, number>;
  message: string;
  hints: string[];
  output: ProcessOutput;
  decoded: DecodedField[];
}

export interface FusePlanInput {
  desired: Record<string, number>;
  current: Record<string, number> | null;
}

export interface FuseWritePlan {
  device: string;
  signature: string;
  programmer: string;
  bytes: { id: string; label: string; current: number | null; desired: number; changed: boolean }[];
  warnings: string[];
  blocked: string[];
  command: string;
  token: string;
}

export interface FuseWriteResult {
  success: boolean;
  verified: boolean;
  readback: Record<string, number>;
  message: string;
  hints: string[];
  writeOutput: ProcessOutput | null;
  verifyOutput: ProcessOutput | null;
}

// ---------------------------------------------------------------- serial

export interface PortInfo {
  name: string;
  kind: string;
  vid: number | null;
  pid: number | null;
  manufacturer: string | null;
  product: string | null;
  serialNumber: string | null;
  inUseBy: string | null;
}

export interface SerialConfig {
  port: string;
  baud: number;
  dataBits: 5 | 6 | 7 | 8;
  parity: "none" | "odd" | "even";
  stopBits: 1 | 2;
  flowControl: "none" | "software" | "hardware";
}

export type SerialStatus =
  { state: "connected"; port: string } | { state: "disconnected"; port: string; reason: string | null };

export type LineEnding = "none" | "lf" | "cr" | "crLf";

export type SerialPayload =
  { mode: "text"; text: string; lineEnding: LineEnding } | { mode: "hex"; hex: string };
