import type { WorkspaceId } from "@/store/app";
import type { Diagnostic } from "./types";

/** The workspace where a diagnostic can be fixed. */
export function diagnosticWorkspace(d: Diagnostic): WorkspaceId {
  if (d.nodeId) return "visual";
  if (d.file) return "code";
  switch (d.section) {
    case "pins":
    case "device":
      return "pins";
    case "clock":
    case "peripherals":
      return "clock";
    case "fuses":
      return "fuses";
    case "variables":
      return "visual";
    default:
      return d.code.startsWith("graph.") ? "visual" : "build";
  }
}

export function worstSeverity(ds: Diagnostic[]): "error" | "warning" | undefined {
  if (ds.some((x) => x.severity === "error")) return "error";
  if (ds.some((x) => x.severity === "warning")) return "warning";
  return undefined;
}
