# 2026-11-17 — Electronics Firmware Flow, Board Detection & MicroPython

## Summary

Reworked the Electronics `Firmware` subtab into a coherent Hardware → Firmware Project → Toolchain flow, removed the hardcoded palette from its toolchain modal and serial monitor, added USB-based board suggestion, and added MicroPython as a first-class development language with its own scaffold and `mpremote` upload path.

## Changes Implemented

### Flow and UI (`ProjectElectronicsTab.vue`)

- **Hardware card:** detected serial port, baud rate, port metadata, PlatformIO board catalog search/list, a board suggestion derived from the USB device (`productName`, `description`, `manufacturer`, VID), and an `Apply` action when the suggestion differs from the current board. The port description (e.g. NodeMCU V2) is no longer presented as the microcontroller choice.
- **Firmware Project card:** language selector `C++ | C | Python` (`AppSegmentedToggle`), framework (hidden for MicroPython), monitor speed, and `Scaffold … Project`. The card reports an existing project instead of duplicating the scaffold form.
- **Toolchain & Operations card:** environment (PlatformIO only), compile/upload actions, `mpremote` warning for MicroPython, operation result and compiler log.
- Serial monitor moved to `ui-surface-title`, `.ui-icon-btn` pause/clear, semantic status colors and the shared dark terminal surface (`#09090b`).
- Toolchain modal rewritten with semantic status tokens, semantic install buttons, copyable non-pacman hints, and an install-all restricted to pacman-backed tools.
- The `Wrench` header action uses `.ui-icon-btn` with ok/warn border states.

### Backend

- `ProjectElectronicsStatus` now exposes `firmware_language` (`platformio`, `micropython`, or `null`), resolved from `rula.toml` and the firmware directory layout.
- `scaffold_firmware_project` accepts `language = micropython`: writes `main.py`, `boot.py`, and `README.md` for ESP32/ESP8266/RP2040/STM32 boards, records `[plugins.electronics].firmware_language = "micropython"` and keeps `__pycache__/` in `.gitignore`. C/C++ scaffolds record `firmware_language = "platformio"`.
- `flash_project_firmware` uploads MicroPython projects with `mpremote connect <port> fs cp main.py :main.py + fs cp boot.py :boot.py + soft-reset`; `build_project_firmware` returns a typed "no compile step" error for MicroPython.
- `check_electronics_toolchain` adds `mpremote` (six tools total) with a pip install hint; pacman-backed ids remain installable through `install_missing_tools`.

## Specs Updated

- `31 §5.1/§5.2` — actual six-tool list, three-card flow, language/MicroPython scaffold and upload contract.
- `13 §3.13` — Electronics Firmware subtab contract.

## Limitations

- In-app MicroPython upload copies `main.py` and `boot.py` only; the MicroPython runtime itself must be flashed once with `esptool`.
- `mpremote` is a pip package, so its toolchain row is copy-only.
- Oscilloscope and Logic Analyzer chrome is still pending.

## Verification

`./scripts/run_dev.sh export-bindings` passed (376 tests) and the watcher rebuilt the daemon. No manual build or runtime process was started by the agent. Pending UI check: board suggestion on a real port, C/C++ scaffold, MicroPython scaffold + upload with `mpremote` installed, and the toolchain modal.
