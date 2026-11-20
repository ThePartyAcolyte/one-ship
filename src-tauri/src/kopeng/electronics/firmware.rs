use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ts_rs::TS;
use crate::kopeng::electronics::errors::ElectronicsError;

/// Result summary of a firmware compilation or upload operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FirmwareOpResult {
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

/// Metadata for a board supported by PlatformIO.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlatformioBoard {
    pub id: String,
    pub name: String,
    pub platform: String,
    pub mcu: String,
    pub vendor: String,
    pub frameworks: Vec<String>,
}

/// Arguments for querying supported PlatformIO boards.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ListPlatformioBoardsArgs {
    pub query: Option<String>,
}

/// Arguments for scaffolding a new PlatformIO firmware project.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScaffoldFirmwareProjectArgs {
    pub project_path: String,
    pub board_id: String,
    pub platform: Option<String>,
    pub framework: String,
    pub monitor_speed: u32,
    pub language: Option<String>,
}

/// Result of firmware scaffolding.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ScaffoldFirmwareResult {
    pub success: bool,
    pub environment: String,
    pub created_files: Vec<String>,
}

static BOARDS_CACHE: OnceLock<RwLock<Option<Vec<PlatformioBoard>>>> = OnceLock::new();

fn boards_lock() -> &'static RwLock<Option<Vec<PlatformioBoard>>> {
    BOARDS_CACHE.get_or_init(|| RwLock::new(None))
}

/// Retrieves the catalog of supported PlatformIO boards, optionally filtered by a query string.
pub async fn list_platformio_boards(
    query: Option<String>,
) -> Result<Vec<PlatformioBoard>, ElectronicsError> {
    // 1. Check if already cached
    {
        let r = boards_lock().read().await;
        if let Some(ref cached) = *r {
            return Ok(filter_boards(cached, query.as_deref()));
        }
    }

    // 2. Load from `pio boards --json-output`
    let mut w = boards_lock().write().await;
    if let Some(ref cached) = *w {
        return Ok(filter_boards(cached, query.as_deref()));
    }

    let out = tokio::process::Command::new("pio")
        .args(["boards", "--json-output"])
        .output()
        .await
        .map_err(|e| ElectronicsError::ToolchainMissing {
            tool: "pio".to_string(),
            hint: format!("Failed to execute 'pio boards --json-output': {}", e),
        })?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(ElectronicsError::Io(format!(
            "'pio boards --json-output' exited with status {}: {}",
            out.status, stderr
        )));
    }

    let loaded = serde_json::from_slice::<Vec<PlatformioBoard>>(&out.stdout)
        .map_err(|e| ElectronicsError::Io(format!("Failed to parse 'pio boards --json-output': {}", e)))?;

    *w = Some(loaded.clone());
    Ok(filter_boards(&loaded, query.as_deref()))
}

fn filter_boards(boards: &[PlatformioBoard], query: Option<&str>) -> Vec<PlatformioBoard> {
    let Some(q) = query.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) else {
        return boards.iter().take(100).cloned().collect();
    };

    boards
        .iter()
        .filter(|b| {
            b.id.to_lowercase().contains(&q)
                || b.name.to_lowercase().contains(&q)
                || b.mcu.to_lowercase().contains(&q)
                || b.vendor.to_lowercase().contains(&q)
                || b.platform.to_lowercase().contains(&q)
        })
        .take(150)
        .cloned()
        .collect()
}

/// Appends `__pycache__/` to the project `.gitignore` when it is missing.
async fn append_python_gitignore(p_path: &Path) -> Result<(), ElectronicsError> {
    let gitignore_path = p_path.join(".gitignore");
    let existing = if gitignore_path.exists() {
        tokio::fs::read_to_string(&gitignore_path)
            .await
            .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to read .gitignore: {}", e)))?
    } else {
        String::new()
    };
    if existing.contains("__pycache__") {
        return Ok(());
    }
    let updated = if existing.trim().is_empty() {
        "# MicroPython caches\n__pycache__/\n".to_string()
    } else {
        format!("{}\n\n# MicroPython caches\n__pycache__/\n", existing.trim_end())
    };
    tokio::fs::write(&gitignore_path, updated)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to update .gitignore: {}", e)))
}

