// Renders shell parts with an empty store (no backend, no settings yet).
// Guards against selectors that return a fresh object on every call, which
// makes Zustand re-render forever and blanks the whole window.
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Explorer } from "@/components/shell/Explorer";
import { CommandPalette } from "@/components/shell/Overlays";
import { StatusBar } from "@/components/shell/StatusBar";
import { useApp } from "@/store/app";
import { NewProjectDialog } from "./NewProjectDialog";
import { Welcome } from "./Welcome";

describe("shell renders without backend data", () => {
  it("welcome, explorer and status bar", () => {
    render(
      <>
        <Explorer />
        <Welcome />
        <StatusBar />
      </>,
    );
    expect(screen.getByText("No project open", { selector: "div" })).toBeInTheDocument();
    expect(screen.getByText("New project")).toBeInTheDocument();
  });

  it("new project dialog and command palette", () => {
    useApp.setState({ newProjectOpen: true, paletteOpen: true });
    render(
      <>
        <NewProjectDialog />
        <CommandPalette />
      </>,
    );
    expect(screen.getByRole("dialog", { name: "New project" })).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/Type a command/)).toBeInTheDocument();
  });
});
