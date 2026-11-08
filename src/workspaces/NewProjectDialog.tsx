import { FolderOpen } from "lucide-react";
import { useState } from "react";
import { Button, Dialog, Field, Input, Select } from "@/components/ui";
import { dialogs } from "@/lib/ipc";
import type { Template } from "@/lib/types";
import { useApp } from "@/store/app";
import { useProject } from "@/store/project";

const TEMPLATES: { id: Template; label: string; description: string }[] = [
  { id: "blink", label: "Blink", description: "Toggles an LED pin every 500 ms." },
  { id: "uart_echo", label: "UART echo", description: "Sends a greeting, then echoes every received byte." },
  {
    id: "adc_reading",
    label: "ADC reading",
    description: "Reads ADC channel 0 and prints millivolts over UART.",
  },
  { id: "empty", label: "Empty", description: "Only the Program Start block." },
];

/** Mirrors the backend's project-name rule so the user gets feedback while typing. */
export function projectNameError(name: string): string | null {
  const n = name.trim();
  if (!n) return "Enter a project name";
  if ([...n].length > 64) return "At most 64 characters";
  if (/[/\\:*?"<>|]/.test(n) || [...n].some((c) => c.charCodeAt(0) < 32))
    return 'Must not contain / \\ : * ? " < > | or control characters';
  if (n === "." || n === "..") return "Not a valid name";
  return null;
}

export function NewProjectDialog() {
  const open = useApp((s) => s.newProjectOpen);
  const setOpen = useApp((s) => s.setNewProjectOpen);
  const devices = useApp((s) => s.info?.devices ?? []);
  const create = useProject((s) => s.create);

  const [name, setName] = useState("Blink");
  const [mcu, setMcu] = useState("atmega328p");
  const [pkgChoice, setPkg] = useState("");
  const [template, setTemplate] = useState<Template>("blink");
  const [parentDir, setParentDir] = useState("");
  const [busy, setBusy] = useState(false);

  const device = devices.find((d) => d.id === mcu) ?? devices[0];
  // Falls back to the device's first package when the choice does not apply.
  const pkg = device?.packages.includes(pkgChoice) ? pkgChoice : (device?.packages[0] ?? "");

  const nameError = projectNameError(name);
  const canCreate = !nameError && !!parentDir && !!device && !!pkg && !busy;

  const submit = async () => {
    if (!canCreate || !device) return;
    setBusy(true);
    const ok = await create({ parentDir, name: name.trim(), mcu: device.id, package: pkg, template });
    setBusy(false);
    if (ok) setOpen(false);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      title="New project"
      description="Creates a project folder with a versioned project.json and configuration files."
      width={560}
      footer={
        <>
          <Button onClick={() => setOpen(false)}>Cancel</Button>
          <Button variant="primary" disabled={!canCreate} busy={busy} onClick={() => void submit()}>
            Create project
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-2 gap-3">
        <Field label="Project name" error={name ? nameError : undefined} className="col-span-2">
          <Input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            aria-invalid={!!nameError}
          />
        </Field>
        <Field
          label="Location"
          hint={parentDir ? undefined : "The project folder is created inside this folder."}
          className="col-span-2"
        >
          <div className="flex gap-2">
            <Input readOnly value={parentDir} placeholder="Choose a parent folder…" />
            <Button
              icon={FolderOpen}
              onClick={async () => {
                const p = await dialogs.pickFolder("Choose where to create the project");
                if (p) setParentDir(p);
              }}
            >
              Browse
            </Button>
          </div>
        </Field>
        <Field label="Microcontroller">
          <Select
            value={device?.id ?? ""}
            onChange={setMcu}
            options={devices.map((d) => ({ value: d.id, label: d.name }))}
          />
        </Field>
        <Field label="Package">
          <Select
            value={pkg}
            onChange={setPkg}
            options={(device?.packages ?? []).map((p) => ({ value: p, label: p }))}
          />
        </Field>
        <fieldset className="col-span-2">
          <legend className="mb-1 text-xs font-medium text-muted">Starting template</legend>
          <div className="grid grid-cols-2 gap-2">
            {TEMPLATES.map((t) => (
              <label
                key={t.id}
                className="flex cursor-pointer items-start gap-2 rounded border border-line p-2 hover:border-line-strong has-[:checked]:border-accent has-[:checked]:bg-accent/10"
              >
                <input
                  type="radio"
                  name="template"
                  className="mt-0.5 accent-[var(--accent)]"
                  checked={template === t.id}
                  onChange={() => setTemplate(t.id)}
                />
                <span>
                  <span className="block text-[13px] font-medium">{t.label}</span>
                  <span className="block text-xs text-muted">{t.description}</span>
                </span>
              </label>
            ))}
          </div>
        </fieldset>
        <p className="col-span-2 text-xs text-faint">
          New projects start with the chip's factory clock (
          {device?.id === "atmega328p" ? "8 MHz internal RC ÷ 8 = 1 MHz" : "1 MHz internal RC"}). Change the
          clock, toolchain profile and programmer later in the project workspaces.
        </p>
      </div>
    </Dialog>
  );
}