/// Ensures `[plugins.electronics]` stays enabled and records the firmware language in `rula.toml`.
async fn upsert_electronics_settings(p_path: &Path, firmware_language: &str) -> Result<(), ElectronicsError> {
    let rula_path = p_path.join("rula.toml");
    if !rula_path.exists() {
        return Ok(());
    }
    let mut val = crate::config::read_project_rula(p_path)
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Invalid rula.toml: {}", e)))?
        .unwrap_or_else(|| toml::Value::Table(Default::default()));
    if let Some(table) = val.as_table_mut() {
        let plugins = table
            .entry("plugins".to_string())
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let Some(plugins_table) = plugins.as_table_mut() {
            let elec = plugins_table
                .entry("electronics".to_string())
                .or_insert_with(|| toml::Value::Table(Default::default()));
            if let Some(elec_table) = elec.as_table_mut() {
                elec_table.insert("enabled".to_string(), toml::Value::Boolean(true));
                elec_table.insert(
                    "firmware_language".to_string(),
                    toml::Value::String(firmware_language.to_string()),
                );
            }
        }
    }
    let toml_str = toml::to_string_pretty(&val)
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to serialize rula.toml: {}", e)))?;
    tokio::fs::write(&rula_path, toml_str)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write rula.toml: {}", e)))
}

