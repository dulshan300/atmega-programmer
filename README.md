# AVR Studio Visual

A desktop IDE for visually programming classic 8-bit AVR microcontrollers (ATmega328P, ATmega32A): build firmware from connected blocks, configure clock, pins, peripherals and fuses, inspect the generated C, compile with AVR-GCC, upload with AVRDUDE and talk to the board through a serial monitor.

Built with Tauri 2 (Rust backend) and React + TypeScript (frontend). Everything runs locally and offline once the toolchains are installed.

## Status

Work in progress. The Rust core (devices, projects, graph → C code generation, build pipeline, AVRDUDE/fuse workflow, serial) and the IDE frontend are implemented and tested. Still to do: end-to-end UI tests (Playwright), user documentation, and testing on physical hardware and on Windows.

## Layout

| Path | Contents |
|---|---|
| `crates/avr-core` | Tauri-independent domain logic and tests |
| `src-tauri` | Thin Tauri command layer over `avr-core` |
| `src` | React frontend (workspaces, stores, typed IPC) |
| `devices/*/device.json` | Device definitions (pinouts, peripherals, fuses) |
| `blocks/catalog.json` | Visual block catalog shared by frontend and backend |

## Requirements

- Node.js 20+ and Rust 1.90+
- Linux: `libwebkit2gtk-4.1-dev librsvg2-dev` (Tauri prerequisites)
- To build firmware: AVR-GCC (modern, or WinAVR on Windows). To program chips: AVRDUDE 6.x–8.x. Both are auto-detected; paths can be set in Settings.

## Development

```sh
npm install
npm run tauri dev        # run the desktop app
npm run check            # typecheck + lint + frontend tests + Rust core tests
cargo test --workspace   # all Rust tests
```

`crates/avr-core/tests/real_toolchain.rs` compiles every template for both chips with a real AVR-GCC. It uses `AVR_STUDIO_TEST_GCC`, or `.tools/root/usr/bin/avr-gcc` if present, and is skipped when neither exists. `tests/mock_tools.rs` exercises the build, upload and fuse workflows against mock executables.

## Hardware safety

Fuses are only written through a plan → review → confirm → write → read-back flow. Values that would disable ISP or RESET (SPIEN, RSTDISBL, DWEN) are refused. An upload counts as successful only when AVRDUDE reports the flash as verified.
