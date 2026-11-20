---
name: oneship-profiler-crafter
description: Guide and operational protocol for diagnosing project stacks, crafting declarative profiler engine manifests (manifest.toml), and resolving initialization flags.
version: "0.2.6"
---

# ONE-SHIP Profiler Crafter Protocol

This skill guides AI agents in diagnosing unknown or unconfigured runtime stacks, authoring declarative profiler engine manifests (`manifest.toml`), formulating safe installation recipes, and resolving initialization flags.

---

## 1. Core Principles

1. **Stack Independence:** Never assume a specific backend or frontend framework. Diagnose the project's actual runtime by inspecting repository trigger files (`Cargo.toml`, `package.json`, `mix.exs`, `pyproject.toml`, `go.mod`, `Gemfile`, `pom.xml`, etc.).
2. **Declarative Recipes Over Recompilation:** Engines are added as declarative manifests (`manifest.toml`) containing a CLI command template. Zero Rust or TypeScript code is compiled.
3. **Continuous, Run-Bound Profiling:** ONE-SHIP already samples CPU, memory, threads and disk I/O of every service and task run, for the whole run. An engine adds code-level detail: a service bound to it (`[scripts.persistent.<id>] profiler = "<engine_id>"`) is launched wrapped by the engine for its whole run, and the engine's output file is linked to the run. There are no on-demand or fixed-duration sessions.
4. **Wrapper Contract:** The template must launch the service command itself (`... -- {original_command}`) and the engine must write its output file when it receives `SIGINT`, which is how ONE-SHIP stops a run (followed by `SIGKILL` after `profiler.engine_shutdown_grace_period_ms`). A tool that has to be the runtime process itself (e.g. `node --cpu-prof`) cannot wrap a launcher such as `npm run dev`.
5. **Interactive Authorization:** Always present the proposed `manifest.toml` and installation requirements to the user via `request_approval` before writing to disk.

---

## 2. Diagnostics & Stack Detection Matrix

| Detected Files / Binaries | Target Runtime | Recommended Engine | Common Formats |
| :--- | :--- | :--- | :--- |
| `pyproject.toml`, `requirements.txt`, `*.py` | Python | `py-spy`, `austin` | Flamegraph SVG, Speedscope |
| `package.json`, `tsconfig.json`, `*.ts`, `*.js` | Node.js / Bun / Deno | `0x`, V8 `--cpu-prof` (only when the command is `node` itself) | Flamegraph, `.cpuprofile` |
| `go.mod`, `*.go` | Go | `go tool pprof` | SVG Flamegraph, Pprof Proto |
| `Cargo.toml`, `*.rs` | Rust | `samply` (built in), `cargo-flamegraph` | Firefox Profile, SVG |
| `mix.exs`, `*.ex`, `*.exs` | Elixir / Erlang (BEAM) | `eprof`, `fprof` | Text digest, Flamegraph |
| `Gemfile`, `*.rb` | Ruby | `rbspy`, `vernier` | Speedscope, Flamegraph SVG |
| `pom.xml`, `build.gradle` | JVM (Java/Kotlin/Scala) | `async-profiler` | Flamegraph HTML/SVG |
| Compiled binaries (`*.c`, `*.cpp`, `*.zig`) | Native Systems | `samply` (built in) | Firefox Profile |

---

## 3. Authoring the Manifest (`manifest.toml`)

Create the plugin directory under:
* **Global (Recommended for general stacks):** `~/.config/one-ship/plugins/profiler-<stack_id>/manifest.toml`
* **Project Local (For bespoke / repo-specific pipelines):** `<project_root>/.one-ship/plugins/profiler-<stack_id>/manifest.toml`

A project manifest shadows a global one with the same `id`, which shadows a built-in one.

### Standard Manifest Template

The manifest is parsed strictly: an unknown key or a missing section makes it invalid. Use exactly these sections and keys.

```toml
[plugin]
id = "<engine_id>"            # e.g. "py-spy", "rbspy"; what rula.toml references
name = "<Human Friendly Name>"
version = "<tool version>"
type = "profiler"
author = "ONE-SHIP Agent"     # optional
description = "Sampling profiler for <Runtime/Framework>"

[detection]                   # optional
file_triggers = ["<config_file_1>", "*.<ext>"]
binary_triggers = ["<binary_1>"]

[requirements]
check_command = "<tool_binary> --version"   # must exit 0 when installed
install_hint = "<one-line install command>" # optional

[requirements.install]        # optional; only these keys
arch = "pacman -S --noconfirm <pkg>"
debian = "apt-get install -y <pkg>"
pip = "pip install <pkg>"
cargo = "cargo install <pkg>"
npm = "npm install -g <pkg>"
script = "scripts/install_<engine_id>.sh"
requires_sudo = false

[execution.defaults]
sampling_rate_hz = 99
format = "flamegraph"

[execution.modes.wrapper]
command_template = "<tool_binary> record --rate {sampling_rate_hz} --format {format} --output {output_file} -- {original_command}"

[artifacts]
visual_extension = "svg"
viewer_type = "flamegraph"    # flamegraph | speedscope | firefox_profiler | text
```

Template tokens: `{original_command}` and `{output_file}` are required; `{sampling_rate_hz}` and `{format}` take the `[execution.defaults]` values. `{output_file}` is `<project>/scratchpad/profiling/<run_id>.<visual_extension>`.

---

## 4. Resolving Initialization Flags

When resolving an initialization flag for a missing profiler:

1. **Craft Manifest:** Write the approved `manifest.toml` to the target plugins directory.
2. **Verify Tool Installation:** Run its `check_command` with `run_command`. If it fails, propose the installation recipe to the user.
3. **Verify the Manifest:** Ask the user to open the Profiler tab: the engine must be listed, and must not appear under the manifest faults.
4. **Bind the Engine in `rula.toml`:** Add `profiler = "<engine_id>"` (a string, nothing else) under `[scripts.persistent.<id>]` in `<project_root>/rula.toml`. From the next start of the service, each run is wrapped by the engine and keeps its output file.
5. **Resolve Initialization Flag:** Invoke the MCP tool `resolve_initialization_flag`:
   ```json
   {
     "flag_id": 1,
     "status": "resolved"
   }
   ```
6. **Log Minutes:** Record the newly provisioned capability in session minutes via `record_session_minutes`.