/// Scaffolds a complete PlatformIO firmware project inside `project_path`.
pub async fn scaffold_firmware_project(
    args: ScaffoldFirmwareProjectArgs,
) -> Result<ScaffoldFirmwareResult, ElectronicsError> {
    let p_path = Path::new(&args.project_path);
    if !p_path.exists() {
        return Err(ElectronicsError::UnregisteredProject(args.project_path));
    }

    // 1. Resolve Platform
    let platform = if let Some(ref p) = args.platform {
        p.clone()
    } else {
        // Query boards catalog or infer from explicit board family
        let boards = list_platformio_boards(Some(args.board_id.clone())).await?;
        if let Some(matched) = boards.iter().find(|b| b.id.eq_ignore_ascii_case(&args.board_id)) {
            matched.platform.clone()
        } else if args.board_id.contains("esp32") {
            "espressif32".to_string()
        } else if args.board_id.contains("esp8266") || args.board_id == "d1_mini" {
            "espressif8266".to_string()
        } else if args.board_id.contains("pico") || args.board_id.contains("rp2040") {
            "raspberrypi".to_string()
        } else if args.board_id.contains("stm32") || args.board_id.contains("bluepill") || args.board_id.contains("nucleo") {
            "ststm32".to_string()
        } else if args.board_id == "uno" || args.board_id.contains("atmega") {
            "atmelavr".to_string()
        } else {
            return Err(ElectronicsError::InvalidSetting {
                key: "platform",
                details: format!("Cannot infer PlatformIO platform for unknown board '{}'; specify 'platform' explicitly", args.board_id),
            });
        }
    };

    let env_name = args.board_id.replace('-', "_");
    let mut created_files = Vec::new();
    let (fw_dir, _) = super::resolve_electronics_firmware_dir(p_path)?;
    tokio::fs::create_dir_all(&fw_dir)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to create firmware dir {}: {}", fw_dir.display(), e)))?;

    // ── MicroPython scaffold: Python sources, no PlatformIO project ──
    if args.language.as_deref() == Some("micropython") {
        let platform = match args.platform.clone().filter(|p| !p.trim().is_empty()) {
            Some(p) => p,
            None => {
                let boards = list_platformio_boards(Some(args.board_id.clone())).await?;
                boards
                    .iter()
                    .find(|b| b.id.eq_ignore_ascii_case(&args.board_id))
                    .map(|b| b.platform.clone())
                    .unwrap_or_default()
            }
        };
        if !matches!(platform.as_str(), "espressif32" | "espressif8266" | "raspberrypi" | "ststm32") {
            return Err(ElectronicsError::InvalidSetting {
                key: "language",
                details: format!(
                    "MicroPython scaffolding supports ESP32, ESP8266, RP2040 and STM32 boards; board '{}' resolves to platform '{}'",
                    args.board_id, platform
                ),
            });
        }

        let main_path = fw_dir.join("main.py");
        if !main_path.exists() {
            let main_code = format!(
                "\"\"\"ONE-SHIP MicroPython Boilerplate\n\
                 Target Board: {board}\n\
                 Runtime: MicroPython\n\
                 \"\"\"\n\
                 import time\n\
                 from machine import Pin\n\n\
                 LED_PIN = 2\n\
                 led = Pin(LED_PIN, Pin.OUT)\n\n\
                 print(\"[ONE-SHIP] MicroPython initialized on {board}\")\n\n\
                 while True:\n\
                     led.value(not led.value())\n\
                     time.sleep(0.5)\n",
                board = args.board_id
            );
            tokio::fs::write(&main_path, main_code)
                .await
                .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write main.py: {}", e)))?;
            created_files.push("main.py".to_string());
        }

        let boot_path = fw_dir.join("boot.py");
        if !boot_path.exists() {
            let boot_code = "\"\"\"ONE-SHIP MicroPython boot.py — runs before main.py on every boot.\"\"\"\n\
                 # Configure Wi-Fi, filesystem or REPL settings here.\n";
            tokio::fs::write(&boot_path, boot_code)
                .await
                .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write boot.py: {}", e)))?;
            created_files.push("boot.py".to_string());
        }

        let readme_path = fw_dir.join("README.md");
        if !readme_path.exists() {
            let readme = format!(
                "# {board} MicroPython Firmware\n\n\
                 Generated by ONE-SHIP Electronics Engineering Workstation.\n\n\
                 1. Flash the MicroPython runtime once with `esptool` if the board still runs stock firmware.\n\
                 2. Upload `main.py` and `boot.py` with `mpremote connect <port> fs cp main.py :main.py + fs cp boot.py :boot.py + soft-reset`.\n\
                 3. Open the serial monitor at {speed} baud to read `print()` output.\n",
                board = args.board_id,
                speed = args.monitor_speed
            );
            tokio::fs::write(&readme_path, readme)
                .await
                .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write README.md: {}", e)))?;
            created_files.push("README.md".to_string());
        }

        append_python_gitignore(p_path).await?;
        upsert_electronics_settings(p_path, "micropython").await?;

        return Ok(ScaffoldFirmwareResult {
            success: true,
            environment: env_name,
            created_files,
        });
    }

    // 2. Generate platformio.ini
    let ini_content = format!(
        "; PlatformIO Project Configuration File\n\
         ; Generated by ONE-SHIP Electronics Engineering Workstation\n\
         ; https://docs.platformio.org/page/projectconf.html\n\n\
         [env:{env_name}]\n\
         platform = {platform}\n\
         board = {board_id}\n\
         framework = {framework}\n\
         monitor_speed = {monitor_speed}\n",
        env_name = env_name,
        platform = platform,
        board_id = args.board_id,
        framework = args.framework,
        monitor_speed = args.monitor_speed
    );

    let ini_path = fw_dir.join("platformio.ini");
    tokio::fs::write(&ini_path, ini_content)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write platformio.ini: {}", e)))?;
    created_files.push("platformio.ini".to_string());

    // 3. Create Directories: src, include, lib
    let src_dir = fw_dir.join("src");
    let inc_dir = fw_dir.join("include");
    let lib_dir = fw_dir.join("lib");
    tokio::fs::create_dir_all(&src_dir)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to create src/: {}", e)))?;
    tokio::fs::create_dir_all(&inc_dir)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to create include/: {}", e)))?;
    tokio::fs::create_dir_all(&lib_dir)
        .await
        .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to create lib/: {}", e)))?;

    // 4. Create Starter Source File (src/main.cpp or src/main.c)
    let is_c = args.language.as_deref() == Some("c");
    let src_file_name = if is_c { "main.c" } else { "main.cpp" };
    let main_path = src_dir.join(src_file_name);

    if !main_path.exists() && !src_dir.join("main.cpp").exists() && !src_dir.join("main.c").exists() {
        let code = if args.framework.eq_ignore_ascii_case("arduino") {
            format!(
                "/**\n\
                 * ONE-SHIP Embedded Firmware Boilerplate\n\
                 * Target Board: {board}\n\
                 * Framework: {framework}\n\
                 */\n\
                 #include <Arduino.h>\n\n\
                 #ifndef LED_BUILTIN\n\
                 #define LED_BUILTIN 2\n\
                 #endif\n\n\
                 void setup() {{\n\
                   Serial.begin({speed});\n\
                   pinMode(LED_BUILTIN, OUTPUT);\n\
                   Serial.println(\"[ONE-SHIP] Embedded Firmware Initialized on {board}\");\n\
                 }}\n\n\
                 void loop() {{\n\
                   digitalWrite(LED_BUILTIN, HIGH);\n\
                   delay(500);\n\
                   digitalWrite(LED_BUILTIN, LOW);\n\
                   delay(500);\n\
                   Serial.println(\"[ONE-SHIP] Heartbeat tick\");\n\
                 }}\n",
                board = args.board_id,
                framework = args.framework,
                speed = args.monitor_speed
            )
        } else {
            format!(
                "/**\n\
                 * ONE-SHIP Embedded Firmware Boilerplate\n\
                 * Target Board: {board}\n\
                 * Framework: {framework}\n\
                 */\n\
                 #include <stdio.h>\n\n\
                 int main(void) {{\n\
                   printf(\"[ONE-SHIP] Embedded Firmware Initialized on {board}\\n\");\n\
                   while (1) {{\n\
                     // Main application loop\n\
                   }}\n\
                   return 0;\n\
                 }}\n",
                board = args.board_id,
                framework = args.framework
            )
        };

        tokio::fs::write(&main_path, code)
            .await
            .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write {}: {}", src_file_name, e)))?;
        created_files.push(format!("src/{}", src_file_name));
    }

    // 5. Create include/README and lib/README
    let inc_readme = inc_dir.join("README");
    if !inc_readme.exists() {
        tokio::fs::write(&inc_readme, "This directory is intended for project header files.\n")
            .await
            .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write include/README: {}", e)))?;
        created_files.push("include/README".to_string());
    }
    let lib_readme = lib_dir.join("README");
    if !lib_readme.exists() {
        tokio::fs::write(&lib_readme, "This directory is intended for project specific (private) libraries.\n")
            .await
            .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to write lib/README: {}", e)))?;
        created_files.push("lib/README".to_string());
    }

    // 6. Update .gitignore
    let gitignore_path = p_path.join(".gitignore");
    if gitignore_path.exists() {
        let content = tokio::fs::read_to_string(&gitignore_path)
            .await
            .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to read .gitignore: {}", e)))?;
        if !content.contains(".pio") {
            let updated = format!("{}\n\n# PlatformIO build outputs\n.pio/\n.pioenvs/\n", content.trim_end());
            tokio::fs::write(&gitignore_path, updated)
                .await
                .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to update .gitignore: {}", e)))?;
        }
    } else {
        tokio::fs::write(&gitignore_path, "# PlatformIO build outputs\n.pio/\n.pioenvs/\n")
            .await
            .map_err(|e| ElectronicsError::FirmwareScaffoldFailed(format!("Failed to create .gitignore: {}", e)))?;
    }

    // 7. Ensure [plugins.electronics] is enabled and the language is recorded in rula.toml
    upsert_electronics_settings(p_path, "platformio").await?;

    Ok(ScaffoldFirmwareResult {
        success: true,
        environment: env_name,
        created_files,
    })
}

