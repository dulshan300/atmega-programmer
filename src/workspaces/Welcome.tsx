import { CheckCircle2, Cpu, FolderOpen, Plus, Trash2, XCircle } from "lucide-react";
import { EMPTY } from "@/lib/hooks";
import { runCommand } from "@/commands";
import { Button, Card, IconButton } from "@/components/ui";
import { formatBytes, formatHz, timeAgo } from "@/lib/format";
import { useApp } from "@/store/app";
import { useProject } from "@/store/project";

function ToolStatus() {
  const tc = useApp((s) => s.toolchain);
  const detecting = useApp((s) => s.detecting);
  const setWorkspace = useApp((s) => s.setWorkspace);
  const row = (ok: boolean, label: string, detail: string) => (
    <div className="flex items-start gap-2 text-xs">
      {ok ? (
        <CheckCircle2 size={14} className="mt-px text-ok" />
      ) : (
        <XCircle size={14} className="mt-px text-danger" />
      )}
      <div>
        <div className="font-medium">{label}</div>
        <div className="selectable break-all text-muted">{detail}</div>
      </div>
    </div>
  );
  return (
    <Card
      title="Toolchain"
      actions={
        <Button size="sm" onClick={() => setWorkspace("settings")}>
          Configure
        </Button>
      }
    >
      {detecting && !tc ? (
        <div className="text-xs text-muted">Detecting installed tools…</div>
      ) : tc ? (
        <div className="flex flex-col gap-2">
          {row(
            !!tc.gcc,
            tc.gcc ? `avr-gcc ${tc.gcc.version}` : "avr-gcc not found",
            tc.gcc?.avrGcc ??
              "Required to build firmware. Install an AVR-GCC toolchain or set its path in Settings.",
          )}
          {row(
            !!tc.avrdude,
            tc.avrdude ? `AVRDUDE ${tc.avrdude.version ?? ""}` : "AVRDUDE not found",
            tc.avrdude?.path ?? "Required to upload firmware and read/write fuses.",
          )}
        </div>
      ) : (
        <div className="text-xs text-muted">Toolchain status unavailable.</div>
      )}
    </Card>
  );
}

export function Welcome() {
  const info = useApp((s) => s.info);
  const recent = useApp((s) => s.settings?.recentProjects ?? EMPTY);
  const removeRecent = useApp((s) => s.removeRecent);
  const open = useProject((s) => s.open);

  return (
    <div className="h-full overflow-auto">
      <div className="mx-auto flex max-w-5xl flex-col gap-6 p-8">
        <div className="flex items-center gap-3">
          <Cpu size={32} className="text-accent" />
          <div>
            <h1 className="text-xl font-semibold">AVR Studio Visual</h1>
            <p className="text-sm text-muted">
              Visual firmware development for classic 8-bit AVR microcontrollers.
            </p>
          </div>
        </div>
        <div className="flex gap-2">
          <Button variant="primary" icon={Plus} onClick={() => runCommand("project.new")}>
            New project
          </Button>
          <Button icon={FolderOpen} onClick={() => runCommand("project.open")}>
            Open project folder
          </Button>
        </div>

        <div className="grid gap-4 lg:grid-cols-[1fr_340px]">
          <Card title="Recent projects">
            {recent.length === 0 ? (
              <div className="py-6 text-center text-xs text-muted">
                Projects you open or create appear here.
              </div>
            ) : (
              <ul className="-mx-3 -my-3 divide-y divide-line">
                {recent.map((r) => (
                  <li key={r.path} className="group flex items-center gap-3 px-3 py-2 hover:bg-raised">
                    <button
                      type="button"
                      className="min-w-0 flex-1 text-left"
                      onClick={() => void open(r.path)}
                    >
                      <div className="truncate text-[13px] font-medium">{r.name}</div>
                      <div className="selectable truncate text-[11px] text-faint">{r.path}</div>
                    </button>
                    <span className="font-mono text-[11px] text-muted">{r.mcu}</span>
                    <span className="w-20 text-right text-[11px] text-faint">{timeAgo(r.openedAtMs)}</span>
                    <IconButton
                      icon={Trash2}
                      label={`Remove ${r.name} from recent projects (files are not deleted)`}
                      className="opacity-0 group-hover:opacity-100 focus:opacity-100"
                      onClick={() => void removeRecent(r.path)}
                    />
                  </li>
                ))}
              </ul>
            )}
          </Card>
          <div className="flex flex-col gap-4">
            <ToolStatus />
            <Card title="Supported devices">
              <ul className="flex flex-col gap-2">
                {(info?.devices ?? []).map((d) => (
                  <li key={d.id} className="text-xs">
                    <div className="font-medium">{d.name}</div>
                    <div className="text-muted">
                      {formatBytes(d.flashBytes)} flash · {formatBytes(d.sramBytes)} SRAM ·{" "}
                      {formatBytes(d.eepromBytes)} EEPROM · up to {formatHz(d.maxFrequencyHz)}
                    </div>
                    <div className="text-faint">{d.packages.join(", ")}</div>
                  </li>
                ))}
              </ul>
            </Card>
          </div>
        </div>
      </div>
    </div>
  );
}
