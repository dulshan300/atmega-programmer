import { create } from "zustand";

export interface TextPrompt {
  title: string;
  label: string;
  hint?: string;
  initial?: string;
  validate?: (v: string) => string | null;
}

interface PromptState {
  current: (TextPrompt & { resolve: (v: string | null) => void }) | null;
}

export const usePrompt = create<PromptState>()(() => ({ current: null }));

/** Shows a modal text prompt. Resolves to the trimmed value, or null if cancelled. */
export function askText(p: TextPrompt): Promise<string | null> {
  usePrompt.getState().current?.resolve(null);
  return new Promise((resolve) => {
    usePrompt.setState({
      current: {
        ...p,
        resolve: (v) => {
          usePrompt.setState({ current: null });
          resolve(v === null ? null : v.trim() || null);
        },
      },
    });
  });
}