/// Discovers declared PlatformIO environments from platformio.ini inside the configured firmware_dir if present.
pub async fn list_firmware_environments(project_path: &Path) -> Vec<String> {
    let Ok((fw_dir, _)) = super::resolve_electronics_firmware_dir(project_path) else {
        return Vec::new();
    };
    let ini_path = fw_dir.join("platformio.ini");
    if !ini_path.exists() {
        return Vec::new();
    }

    let Ok(content) = tokio::fs::read_to_string(&ini_path).await else {
        return Vec::new();
    };

    let mut envs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[env:") && trimmed.ends_with(']') {
            let env_name = &trimmed[5..trimmed.len() - 1];
            if !env_name.is_empty() {
                envs.push(env_name.to_string());
            }
        }
    }
    envs
}

/// Builds project firmware using PlatformIO Core (`pio run`).
pub async fn build_project_firmware(
    project_path: &str,
    environment: Option<String>,
) -> Result<FirmwareOpResult, ElectronicsError> {
    let start = Instant::now();
    let p_path = Path::new(project_path);
    if !p_path.exists() {
        return Err(ElectronicsError::UnregisteredProject(project_path.to_string()));
    }
    let (fw_dir, _) = super::resolve_electronics_firmware_dir(p_path)?;

    let language = super::detect_firmware_language(p_path, &fw_dir);
    if language.as_deref() == Some("micropython") {
        return Err(ElectronicsError::BuildFailed {
            environment: "micropython".to_string(),
            details: "MicroPython projects have no compile step. Use Upload to copy main.py to the device.".to_string(),
        });
    }

    let mut cmd = tokio::process::Command::new("pio");
    cmd.current_dir(&fw_dir);
    cmd.arg("run");

    if let Some(ref env) = environment {
        cmd.arg("-e").arg(env);
    }

    let output = cmd.output().await.map_err(|e| ElectronicsError::ToolchainMissing {
        tool: "pio".to_string(),
        hint: format!("Failed to spawn PlatformIO ('pio run'): {}", e),
    })?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let success = output.status.success();
    if !success {
        return Err(ElectronicsError::BuildFailed {
            environment: environment.unwrap_or_else(|| "default".to_string()),
            details: combined_output,
        });
    }

    Ok(FirmwareOpResult {
        success: true,
        output: combined_output,
        duration_ms,
    })
}

