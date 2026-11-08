import clsx from "clsx";
import { FileDown, Plug, PlugZap, RefreshCw, Send, Trash2 } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Callout, Card, Field, IconButton, Input, Select, Switch } from "@/components/ui";
import { api, dialogs, toAppError } from "@/lib/ipc";
import { clockTime } from "@/lib/format";
import type { LineEnding, SerialConfig } from "@/lib/types";
import { useApp } from "@/store/app";
import { useProject } from "@/store/project";
import { decodeText, useSerial, type Chunk } from "@/store/serial";

interface Line {
  dir: Chunk["dir"];
  ts: number;
  text: string;
}

const hex = (bytes: number[]) => bytes.map((b) => b.toString(16).toUpperCase().padStart(2, "0")).join(" ");

/** Groups received bytes into lines (split on LF); TX and system entries stay separate. */
export function toLines(chunks: Chunk[], mode: "text" | "hex"): Line[] {
  const out: Line[] = [];
  let pending: { ts: number; bytes: number[] } | null = null;
  const flush = () => {
    if (pending)
      out.push({
        dir: "rx",
        ts: pending.ts,
        text: mode === "hex" ? hex(pending.bytes) : decodeText(pending.bytes).replace(/\r$/, ""),
      });
    pending = null;
  };
  for (const c of chunks) {
    if (c.dir !== "rx") {
      flush();
      out.push({
        dir: c.dir,
        ts: c.ts,
        text: mode === "hex" && c.dir === "tx" ? hex(c.bytes) : decodeText(c.bytes).replace(/\r?\n$/, ""),
      });
      continue;
    }
    for (const b of c.bytes) {
      pending ??= { ts: c.ts, bytes: [] };
      if (mode === "text" && b === 10) {
        flush();
        continue;
      }
      pending.bytes.push(b);
      if (mode === "hex" && pending.bytes.length >= 16) flush();
    }
  }
  flush();
  return out;
}

