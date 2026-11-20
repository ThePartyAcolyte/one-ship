use std::time::Instant;
use crate::kopeng::electronics::errors::ElectronicsError;
use crate::kopeng::electronics::instruments::driver::{
    DiscoveredInstrument, LogicCaptureResult, DecodedProtocolPacket,
};
use crate::kopeng::electronics::instruments::arbitrator::HardwareLockArbitrator;

/// USB Vendor ID for Saleae Logic analyzers.
pub const FX2_VID_SALEAE: u16 = 0x0925;
/// USB Product ID for Saleae Logic analyzers.
pub const FX2_PID_SALEAE: u16 = 0x3881;
/// USB Vendor ID for generic Cypress FX2LP logic analyzers.
pub const FX2_VID_CYPRESS: u16 = 0x04b4;
/// USB Product ID for generic Cypress FX2LP logic analyzers.
pub const FX2_PID_CYPRESS: u16 = 0x8613;

/// Probes USB bus for Cypress FX2 / Saleae clone 24MHz 8-channel logic analyzer.
pub fn probe_fx2_logic_analyzer() -> Result<Option<DiscoveredInstrument>, ElectronicsError> {
    let devices = rusb::devices().map_err(|e| ElectronicsError::SerialError {
        port: "USB".to_string(),
        details: format!("Failed to enumerate USB devices: {}", e),
    })?;

    for device in devices.iter() {
        if let Ok(desc) = device.device_descriptor() {
            let vid = desc.vendor_id();
            let pid = desc.product_id();
            if (vid == FX2_VID_SALEAE && pid == FX2_PID_SALEAE) || (vid == FX2_VID_CYPRESS && pid == FX2_PID_CYPRESS) {
                return Ok(Some(DiscoveredInstrument {
                    id: "fx2lafw_8ch".to_string(),
                    name: "Generic 24MHz 8-Channel Logic Analyzer (Cypress FX2LP / fx2lafw)".to_string(),
                    category: "logic_analyzer".to_string(),
                    vendor_id: format!("{:04x}", vid),
                    product_id: format!("{:04x}", pid),
                    is_connected: true,
                    transport: "sigrok / fx2lafw".to_string(),
                }));
            }
        }
    }

    Ok(None)
}

/// Executes a timed capture on the 24MHz logic analyzer using `sigrok-cli`.
pub async fn capture_fx2_logic(
    sample_rate_hz: u64,
    duration_ms: u64,
    decoder: Option<String>,
    decoder_pins: Option<String>,
) -> Result<LogicCaptureResult, ElectronicsError> {
    let _lock = HardwareLockArbitrator::acquire_logic_analyzer_lock()?;
    let start = Instant::now();

    // Verify sigrok-cli presence
    let sigrok_ok = crate::desktop::resolve_executable("sigrok-cli").is_some();
    if !sigrok_ok {
        let (hint, _) = crate::kopeng::diagnostic::resolve_tool_install_hint(
            "sigrok-cli",
            "Install sigrok-cli and sigrok-firmware-fx2lafw using your distribution package manager",
        );
        return Err(ElectronicsError::ToolchainMissing {
            tool: "sigrok-cli".to_string(),
            hint: format!("Install sigrok via: {}", hint),
        });
    }

    let mut cmd = tokio::process::Command::new("sigrok-cli");
    cmd.arg("--driver").arg("fx2lafw");
    cmd.arg("--config").arg(format!("samplerate={}", sample_rate_hz));
    cmd.arg("--time").arg(format!("{}ms", duration_ms));

    if let Some(ref proto) = decoder {
        let pin_spec = decoder_pins.as_deref().unwrap_or("");
        let dec_arg = if pin_spec.is_empty() {
            proto.clone()
        } else {
            format!("{}:{}", proto, pin_spec)
        };
        cmd.arg("-P").arg(dec_arg);
        cmd.arg("--protocol-decoder-jsontrace");
    } else {
        cmd.arg("-O").arg("wavedrom");
    }

    let output = cmd.output().await.map_err(|e| ElectronicsError::SerialError {
        port: "fx2lafw".to_string(),
        details: format!("Failed to spawn sigrok-cli: {}", e),
    })?;

    let stdout_str = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() && stdout_str.is_empty() {
        return Err(ElectronicsError::SerialError {
            port: "fx2lafw".to_string(),
            details: format!("sigrok-cli capture failed: {}", stderr_str.trim()),
        });
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;
    let samples_count = ((sample_rate_hz as f64) * (duration_ms as f64 / 1000.0)) as usize;

    // Parse decoded packets if jsontrace was requested
    let mut decoded_packets = Vec::new();
    let mut wavedrom_json = None;

    if decoder.is_some() {
        // Collect decoded events from JSON lines
        for line in stdout_str.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let (Some(name), Some(ts)) = (val.get("name").and_then(|v| v.as_str()), val.get("ts").and_then(|v| v.as_u64())) {
                        decoded_packets.push(DecodedProtocolPacket {
                            start_sample_index: ts,
                            end_sample_index: ts + 10,
                            proto: decoder.clone().unwrap_or_default(),
                            summary: name.to_string(),
                            data_hex: val.get("args").map(|a| a.to_string()),
                        });
                    }
                }
            }
        }
    } else {
        wavedrom_json = Some(stdout_str);
    }

    // Return empty 8 channels stub for waveform display if raw binary wasn't requested
    let channels = vec![Vec::new(); 8];

    Ok(LogicCaptureResult {
        channels,
        sample_rate_hz,
        samples_count,
        decoded_packets,
        wavedrom_json,
        duration_ms: elapsed_ms,
    })
}