/// Uploads/flashes compiled firmware to the target board using PlatformIO (`pio run -t upload`).
pub async fn flash_project_firmware(
    project_path: &str,
    environment: Option<String>,
    upload_port: Option<String>,
) -> Result<FirmwareOpResult, ElectronicsError> {
    let start = Instant::now();
    let p_path = Path::new(project_path);
    if !p_path.exists() {
        return Err(ElectronicsError::UnregisteredProject(project_path.to_string()));
    }
    let (fw_dir, _) = super::resolve_electronics_firmware_dir(p_path)?;

    let language = super::detect_firmware_language(p_path, &fw_dir);
    if language.as_deref() == Some("micropython") {
        let port = upload_port.clone().ok_or_else(|| ElectronicsError::InvalidSetting {
            key: "upload_port",
            details: "MicroPython upload requires the serial port of the board".to_string(),
        })?;
        let main_path = fw_dir.join("main.py");
        if !main_path.exists() {
            return Err(ElectronicsError::FirmwareScaffoldFailed(
                "main.py is missing; scaffold the MicroPython project first".to_string(),
            ));
        }

        let mut args: Vec<String> = vec![
            "connect".to_string(),
            port.clone(),
            "fs".to_string(),
            "cp".to_string(),
            "main.py".to_string(),
            ":main.py".to_string(),
        ];
        if fw_dir.join("boot.py").exists() {
            args.extend([
                "+".to_string(),
                "fs".to_string(),
                "cp".to_string(),
                "boot.py".to_string(),
                ":boot.py".to_string(),
            ]);
        }
        args.extend(["+".to_string(), "soft-reset".to_string()]);

        let timeout_seconds = crate::config::resolve_effective_u64(
            None,
            None,
            "electronics",
            "firmware_upload_timeout_seconds",
        )
        .max(1);
        let mut cmd = tokio::process::Command::new("mpremote");
        cmd.current_dir(&fw_dir)
            .args(&args)
            .kill_on_drop(true);

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_seconds),
            cmd.output(),
        )
        .await
        .map_err(|_| ElectronicsError::FlashFailed {
            port: port.clone(),
            details: format!(
                "MicroPython upload (`mpremote`) timed out after {timeout_seconds}s on port '{port}'. Check USB connection, BOOTSEL/reset state, or increase `[plugins.electronics].firmware_upload_timeout_seconds`."
            ),
        })?
        .map_err(|e| ElectronicsError::ToolchainMissing {
            tool: "mpremote".to_string(),
            hint: format!("Failed to spawn 'mpremote': {}. Install it with 'python -m pip install --user mpremote'.", e),
        })?;

        let duration_ms = start.elapsed().as_millis() as u64;
        let combined_output = format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            return Err(ElectronicsError::FlashFailed {
                port,
                details: combined_output,
            });
        }
        return Ok(FirmwareOpResult {
            success: true,
            output: combined_output,
            duration_ms,
        });
    }

    let timeout_seconds = crate::config::resolve_effective_u64(
        None,
        None,
        "electronics",
        "firmware_upload_timeout_seconds",
    )
    .max(1);
    let mut cmd = tokio::process::Command::new("pio");
    cmd.current_dir(&fw_dir);
    cmd.kill_on_drop(true);
    cmd.arg("run").arg("-t").arg("upload");

    if let Some(ref env) = environment {
        cmd.arg("-e").arg(env);
    }

    if let Some(ref port) = upload_port {
        cmd.arg("--upload-port").arg(port);
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        cmd.output(),
    )
    .await
    .map_err(|_| ElectronicsError::FlashFailed {
        port: upload_port.clone().unwrap_or_else(|| "auto".to_string()),
        details: format!(
            "PlatformIO upload (`pio run -t upload`) timed out after {timeout_seconds}s. Check board connection/bootloader mode or increase `[plugins.electronics].firmware_upload_timeout_seconds`."
        ),
    })?
    .map_err(|e| ElectronicsError::ToolchainMissing {
        tool: "pio".to_string(),
        hint: format!("Failed to spawn PlatformIO ('pio run -t upload'): {}", e),
    })?;

    let duration_ms = start.elapsed().as_millis() as u64;
    let combined_output = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let success = output.status.success();
    if !success {
        return Err(ElectronicsError::FlashFailed {
            port: upload_port.unwrap_or_else(|| "auto".to_string()),
            details: combined_output,
        });
    }

    Ok(FirmwareOpResult {
        success: true,
        output: combined_output,
        duration_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_bindings_platformioboard() {
        PlatformioBoard::export_all().unwrap();
    }

    #[test]
    fn export_bindings_listplatformioboardsargs() {
        ListPlatformioBoardsArgs::export_all().unwrap();
    }

    #[test]
    fn export_bindings_scaffoldfirmwareprojectargs() {
        ScaffoldFirmwareProjectArgs::export_all().unwrap();
    }

    #[test]
    fn export_bindings_scaffoldfirmwareresult() {
        ScaffoldFirmwareResult::export_all().unwrap();
    }
}