export function SerialConsole({ compact }: { compact?: boolean }) {
  const { chunks, display, timestamps, autoscroll, status, lineEnding, sendMode, history, send, clear, set } =
    useSerial();
  const notify = useApp((s) => s.notify);
  const [input, setInput] = useState("");
  const [hIdx, setHIdx] = useState(-1);
  const ref = useRef<HTMLDivElement>(null);
  const lines = useMemo(() => toLines(chunks, display), [chunks, display]);
  useEffect(() => {
    if (autoscroll && ref.current) ref.current.scrollTop = ref.current.scrollHeight;
  }, [lines, autoscroll]);

  const submit = async () => {
    if (!input && sendMode === "hex") return;
    if (await send(input)) {
      setInput("");
      setHIdx(-1);
    }
  };

  const exportLog = async () => {
    const path = await dialogs.saveFile("Save serial log", "serial-log.txt", ["txt", "log", "csv"]);
    if (!path) return;
    const text = lines.map((l) => `${clockTime(l.ts)} ${l.dir.toUpperCase()} ${l.text}`).join("\n");
    try {
      await api.saveTextFile(path, text);
      notify("success", `Saved ${path}`);
    } catch (e) {
      notify("error", toAppError(e).message);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 flex-wrap items-center gap-3 border-b border-line px-2 py-1 text-xs">
        <Select<"text" | "hex">
          aria-label="Display"
          className="w-24"
          value={display}
          onChange={(v) => set("display", v)}
          options={[
            { value: "text", label: "Text" },
            { value: "hex", label: "Hex" },
          ]}
        />
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={timestamps}
            onChange={(e) => set("timestamps", e.target.checked)}
            className="accent-[var(--accent)]"
          />{" "}
          Timestamps
        </label>
        <label className="flex items-center gap-1">
          <input
            type="checkbox"
            checked={autoscroll}
            onChange={(e) => set("autoscroll", e.target.checked)}
            className="accent-[var(--accent)]"
          />{" "}
          Autoscroll
        </label>
        <div className="flex-1" />
        <IconButton
          icon={FileDown}
          label="Save log to file"
          disabled={!lines.length}
          onClick={() => void exportLog()}
        />
        <IconButton icon={Trash2} label="Clear" onClick={clear} />
      </div>
      <div
        ref={ref}
        className="selectable min-h-0 flex-1 overflow-auto px-3 py-1 font-mono text-xs leading-5"
        role="log"
        aria-label="Serial data"
      >
        {lines.length === 0 && (
          <div className="text-faint">{status === "connected" ? "Waiting for data…" : "Not connected."}</div>
        )}
        {lines.map((l, i) => (
          <div
            key={i}
            className={clsx(
              "whitespace-pre-wrap",
              l.dir === "tx" && "text-accent",
              l.dir === "sys" && "text-faint italic",
            )}
          >
            {timestamps && <span className="mr-2 text-faint">{clockTime(l.ts)}</span>}
            {l.dir === "tx" && <span className="mr-1">→</span>}
            {l.text}
          </div>
        ))}
      </div>
      <div className="flex shrink-0 items-center gap-2 border-t border-line p-1.5">
        <Input
          value={input}
          disabled={status !== "connected"}
          placeholder={
            status !== "connected"
              ? "Connect to send"
              : sendMode === "hex"
                ? "Hex bytes, e.g. 48 65 0x6C"
                : "Text to send (↑/↓ history)"
          }
          className="font-mono"
          aria-label="Data to send"
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void submit();
            else if (e.key === "ArrowUp" && history.length) {
              const n = Math.min(hIdx + 1, history.length - 1);
              setHIdx(n);
              setInput(history[n] ?? "");
              e.preventDefault();
            } else if (e.key === "ArrowDown") {
              const n = Math.max(hIdx - 1, -1);
              setHIdx(n);
              setInput(n < 0 ? "" : (history[n] ?? ""));
              e.preventDefault();
            }
          }}
        />
        {!compact && (
          <Select<"text" | "hex">
            aria-label="Send mode"
            className="w-24"
            value={sendMode}
            onChange={(v) => set("sendMode", v)}
            options={[
              { value: "text", label: "Text" },
              { value: "hex", label: "Hex" },
            ]}
          />
        )}
        {sendMode === "text" && (
          <Select<LineEnding>
            aria-label="Line ending"
            className="w-28"
            value={lineEnding}
            onChange={(v) => set("lineEnding", v)}
            options={[
              { value: "none", label: "No ending" },
              { value: "lf", label: "LF" },
              { value: "cr", label: "CR" },
              { value: "crLf", label: "CR+LF" },
            ]}
          />
        )}
        <Button icon={Send} disabled={status !== "connected"} onClick={() => void submit()}>
          Send
        </Button>
      </div>
    </div>
  );
}

