import Editor, { type OnMount } from "@monaco-editor/react";
import clsx from "clsx";
import { Copy, FileCode2, FileDown, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button, Callout, EmptyState } from "@/components/ui";
import { DiagnosticRow } from "@/components/shell/BottomPanel";
import { api, dialogs, toAppError } from "@/lib/ipc";
import { defineThemes } from "@/lib/monaco";
import type { PreviewResponse } from "@/lib/types";
import { useApp } from "@/store/app";
import { useNav } from "@/store/nav";
import { useProject } from "@/store/project";

export default function CodeView() {
  const project = useProject((s) => s.project);
  const theme = useApp((s) => s.theme);
  const notify = useApp((s) => s.notify);
  const reveal = useNav((s) => s.codeReveal);
  const [preview, setPreview] = useState<PreviewResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [active, setActive] = useState("src/generated/main.c");
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const seq = useRef(0);

  const refresh = useCallback(async () => {
    const p = useProject.getState().project;
    if (!p) return;
    const mine = ++seq.current;
    setLoading(true);
    try {
      const r = await api.generatePreview(p);
      if (mine === seq.current) {
        setPreview(r);
        setError(null);
      }
    } catch (e) {
      if (mine === seq.current) setError(toAppError(e).message);
    } finally {
      if (mine === seq.current) setLoading(false);
    }
  }, []);

  // Regenerate (debounced) whenever the project changes.
  useEffect(() => {
    const t = setTimeout(() => void refresh(), 300);
    return () => clearTimeout(t);
  }, [project, refresh]);

  const file = preview?.files.find((f) => f.path === active) ?? preview?.files[0];

  // Switch to the file of a newly requested reveal (adjusting state during render).
  const [handledReveal, setHandledReveal] = useState<number | null>(null);
  if (reveal && preview && handledReveal !== reveal.seq) {
    setHandledReveal(reveal.seq);
    const target = preview.files.find((f) => reveal.file.endsWith(f.path) || f.path.endsWith(reveal.file));
    if (target) setActive(target.path);
  }
  useEffect(() => {
    if (!reveal) return;
    const t = setTimeout(() => {
      editorRef.current?.revealLineInCenter(reveal.line);
      editorRef.current?.setPosition({ lineNumber: reveal.line, column: 1 });
    }, 50);
    return () => clearTimeout(t);
  }, [reveal, preview]);

  if (!project) return <EmptyState icon={FileCode2} title="No project open" />;
  const errors = preview?.diagnostics.filter((d) => d.severity === "error") ?? [];

  const saveAs = async () => {
    if (!file) return;
    const name = file.path.split("/").pop() ?? "file.c";
    const path = await dialogs.saveFile("Save generated file", name, [name.endsWith(".h") ? "h" : "c"]);
    if (!path) return;
    try {
      await api.saveTextFile(path, file.content);
      notify("success", `Saved ${path}`);
    } catch (e) {
      notify("error", toAppError(e).message);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-9 shrink-0 items-center gap-1 border-b border-line bg-panel px-2">
        {(preview?.files ?? []).map((f) => (
          <button
            key={f.path}
            type="button"
            onClick={() => setActive(f.path)}
            className={clsx(
              "h-7 rounded px-2 font-mono text-xs",
              file?.path === f.path ? "bg-raised text-text" : "text-muted hover:text-text",
            )}
          >
            {f.path.split("/").pop()}
          </button>
        ))}
        <div className="flex-1" />
        {preview?.legacy && <Badge tone="info">WinAVR / C99 output</Badge>}
        <span className="px-2 text-xs text-faint">Read-only preview · written to src/generated on build</span>
        <Button size="sm" variant="ghost" icon={RefreshCw} busy={loading} onClick={() => void refresh()}>
          Refresh
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={Copy}
          disabled={!file}
          onClick={() =>
            file &&
            void navigator.clipboard
              .writeText(file.content)
              .then(() => notify("success", "Copied to clipboard"))
          }
        >
          Copy
        </Button>
        <Button size="sm" variant="ghost" icon={FileDown} disabled={!file} onClick={() => void saveAs()}>
          Save as…
        </Button>
      </div>
      {error && (
        <div className="p-3">
          <Callout tone="danger" title="Code generation failed">
            {error}
          </Callout>
        </div>
      )}
      {preview && !file && (
        <div className="overflow-auto p-3">
          <Callout tone="warn" title="No code generated">
            Fix the following {errors.length} error(s); code is only generated for a valid project.
          </Callout>
          <div className="mt-2 rounded border border-line bg-panel py-1">
            {errors.map((d, i) => (
              <DiagnosticRow key={i} d={d} />
            ))}
          </div>
        </div>
      )}
      {file && (
        <div className="min-h-0 flex-1">
          <Editor
            path={file.path}
            value={file.content}
            language="c"
            theme={theme === "dark" ? "avsv-dark" : "avsv-light"}
            beforeMount={() => defineThemes()}
            onMount={(ed) => (editorRef.current = ed)}
            options={{
              readOnly: true,
              domReadOnly: true,
              minimap: { enabled: true },
              fontSize: 13,
              fontFamily: "JetBrains Mono, Cascadia Code, Consolas, monospace",
              scrollBeyondLastLine: false,
              renderWhitespace: "selection",
              automaticLayout: true,
            }}
          />
        </div>
      )}
      {!preview && !error && <EmptyState icon={FileCode2} title="Generating…" />}
    </div>
  );
}
