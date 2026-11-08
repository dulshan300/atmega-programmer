import { create } from "zustand";
import type { Diagnostic } from "@/lib/types";
import { diagnosticWorkspace } from "@/lib/diagnostics";
import { useApp } from "./app";
import { useProject } from "./project";

interface NavState {
  /** A source location the code view should scroll to (consumed once). */
  codeReveal: { file: string; line: number; seq: number } | null;
  /** A configuration section to highlight (consumed by the workspace). */
  focusSection: { section: string; seq: number } | null;
}

let seq = 0;
export const useNav = create<NavState>()(() => ({ codeReveal: null, focusSection: null }));

/** Navigates to wherever a diagnostic can be fixed. */
export function revealDiagnostic(d: Diagnostic) {
  const ws = diagnosticWorkspace(d);
  const project = useProject.getState();
  useApp.getState().setWorkspace(ws);
  if (d.nodeId) project.select({ kind: "node", id: d.nodeId });
  else if (d.pin) project.select({ kind: "pin", pin: d.pin });
  if (d.file && d.line) useNav.setState({ codeReveal: { file: d.file, line: d.line, seq: ++seq } });
  if (d.section) useNav.setState({ focusSection: { section: d.section, seq: ++seq } });
}
