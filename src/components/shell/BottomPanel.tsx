import clsx from "clsx";
import { AlertCircle, AlertTriangle, ChevronDown, Info, Trash2, type LucideIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { IconButton } from "@/components/ui";
import { SerialConsole } from "@/workspaces/SerialMonitor";
import type { Diagnostic, Severity } from "@/lib/types";
import { useApp, type BottomTab } from "@/store/app";
import { useBuild, type LogLine } from "@/store/build";
import { revealDiagnostic } from "@/store/nav";
import { useProgrammer } from "@/store/programmer";
import { diagnosticCounts, useProject } from "@/store/project";

export function LogView({ lines, empty }: { lines: LogLine[]; empty: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const stick = useRef(true);
  useEffect(() => {
    const el = ref.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [lines]);
  return (
    <div
      ref={ref}
      onScroll={(e) => {
        const el = e.currentTarget;
        stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className="selectable h-full overflow-auto px-3 py-1.5 font-mono text-xs leading-5"
      role="log"
      aria-live="off"
    >
      {lines.length === 0 ? (
        <div className="text-faint">{empty}</div>
      ) : (
        lines.map((l, i) => (
          <div
            key={i}
            className={clsx(
              "whitespace-pre-wrap",
              l.stream === "stderr" && /error|fail|cannot|not found/i.test(l.text)
                ? "text-danger"
                : l.stream === "stderr"
                  ? "text-muted"
                  : "",
              /^==/.test(l.text) && "font-semibold text-accent",
              /warning/i.test(l.text) && "text-warn",
            )}
          >
            {l.text || " "}
          </div>
        ))
      )}
    </div>
  );
}

const SEVERITY: Record<Severity, { icon: LucideIcon; cls: string }> = {
  error: { icon: AlertCircle, cls: "text-danger" },
  warning: { icon: AlertTriangle, cls: "text-warn" },
  info: { icon: Info, cls: "text-info" },
};

export function DiagnosticRow({ d }: { d: Diagnostic }) {
  const { icon: Icon, cls } = SEVERITY[d.severity];
  const where = d.file
    ? `${d.file}${d.line ? `:${d.line}` : ""}`
    : d.pin
      ? d.pin
      : d.nodeId
        ? "block"
        : d.section;
  return (
    <button
      type="button"
      onClick={() => revealDiagnostic(d)}
      className="flex w-full items-start gap-2 px-3 py-1 text-left text-xs hover:bg-raised"
    >
      <Icon size={13} className={clsx("mt-px shrink-0", cls)} aria-label={d.severity} />
      <span className="flex-1">{d.message}</span>
      {where && <span className="shrink-0 font-mono text-[11px] text-faint">{where}</span>}
      <span className="shrink-0 font-mono text-[11px] text-faint">{d.code}</span>
    </button>
  );
}

function Problems() {
  const validation = useProject((s) => s.diagnostics);
  const report = useBuild((s) => s.report);
  // Build diagnostics with a file location (compiler output) complement the live validation.
  const compiler = (report?.diagnostics ?? []).filter((d) => d.file);
  const all = [...validation, ...compiler];
  if (!all.length) return <div className="px-3 py-2 text-xs text-faint">No problems detected.</div>;
  return (
    <div className="h-full overflow-auto py-1">
      {all.map((d, i) => (
        <DiagnosticRow key={i} d={d} />
      ))}
    </div>
  );
}

export function BottomPanel() {
  const tab = useApp((s) => s.bottomTab);
  const showBottomTab = useApp((s) => s.showBottomTab);
  const toggle = useApp((s) => s.toggle);
  const buildLines = useBuild((s) => s.lines);
  const clearBuild = useBuild((s) => s.clear);
  const progLines = useProgrammer((s) => s.lines);
  const clearProg = useProgrammer((s) => s.clearLog);
  const diags = useProject((s) => s.diagnostics);
  const { errors, warnings } = diagnosticCounts(diags);

  const tabs: { id: BottomTab; label: string; badge?: React.ReactNode }[] = [
    { id: "output", label: "Build output" },
    {
      id: "problems",
      label: "Problems",
      badge:
        errors + warnings > 0 ? (
          <span
            className={clsx(
              "rounded px-1 font-mono text-[10px]",
              errors ? "bg-danger/20 text-danger" : "bg-warn/20 text-warn",
            )}
          >
            {errors + warnings}
          </span>
        ) : null,
    },
    { id: "programmer", label: "Programmer log" },
    { id: "serial", label: "Serial" },
  ];

  return (
    <div className="flex h-full flex-col bg-panel">
      <div
        className="flex h-8 shrink-0 items-center border-b border-line px-1"
        role="tablist"
        aria-label="Bottom panel"
      >
        {tabs.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => showBottomTab(t.id)}
            className={clsx(
              "flex h-8 items-center gap-1.5 border-b-2 px-3 text-xs",
              tab === t.id ? "border-accent text-text" : "border-transparent text-muted hover:text-text",
            )}
          >
            {t.label}
            {t.badge}
          </button>
        ))}
        <div className="flex-1" />
        {tab === "output" && <IconButton icon={Trash2} label="Clear build output" onClick={clearBuild} />}
        {tab === "programmer" && (
          <IconButton icon={Trash2} label="Clear programmer log" onClick={clearProg} />
        )}
        <IconButton icon={ChevronDown} label="Hide panel" onClick={() => toggle("showBottom")} />
      </div>
      <div className="min-h-0 flex-1" role="tabpanel">
        {tab === "output" && (
          <LogView
            lines={buildLines}
            empty="Build output appears here. Press Build to compile the project."
          />
        )}
        {tab === "problems" && <Problems />}
        {tab === "programmer" && (
          <LogView lines={progLines} empty="AVRDUDE output appears here during device operations." />
        )}
        {tab === "serial" && <SerialConsole compact />}
      </div>
    </div>
  );
}
