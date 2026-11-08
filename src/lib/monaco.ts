// Bundles Monaco locally (no CDN), so the editor works offline and under the
// app's Content-Security-Policy. Only the C language and the base editor
// worker are included.
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor/editor";
import "monaco-editor/languages/definitions/cpp/register";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";

self.MonacoEnvironment = { getWorker: () => new EditorWorker() };
loader.config({ monaco });

/** Defines Monaco themes from the app's CSS tokens. */
export function defineThemes() {
  for (const [id, base] of [
    ["avsv-dark", "vs-dark"],
    ["avsv-light", "vs"],
  ] as const) {
    monaco.editor.defineTheme(id, {
      base,
      inherit: true,
      rules: [],
      colors:
        id === "avsv-dark"
          ? { "editor.background": "#0b0f14", "editor.lineHighlightBackground": "#161c25" }
          : { "editor.background": "#ffffff" },
    });
  }
}

export { monaco };