export default function SerialMonitor() {
  const { ports, config, status, lastError, rxBytes, txBytes, refreshPorts, setConfig, connect, disconnect } =
    useSerial();
  const project = useProject((s) => s.project);
  useEffect(() => {
    void refreshPorts();
  }, [refreshPorts]);
  const projBaud = project?.peripherals.usart.enabled ? project.peripherals.usart.baud : null;
  const locked = status !== "disconnected";
  const port = ports.find((p) => p.name === config.port);

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <Card
        title="Connection"
        actions={
          status === "connected" ? (
            <Button size="sm" icon={PlugZap} onClick={() => void disconnect()}>
              Disconnect
            </Button>
          ) : (
            <Button
              size="sm"
              variant="primary"
              icon={Plug}
              busy={status === "connecting"}
              disabled={!config.port}
              onClick={() => void connect()}
            >
              Connect
            </Button>
          )
        }
      >
        <div
          className={clsx(
            "grid grid-cols-2 gap-3 lg:grid-cols-6",
            locked && "pointer-events-none opacity-60",
          )}
        >
          <Field label="Port" className="col-span-2">
            <div className="flex gap-1">
              <Select
                value={config.port}
                onChange={(p) => setConfig({ port: p })}
                options={[
                  { value: "", label: ports.length ? "Select a port…" : "No ports found" },
                  ...ports.map((p) => ({
                    value: p.name,
                    label: `${p.name}${p.product ? ` — ${p.product}` : ""}${p.inUseBy ? ` (in use: ${p.inUseBy})` : ""}`,
                  })),
                  ...(config.port && !port
                    ? [{ value: config.port, label: `${config.port} (not present)` }]
                    : []),
                ]}
              />
              <Button icon={RefreshCw} aria-label="Refresh ports" onClick={() => void refreshPorts()} />
            </div>
          </Field>
          <Field
            label="Baud"
            hint={projBaud && projBaud !== config.baud ? `Project UART uses ${projBaud}` : undefined}
          >
            <Select
              value={String(config.baud)}
              onChange={(v) => setConfig({ baud: Number(v) })}
              options={[
                ...new Set([
                  300,
                  1200,
                  2400,
                  4800,
                  9600,
                  19200,
                  38400,
                  57600,
                  115200,
                  230400,
                  250000,
                  500000,
                  1000000,
                  ...(projBaud ? [projBaud] : []),
                  config.baud,
                ]),
              ]
                .sort((a, b) => a - b)
                .map((b) => ({ value: String(b), label: String(b) }))}
            />
          </Field>
          <Field label="Data bits">
            <Select
              value={String(config.dataBits)}
              onChange={(v) => setConfig({ dataBits: Number(v) as SerialConfig["dataBits"] })}
              options={[5, 6, 7, 8].map((n) => ({ value: String(n), label: String(n) }))}
            />
          </Field>
          <Field label="Parity">
            <Select<SerialConfig["parity"]>
              value={config.parity}
              onChange={(parity) => setConfig({ parity })}
              options={[
                { value: "none", label: "None" },
                { value: "even", label: "Even" },
                { value: "odd", label: "Odd" },
              ]}
            />
          </Field>
          <Field label="Stop bits / flow">
            <div className="flex gap-1">
              <Select
                value={String(config.stopBits)}
                onChange={(v) => setConfig({ stopBits: Number(v) as 1 | 2 })}
                options={[
                  { value: "1", label: "1" },
                  { value: "2", label: "2" },
                ]}
              />
              <Select<SerialConfig["flowControl"]>
                aria-label="Flow control"
                value={config.flowControl}
                onChange={(flowControl) => setConfig({ flowControl })}
                options={[
                  { value: "none", label: "None" },
                  { value: "software", label: "XON/XOFF" },
                  { value: "hardware", label: "RTS/CTS" },
                ]}
              />
            </div>
          </Field>
        </div>
        {projBaud && projBaud !== config.baud && !locked && (
          <div className="mt-2">
            <Switch
              checked={false}
              onChange={() => setConfig({ baud: projBaud })}
              label={`Use the project's UART baud rate (${projBaud})`}
            />
          </div>
        )}
        <div className="mt-2 flex gap-4 text-xs text-muted">
          <span className={status === "connected" ? "text-ok" : undefined}>● {status}</span>
          <span>RX {rxBytes} B</span>
          <span>TX {txBytes} B</span>
          {port?.vid != null && (
            <span className="font-mono">
              USB {port.vid.toString(16).padStart(4, "0")}:{(port.pid ?? 0).toString(16).padStart(4, "0")}{" "}
              {port.manufacturer ?? ""}
            </span>
          )}
        </div>
        {lastError && (
          <div className="mt-2">
            <Callout tone="danger">{lastError}</Callout>
          </div>
        )}
      </Card>
      <div className="min-h-0 flex-1 overflow-hidden rounded-md border border-line bg-panel">
        <SerialConsole />
      </div>
    </div>
  );
}
