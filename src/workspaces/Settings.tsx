import { CheckCircle2, FolderOpen, Moon, Plus, RefreshCw, Sun, Trash2, XCircle } from "lucide-react";
import { useSyncedState } from "@/lib/hooks";
import { Button, Callout, Card, Field, IconButton, Input } from "@/components/ui";
import { dialogs, toAppError } from "@/lib/ipc";
import type { ToolchainSettings } from "@/lib/types";
import { useApp } from "@/store/app";

function PathField({
  label,
  hint,
  value,
  onChange,
  pick,
}: {
  label: string;
  hint: string;
  value: string | null;
  onChange: (v: string | null) => void;
  pick: () => Promise<string | null>;
}) {
  return (
    <Field label={label} hint={hint}>
      <div className="flex gap-1">
        <Input
          value={value ?? ""}
          placeholder="Auto-detect"
          className="font-mono"
          onChange={(e) => onChange(e.target.value.trim() || null)}
        />
        <Button
          icon={FolderOpen}
          aria-label={`Browse for ${label}`}
          onClick={async () => {
            const p = await pick();
            if (p) onChange(p);
          }}
        />
        <IconButton
          icon={XCircle}
          label="Clear (auto-detect)"
          disabled={!value}
          onClick={() => onChange(null)}
        />
      </div>
    </Field>
  );
}

export default function Settings() {
  const settings = useApp((s) => s.settings);
  const status = useApp((s) => s.toolchain);
  const detecting = useApp((s) => s.detecting);
  const detect = useApp((s) => s.detectToolchains);
  const save = useApp((s) => s.saveToolchainSettings);
  const theme = useApp((s) => s.theme);
  const setTheme = useApp((s) => s.setTheme);
  const notify = useApp((s) => s.notify);
  const info = useApp((s) => s.info);
  const [draft, setDraft] = useSyncedState<ToolchainSettings | null>(settings?.toolchain ?? null);
  const dirty = JSON.stringify(draft) !== JSON.stringify(settings?.toolchain ?? null);

  const exe = (name: string) => () =>
    dialogs.pickFile(`Locate ${name}`, info?.os === "windows" ? ["exe"] : undefined);

  return (
    <div className="h-full overflow-auto">
      <div className="flex max-w-4xl flex-col gap-4 p-4">
        <Card
          title="Toolchains"
          actions={
            <>
              <Button size="sm" icon={RefreshCw} busy={detecting} onClick={() => void detect()}>
                Re-detect
              </Button>
              <Button
                size="sm"
                variant="primary"
                disabled={!dirty || !draft}
                onClick={async () => {
                  if (!draft) return;
                  try {
                    await save(draft);
                    notify("success", "Toolchain settings saved");
                  } catch (e) {
                    notify("error", toAppError(e).message);
                  }
                }}
              >
                Save
              </Button>
            </>
          }
        >
          {draft ? (
            <div className="flex flex-col gap-3">
              <PathField
                label="avr-gcc"
                hint="avr-objcopy and avr-size are taken from the same folder. WinAVR (e.g. C:\WinAVR-20100110\bin\avr-gcc.exe) uses the legacy C99 profile."
                value={draft.avrGccPath}
                onChange={(v) => setDraft({ ...draft, avrGccPath: v })}
                pick={exe("avr-gcc")}
              />
              <PathField
                label="avrdude"
                hint="AVRDUDE 6.x–8.x."
                value={draft.avrdudePath}
                onChange={(v) => setDraft({ ...draft, avrdudePath: v })}
                pick={exe("avrdude")}
              />
              <PathField
                label="avrdude.conf"
                hint="Only needed when AVRDUDE cannot find its configuration file."
                value={draft.avrdudeConfPath}
                onChange={(v) => setDraft({ ...draft, avrdudeConfPath: v })}
                pick={() => dialogs.pickFile("Locate avrdude.conf", ["conf"])}
              />
              <Field
                label="Extra search folders"
                hint="Searched before the well-known install locations and PATH."
              >
                <div className="flex flex-col gap-1">
                  {draft.extraSearchDirs.map((d, i) => (
                    <div key={d} className="flex items-center gap-1">
                      <Input readOnly value={d} className="font-mono" />
                      <IconButton
                        icon={Trash2}
                        label="Remove folder"
                        onClick={() =>
                          setDraft({
                            ...draft,
                            extraSearchDirs: draft.extraSearchDirs.filter((_, j) => j !== i),
                          })
                        }
                      />
                    </div>
                  ))}
                  <Button
                    size="sm"
                    icon={Plus}
                    className="self-start"
                    onClick={async () => {
                      const p = await dialogs.pickFolder("Add toolchain folder");
                      if (p && !draft.extraSearchDirs.includes(p))
                        setDraft({ ...draft, extraSearchDirs: [...draft.extraSearchDirs, p] });
                    }}
                  >
                    Add folder
                  </Button>
                </div>
              </Field>
            </div>
          ) : (
            <div className="text-xs text-muted">Settings unavailable.</div>
          )}
        </Card>

        <Card title="Detected">
          {!status ? (
            <div className="text-xs text-muted">{detecting ? "Detecting…" : "Not detected yet."}</div>
          ) : (
            <div className="flex flex-col gap-2 text-xs">
              <div className="flex items-start gap-2">
                {status.gcc ? (
                  <CheckCircle2 size={14} className="text-ok" />
                ) : (
                  <XCircle size={14} className="text-danger" />
                )}
                <div className="selectable">
                  {status.gcc ? (
                    <>
                      avr-gcc {status.gcc.version} · profile {status.gcc.detectedProfile.replace("_", " ")}
                      <div className="font-mono text-faint">{status.gcc.avrGcc}</div>
                    </>
                  ) : (
                    "avr-gcc not found"
                  )}
                </div>
              </div>
              <div className="flex items-start gap-2">
                {status.avrdude ? (
                  <CheckCircle2 size={14} className="text-ok" />
                ) : (
                  <XCircle size={14} className="text-danger" />
                )}
                <div className="selectable">
                  {status.avrdude ? (
                    <>
                      AVRDUDE {status.avrdude.version ?? "(version unknown)"}
                      <div className="font-mono text-faint">{status.avrdude.path}</div>
                      {status.avrdude.conf && (
                        <div className="font-mono text-faint">-C {status.avrdude.conf}</div>
                      )}
                    </>
                  ) : (
                    "AVRDUDE not found"
                  )}
                </div>
              </div>
              {status.problems.length > 0 && (
                <Callout tone="warn" title="Notes">
                  <ul className="list-disc pl-4">
                    {status.problems.map((p) => (
                      <li key={p}>{p}</li>
                    ))}
                  </ul>
                </Callout>
              )}
              <details className="text-faint">
                <summary className="cursor-pointer">Searched folders ({status.searchedDirs.length})</summary>
                <ul className="selectable mt-1 font-mono">
                  {status.searchedDirs.map((d) => (
                    <li key={d}>{d}</li>
                  ))}
                </ul>
              </details>
            </div>
          )}
        </Card>

        <Card title="Appearance">
          <div className="flex gap-2">
            <Button
              variant={theme === "dark" ? "primary" : "secondary"}
              icon={Moon}
              onClick={() => setTheme("dark")}
            >
              Dark
            </Button>
            <Button
              variant={theme === "light" ? "primary" : "secondary"}
              icon={Sun}
              onClick={() => setTheme("light")}
            >
              Light
            </Button>
          </div>
        </Card>

        <div className="text-xs text-faint">
          AVR Studio Visual {info?.version} · {info?.os}
        </div>
      </div>
    </div>
  );
}
