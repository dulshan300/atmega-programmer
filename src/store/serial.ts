import { create } from "zustand";
import { persist } from "zustand/middleware";
import { api, events, toAppError } from "@/lib/ipc";
import type { LineEnding, PortInfo, SerialConfig } from "@/lib/types";
import { useApp } from "./app";

const MAX_CHUNKS = 20_000;

export interface Chunk {
  dir: "rx" | "tx" | "sys";
  bytes: number[];
  ts: number;
}

type Status = "disconnected" | "connecting" | "connected";

interface SerialState {
  ports: PortInfo[];
  config: SerialConfig;
  status: Status;
  lastError: string | null;
  chunks: Chunk[];
  rxBytes: number;
  txBytes: number;
  display: "text" | "hex";
  timestamps: boolean;
  autoscroll: boolean;
  lineEnding: LineEnding;
  sendMode: "text" | "hex";
  history: string[];

  refreshPorts(): Promise<void>;
  setConfig(patch: Partial<SerialConfig>): void;
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  send(input: string): Promise<boolean>;
  clear(): void;
  set<K extends keyof SerialState>(key: K, value: SerialState[K]): void;
}

let unlisteners: (() => void)[] = [];
const encoder = new TextEncoder();
const sys = (text: string): Chunk => ({
  dir: "sys",
  bytes: Array.from(encoder.encode(text)),
  ts: Date.now(),
});

export const useSerial = create<SerialState>()(
  persist(
    (set, get) => {
      const add = (c: Chunk) =>
        set((s) => {
          const chunks = s.chunks.length >= MAX_CHUNKS ? s.chunks.slice(-MAX_CHUNKS / 2) : s.chunks.slice();
          chunks.push(c);
          return {
            chunks,
            rxBytes: s.rxBytes + (c.dir === "rx" ? c.bytes.length : 0),
            txBytes: s.txBytes + (c.dir === "tx" ? c.bytes.length : 0),
          };
        });

      async function listenOnce() {
        if (unlisteners.length) return;
        unlisteners = [
          await events.serialData((d) => add({ dir: "rx", bytes: d.bytes, ts: d.timestampMs })),
          await events.serialStatus((st) => {
            if (st.state === "connected") {
              set({ status: "connected", lastError: null });
              add(sys(`Connected to ${st.port}`));
            } else {
              set({ status: "disconnected", lastError: st.reason });
              add(
                sys(
                  st.reason ? `Disconnected from ${st.port}: ${st.reason}` : `Disconnected from ${st.port}`,
                ),
              );
              if (st.reason)
                useApp.getState().notify("warning", `Serial port ${st.port} disconnected: ${st.reason}`);
            }
          }),
        ];
      }

      return {
        ports: [],
        config: { port: "", baud: 9600, dataBits: 8, parity: "none", stopBits: 1, flowControl: "none" },
        status: "disconnected",
        lastError: null,
        chunks: [],
        rxBytes: 0,
        txBytes: 0,
        display: "text",
        timestamps: true,
        autoscroll: true,
        lineEnding: "crLf",
        sendMode: "text",
        history: [],

        async refreshPorts() {
          try {
            const ports = await api.serialListPorts();
            set({ ports });
            const cur = get().config.port;
            if (!cur && ports[0]) get().setConfig({ port: ports[0].name });
          } catch (e) {
            set({ lastError: toAppError(e).message });
          }
        },

        setConfig: (patch) => set((s) => ({ config: { ...s.config, ...patch } })),

        async connect() {
          const { config } = get();
          if (!config.port) {
            set({ lastError: "Choose a serial port first." });
            return;
          }
          await listenOnce();
          set({ status: "connecting", lastError: null });
          try {
            await api.serialOpen(config);
          } catch (e) {
            const msg = toAppError(e).message;
            set({ status: "disconnected", lastError: msg });
            add(sys(msg));
          }
        },

        async disconnect() {
          await api.serialClose().catch(() => {});
          set({ status: "disconnected" });
        },

        async send(input) {
          const { sendMode, lineEnding, status } = get();
          if (status !== "connected") return false;
          try {
            const payload =
              sendMode === "hex"
                ? { mode: "hex" as const, hex: input }
                : { mode: "text" as const, text: input, lineEnding };
            await api.serialWrite(payload);
            const bytes =
              sendMode === "hex"
                ? parseHexPreview(input)
                : Array.from(
                    encoder.encode(input + { none: "", lf: "\n", cr: "\r", crLf: "\r\n" }[lineEnding]),
                  );
            add({ dir: "tx", bytes, ts: Date.now() });
            set((s) => ({
              history: [input, ...s.history.filter((h) => h !== input)].slice(0, 30),
              lastError: null,
            }));
            return true;
          } catch (e) {
            set({ lastError: toAppError(e).message });
            return false;
          }
        },

        clear: () => set({ chunks: [], rxBytes: 0, txBytes: 0 }),
        set: (key, value) => set({ [key]: value } as Partial<SerialState>),
      };
    },
    {
      name: "avsv.serial",
      version: 1,
      partialize: (s) => ({
        config: s.config,
        display: s.display,
        timestamps: s.timestamps,
        lineEnding: s.lineEnding,
        sendMode: s.sendMode,
        history: s.history,
      }),
    },
  ),
);

/** Mirrors the backend's hex parser for the local echo (the backend is authoritative). */
export function parseHexPreview(input: string): number[] {
  return input
    .split(/[\s,]+/)
    .filter(Boolean)
    .map((t) => Number.parseInt(t.replace(/^0x/i, ""), 16))
    .filter((n) => Number.isInteger(n) && n >= 0 && n <= 255);
}

/** Renders chunks as lines of text, splitting on LF. */
export function decodeText(bytes: number[]): string {
  return new TextDecoder("utf-8", { fatal: false }).decode(new Uint8Array(bytes));
}
