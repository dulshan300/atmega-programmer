import clsx from "clsx";
import { AlertCircle, AlertTriangle, Cpu, Hammer, Loader2, Plug, Wrench } from "lucide-react";
import { formatHz } from "@/lib/format";
import { useApp } from "@/store/app";
import { useBuild } from "@/store/build";
import { useProgrammer } from "@/store/programmer";
import { diagnosticCounts, useProject } from "@/store/project";
import { useSerial } from "@/store/serial";

function Item({
  children,
  onClick,
  title,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  title?: string;
}) {
  const cls = "flex h-full items-center gap-1 px-2 text-[11px] whitespace-nowrap";
  return onClick ? (
    <button type="button" className={clsx(cls, "hover:bg-raised")} onClick={onClick} title={title}>
      {children}
    </button>
  ) : (
    <span className={cls} title={title}>
      {children}
    </span>
  );
}

const STAGE_LABEL = {
  validate: "Validating",
  toolchain: "Checking toolchain",
  generate: "Generating",
  compile: "Compiling",
  size: "Analyzing",
  hex: "Creating HEX",
  done: "Done",
};

export function StatusBar() {
  const project = useProject((s) => s.project);
  const device = useProject((s) => s.device);
  const diags = useProject((s) => s.diagnostics);
  const validating = useProject((s) => s.validating);
  const toolchain = useApp((s) => s.toolchain);
  const detecting = useApp((s) => s.detecting);
  const programmers = useApp((s) => s.programmers);
  const setWorkspace = useApp((s) => s.setWorkspace);
  const showBottomTab = useApp((s) => s.showBottomTab);
  const running = useBuild((s) => s.running);
  const stage = useBuild((s) => s.stage);
  const report = useBuild((s) => s.report);
  const hw = useProgrammer((s) => s.busy);
  const serial = useSerial((s) => s.status);
  const serialPort = useSerial((s) => s.config.port);
  const { errors, warnings } = diagnosticCounts(diags);

  const prog = project ? programmers.find((p) => p.id === project.manifest.programmer.programmer) : undefined;
  const fcpu = project
    ? project.clock.ckdiv8
      ? project.clock.sourceFrequencyHz / 8
      : project.clock.sourceFrequencyHz
    : 0;

  let buildState: React.ReactNode = "Not built";
  if (running)
    buildState = (
      <>
        <Loader2 size={11} className="animate-spin" /> {stage ? STAGE_LABEL[stage] : "Building"}…
      </>
    );
  else if (report)
    buildState = report.success ? (
      <span className="text-ok">Build OK</span>
    ) : (
      <span className="text-danger">Build failed</span>
    );

  return (
    <footer
      className="flex h-6 shrink-0 items-stretch border-t border-line bg-panel text-muted"
      aria-label="Status bar"
    >
      {project ? (
        <>
          <Item onClick={() => setWorkspace("pins")} title="Selected MCU, package and CPU clock">
            <Cpu size={11} /> {device?.name ?? project.manifest.device.mcu} ·{" "}
            {project.manifest.device.package} · {formatHz(fcpu)}
          </Item>
          <Item onClick={() => showBottomTab("problems")} title="Problems">
            <AlertCircle size={11} className={errors ? "text-danger" : undefined} /> {errors}
            <AlertTriangle size={11} className={clsx("ml-1", warnings ? "text-warn" : undefined)} />{" "}
            {warnings}
            {validating && <Loader2 size={10} className="ml-1 animate-spin" />}
          </Item>
          <Item onClick={() => showBottomTab("output")} title="Build state">
            <Hammer size={11} /> {buildState}
          </Item>
        </>
      ) : (
        <Item>No project open</Item>
      )}
      <div className="flex-1" />
      {hw && (
        <Item>
          <Loader2 size={11} className="animate-spin" /> Programmer: {hw.replace("-", " ")}
        </Item>
      )}
      <Item onClick={() => setWorkspace("serial")} title="Serial monitor">
        <Plug size={11} className={serial === "connected" ? "text-ok" : undefined} />
        {serial === "connected" ? serialPort : "Serial closed"}
      </Item>
      {project && (
        <Item onClick={() => setWorkspace("programmer")} title="Programmer">
          {prog?.label ?? project.manifest.programmer.programmer}
          {project.manifest.programmer.port && project.manifest.programmer.port !== "usb"
            ? ` @ ${project.manifest.programmer.port}`
            : ""}
        </Item>
      )}
      <Item onClick={() => setWorkspace("settings")} title="Toolchain">
        <Wrench size={11} className={toolchain && !toolchain.gcc ? "text-danger" : undefined} />
        {detecting ? "Detecting…" : toolchain?.gcc ? `avr-gcc ${toolchain.gcc.version}` : "avr-gcc not found"}
        {toolchain &&
          (toolchain.avrdude ? ` · avrdude ${toolchain.avrdude.version ?? "?"}` : " · avrdude not found")}
      </Item>
    </footer>
  );
}
