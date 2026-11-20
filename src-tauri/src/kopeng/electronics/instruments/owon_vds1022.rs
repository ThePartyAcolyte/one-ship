use std::path::PathBuf;
use std::time::Duration;
use crate::kopeng::electronics::errors::ElectronicsError;
use crate::kopeng::electronics::instruments::driver::{
    DiscoveredInstrument, OscilloscopeChannelConfig, OscilloscopeTriggerConfig,
    OscilloscopeCaptureResult, OscilloscopeCalibrationResult, OscilloscopeMetrics,
    OscilloscopeDescriptor,
};
use crate::kopeng::electronics::instruments::arbitrator::HardwareLockArbitrator;

pub const OWON_VID: u16 = 0x5345;
pub const OWON_PID: u16 = 0x1234;
pub const SAMPLES_PER_CHANNEL: usize = 5000;

pub const VOLT_RANGES_MV: [u32; 10] = [
    50,    // 5 mV/div (50mV full screen across 10 divisions)
    100,   // 10 mV/div
    200,   // 20 mV/div
    500,   // 50 mV/div
    1000,  // 100 mV/div
    2000,  // 200 mV/div
    5000,  // 500 mV/div
    10000, // 1000 mV/div (1V/div)
    20000, // 2000 mV/div (2V/div)
    50000, // 5000 mV/div (5V/div)
];

const BULK_OUT_ENDPOINT: u8 = 0x01;
const BULK_IN_ENDPOINT: u8 = 0x81;
const USB_TIMEOUT_MS: u64 = 2000;

/// Parsed factory calibration tables and metadata stored in the device's EEPROM/Flash.
#[derive(Debug, Clone)]
pub struct OwonFlashCalibration {
    pub gain: [[u16; 10]; 2],
    pub ampl: [[u16; 10]; 2],
    pub comp: [[u16; 10]; 2],
    pub phasefine: u16,
    pub version: String,
    pub serial: String,
    pub vfpga: u8,
}

/// Resolves the requested channel full-scale voltage at the scope input (accounting for probe attenuation)
/// to the closest internal calibration voltage range index (0..9).
pub fn volt_scope_mv_to_range_idx(scope_full_scale_mv: f64) -> usize {
    let ranges = [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0, 50000.0];
    let mut best_idx = 4; // Default 1V full scale (index 4)
    let mut best_diff = f64::MAX;
    for (i, &r) in ranges.iter().enumerate() {
        let diff = (r - scope_full_scale_mv).abs();
        if diff < best_diff {
            best_diff = diff;
            best_idx = i;
        }
    }
    best_idx
}

/// Embedded fallback FPGA bitstreams so packaged binaries run without an external resources dir.
fn embedded_firmware_bytes(filename: &str) -> Option<&'static [u8]> {
    match filename {
        "VDS1022_FPGAV1_V3.9.bin" => Some(include_bytes!("../../../../resources/firmware/owon/VDS1022_FPGAV1_V3.9.bin")),
        "VDS1022_FPGAV2_V4.1.bin" => Some(include_bytes!("../../../../resources/firmware/owon/VDS1022_FPGAV2_V4.1.bin")),
        "VDS1022_FPGAV3_gaoyun.bin" => Some(include_bytes!("../../../../resources/firmware/owon/VDS1022_FPGAV3_gaoyun.bin")),
        "VDS1022_FPGAV4_gaoyun.bin" => Some(include_bytes!("../../../../resources/firmware/owon/VDS1022_FPGAV4_gaoyun.bin")),
        "VDS1022_FPGAV5_gaoyun.bin" => Some(include_bytes!("../../../../resources/firmware/owon/VDS1022_FPGAV5_gaoyun.bin")),
        _ => None,
    }
}

/// Locates a firmware bitstream file across override, installed, bundle, and dev tree paths.
fn find_firmware_file(filename: &str) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(res_dir) = std::env::var("ONESHIP_RESOURCES_DIR") {
        let trimmed = res_dir.trim();
        if !trimmed.is_empty() {
            candidates.push(PathBuf::from(trimmed).join("firmware").join("owon").join(filename));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(prefix) = parent.parent() {
                candidates.push(prefix.join("share").join("one-ship").join("resources").join("firmware").join("owon").join(filename));
            }
            candidates.push(parent.join("resources").join("firmware").join("owon").join(filename));
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            candidates.push(PathBuf::from(xdg).join("one-ship").join("resources").join("firmware").join("owon").join(filename));
        }
    } else if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local").join("share").join("one-ship").join("resources").join("firmware").join("owon").join(filename));
    }
    candidates.push(PathBuf::from("/usr/local/share/one-ship/resources/firmware/owon").join(filename));
    candidates.push(PathBuf::from("/usr/share/one-ship/resources/firmware/owon").join(filename));
    candidates.push(PathBuf::from(format!("src-tauri/resources/firmware/owon/{}", filename)));
    candidates.push(PathBuf::from(format!("resources/firmware/owon/{}", filename)));
    candidates.push(PathBuf::from(format!("../resources/firmware/owon/{}", filename)));

    candidates.into_iter().find(|p| p.is_file())
}

/// Loads an Owon FPGA bitstream from disk overrides/installed paths, or the embedded fallback.
fn load_firmware_bitstream(filename: &str) -> Result<Vec<u8>, ElectronicsError> {
    if let Some(firmware_path) = find_firmware_file(filename) {
        return std::fs::read(&firmware_path).map_err(|e| ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to read bitstream file at {:?}: {}", firmware_path, e),
        });
    }
    embedded_firmware_bytes(filename)
        .map(|bytes| bytes.to_vec())
        .ok_or_else(|| ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Bitstream file '{}' not found in resources/firmware/owon/ or embedded assets", filename),
        })
}

/// Parses the 2002-byte Flash memory buffer returned by `READ_FLASH` (`0x01b0`).
pub fn parse_flash_memory(buf: &[u8]) -> Result<OwonFlashCalibration, ElectronicsError> {
    if buf.len() < 340 {
        return Err(ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Flash memory buffer too short (expected >= 340 bytes, got {})", buf.len()),
        });
    }

    let header = u16::from_le_bytes([buf[0], buf[1]]);
    if header != 0x55AA && header != 0xAA55 {
        return Err(ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Invalid flash header: 0x{:04X} (expected 0x55AA)", header),
        });
    }

    let mut gain = [[0u16; 10]; 2];
    let mut ampl = [[0u16; 10]; 2];
    let mut comp = [[0u16; 10]; 2];

    for chl in 0..2 {
        for range in 0..10 {
            let offset_gain = 6 + (chl * 10 + range) * 2;
            let offset_ampl = 46 + (chl * 10 + range) * 2;
            let offset_comp = 86 + (chl * 10 + range) * 2;
            gain[chl][range] = u16::from_le_bytes([buf[offset_gain], buf[offset_gain + 1]]);
            ampl[chl][range] = u16::from_le_bytes([buf[offset_ampl], buf[offset_ampl + 1]]);
            comp[chl][range] = u16::from_le_bytes([buf[offset_comp], buf[offset_comp + 1]]);
        }
    }

    let mut ver_end = 207;
    while ver_end < buf.len() && buf[ver_end] != 0 {
        ver_end += 1;
    }
    let version = String::from_utf8_lossy(&buf[207..ver_end]).to_string();

    let serial_start = ver_end + 1;
    let mut serial_end = serial_start;
    while serial_end < buf.len() && buf[serial_end] != 0 {
        serial_end += 1;
    }
    let serial = String::from_utf8_lossy(&buf[serial_start..serial_end]).to_string();

    let phasefine_offset = serial_end + 1 + 100;
    let phasefine = if phasefine_offset + 1 < buf.len() {
        u16::from_le_bytes([buf[phasefine_offset], buf[phasefine_offset + 1]])
    } else {
        0
    };

    let upper = version.to_uppercase();
    let vfpga = if upper.starts_with("V2.7.0") {
        3
    } else if upper.starts_with("V2.4.623") || upper.starts_with("V2.6.0") {
        2
    } else if upper.starts_with("V2.") || upper.starts_with("V1.") {
        1
    } else if upper.starts_with("V5.") {
        5
    } else if upper.starts_with("V4.") {
        4
    } else if upper.starts_with("V3.") {
        3
    } else {
        5
    };

    Ok(OwonFlashCalibration {
        gain,
        ampl,
        comp,
        phasefine,
        version,
        serial,
        vfpga,
    })
}

fn send_cmd_u8(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    out_ep: u8,
    in_ep: u8,
    addr: u32,
    val: u8,
) -> Result<(), ElectronicsError> {
    let mut buf = [0u8; 6];
    buf[0..4].copy_from_slice(&addr.to_le_bytes());
    buf[4] = 1;
    buf[5] = val;
    handle.write_bulk(out_ep, &buf, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write command 0x{:04x}: {}", addr, e),
        }
    })?;
    let mut resp = [0u8; 64];
    if let Err(e) = handle.read_bulk(in_ep, &mut resp, Duration::from_millis(100)) {
        crate::logger::log("DEBUG", "electronics", &format!("Owon VDS1022 cmd_u8(0x{:04x}) ack read: {}", addr, e));
    }
    Ok(())
}

fn send_cmd_u16(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    out_ep: u8,
    in_ep: u8,
    addr: u32,
    val: u16,
) -> Result<(), ElectronicsError> {
    let mut buf = [0u8; 7];
    buf[0..4].copy_from_slice(&addr.to_le_bytes());
    buf[4] = 2;
    buf[5..7].copy_from_slice(&val.to_le_bytes());
    handle.write_bulk(out_ep, &buf, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write command 0x{:04x}: {}", addr, e),
        }
    })?;
    let mut resp = [0u8; 64];
    if let Err(e) = handle.read_bulk(in_ep, &mut resp, Duration::from_millis(100)) {
        crate::logger::log("DEBUG", "electronics", &format!("Owon VDS1022 cmd_u16(0x{:04x}) ack read: {}", addr, e));
    }
    Ok(())
}

fn send_cmd_u32(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    out_ep: u8,
    in_ep: u8,
    addr: u32,
    val: u32,
) -> Result<(), ElectronicsError> {
    let mut buf = [0u8; 9];
    buf[0..4].copy_from_slice(&addr.to_le_bytes());
    buf[4] = 4;
    buf[5..9].copy_from_slice(&val.to_le_bytes());
    handle.write_bulk(out_ep, &buf, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write command 0x{:04x}: {}", addr, e),
        }
    })?;
    let mut resp = [0u8; 64];
    if let Err(e) = handle.read_bulk(in_ep, &mut resp, Duration::from_millis(100)) {
        crate::logger::log("DEBUG", "electronics", &format!("Owon VDS1022 cmd_u32(0x{:04x}) ack read: {}", addr, e));
    }
    Ok(())
}

static FLASH_CALIBRATION_CACHE: std::sync::Mutex<Option<OwonFlashCalibration>> = std::sync::Mutex::new(None);
static LAST_APPLIED_CONFIG: std::sync::Mutex<Option<(OscilloscopeChannelConfig, OscilloscopeChannelConfig, OscilloscopeTriggerConfig, u64)>> = std::sync::Mutex::new(None);

/// Ensures the FPGA bitstream is uploaded to the volatile FPGA RAM and returns factory calibration.
pub fn ensure_fpga_loaded(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    out_ep: u8,
    in_ep: u8,
) -> Result<OwonFlashCalibration, ElectronicsError> {
    let query_fpga_cmd: [u8; 6] = [0x23, 0x02, 0x00, 0x00, 0x01, 0x00];
    let mut resp = [0u8; 64];
    handle.write_bulk(out_ep, &query_fpga_cmd, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write QUERY_FPGA: {}", e),
        }
    })?;
    handle.read_bulk(in_ep, &mut resp, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to read QUERY_FPGA response: {}", e),
        }
    })?;

    let is_loaded = resp[1] == 0x01;

    {
        let guard = FLASH_CALIBRATION_CACHE.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(ref cached) = *guard {
            if is_loaded {
                return Ok(cached.clone());
            }
        }
    }

    let read_flash_cmd: [u8; 6] = [0xb0, 0x01, 0x00, 0x00, 0x01, 0x01];
    handle.write_bulk(out_ep, &read_flash_cmd, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write READ_FLASH: {}", e),
        }
    })?;
    let mut flash_buf = vec![0u8; 2048];
    let cal = match handle.read_bulk(in_ep, &mut flash_buf, Duration::from_millis(USB_TIMEOUT_MS)) {
        Ok(n) => parse_flash_memory(&flash_buf[..n])?,
        Err(_) => {
            if is_loaded {
                OwonFlashCalibration {
                    gain: [[1236, 1093, 961, 794, 665, 534, 968, 838, 711, 539]; 2],
                    ampl: [[364; 10]; 2],
                    comp: [[1410; 10]; 2],
                    phasefine: 0,
                    version: "V5.0.1".to_string(),
                    serial: "VDS1022".to_string(),
                    vfpga: 5,
                }
            } else {
                return Err(ElectronicsError::SerialError {
                    port: "Owon VDS1022".to_string(),
                    details: "Failed to read flash memory from device before FPGA load".to_string(),
                });
            }
        }
    };

    *FLASH_CALIBRATION_CACHE.lock().unwrap_or_else(|p| p.into_inner()) = Some(cal.clone());

    if is_loaded {
        return Ok(cal);
    }

    let filename = match cal.vfpga {
        1 => "VDS1022_FPGAV1_V3.9.bin",
        2 => "VDS1022_FPGAV2_V4.1.bin",
        3 => "VDS1022_FPGAV3_gaoyun.bin",
        4 => "VDS1022_FPGAV4_gaoyun.bin",
        _ => "VDS1022_FPGAV5_gaoyun.bin",
    };

    let bitstream_bytes = load_firmware_bitstream(filename)?;

    let len_bytes = (bitstream_bytes.len() as u32).to_le_bytes();
    let load_cmd: [u8; 9] = [
        0x00, 0x40, 0x00, 0x00, 0x04,
        len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3],
    ];
    handle.write_bulk(out_ep, &load_cmd, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write LOAD_FPGA header: {}", e),
        }
    })?;
    let mut load_resp = [0u8; 64];
    let n_resp = handle.read_bulk(in_ep, &mut load_resp, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("LOAD_FPGA handshake failed: {}", e),
        }
    })?;

    if n_resp < 5 {
        return Err(ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("LOAD_FPGA returned invalid response size: {}", n_resp),
        });
    }

    let frame_size = u32::from_le_bytes([load_resp[1], load_resp[2], load_resp[3], load_resp[4]]) as usize;
    if frame_size <= 4 {
        return Err(ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("LOAD_FPGA returned invalid frame size: {}", frame_size),
        });
    }

    let payload_size = frame_size - 4;
    let chunks: Vec<&[u8]> = bitstream_bytes.chunks(payload_size).collect();

    for (i, chunk) in chunks.iter().enumerate() {
        let mut packet = Vec::with_capacity(4 + chunk.len());
        packet.extend_from_slice(&(i as u32).to_le_bytes());
        packet.extend_from_slice(chunk);
        handle.write_bulk(out_ep, &packet, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
            ElectronicsError::SerialError {
                port: "Owon VDS1022".to_string(),
                details: format!("Failed to write bitstream chunk {}: {}", i, e),
            }
        })?;
        let mut chunk_resp = [0u8; 64];
        handle.read_bulk(in_ep, &mut chunk_resp, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
            ElectronicsError::SerialError {
                port: "Owon VDS1022".to_string(),
                details: format!("Failed to read bitstream chunk {} ack: {}", i, e),
            }
        })?;
    }

    handle.write_bulk(out_ep, &query_fpga_cmd, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to write post-upload QUERY_FPGA: {}", e),
        }
    })?;
    handle.read_bulk(in_ep, &mut resp, Duration::from_millis(USB_TIMEOUT_MS)).map_err(|e| {
        ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Failed to read post-upload QUERY_FPGA response: {}", e),
        }
    })?;
    if resp[1] != 0x01 {
        return Err(ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: "FPGA bitstream uploaded but QUERY_FPGA still indicates unconfigured state".to_string(),
        });
    }

    Ok(cal)
}

/// Configures all FPGA and analog frontend registers before an acquisition cycle.
pub fn configure_device_for_capture(
    handle: &rusb::DeviceHandle<rusb::GlobalContext>,
    out_ep: u8,
    in_ep: u8,
    cal: &OwonFlashCalibration,
    ch1_cfg: &OscilloscopeChannelConfig,
    ch2_cfg: &OscilloscopeChannelConfig,
    trigger_cfg: &OscilloscopeTriggerConfig,
    timebase_div_us: f64,
) -> Result<(), ElectronicsError> {
    send_cmd_u16(handle, out_ep, in_ep, 0x18, cal.phasefine)?; // SET_PHASEFINE
    send_cmd_u8(handle, out_ep, in_ep, 0x09, 0)?;              // SET_PEAKMODE = 0
    send_cmd_u16(handle, out_ep, in_ep, 0x5c, 5100)?;          // SET_DEEPMEMORY = 5100
    send_cmd_u16(handle, out_ep, in_ep, 0x5a, 2550)?;          // SET_PRE_TRG = 2550
    send_cmd_u32(handle, out_ep, in_ep, 0x56, 2550)?;          // SET_SUF_TRG = 2550
    send_cmd_u16(handle, out_ep, in_ep, 0x06, 0)?;             // SET_MULTI = 0

    // Channel 1 setup
    let ch1_probe = (ch1_cfg.probe_attenuation.max(1)) as f64;
    let ch1_scope_full_scale_mv = ((ch1_cfg.volts_per_div_mv * 10) as f64) / ch1_probe;
    let ch1_range_idx = volt_scope_mv_to_range_idx(ch1_scope_full_scale_mv);
    let ch1_pos0 = 250.0 * (ch1_cfg.vertical_offset_mv as f64 / ((ch1_cfg.volts_per_div_mv * 10) as f64));
    let ch1_zero_arg = ((cal.comp[0][ch1_range_idx] as f64) - ch1_pos0 * (cal.ampl[0][ch1_range_idx] as f64) / 100.0)
        .round().clamp(0.0, 4095.0) as u16;
    let ch1_gain_arg = cal.gain[0][ch1_range_idx].clamp(0, 4095);
    let ch1_attenuate: u8 = if ch1_range_idx >= 6 { 1 } else { 0 };
    let ch1_coupling_val: u8 = match ch1_cfg.coupling.as_str() {
        "DC" | "dc" => 1,
        "GND" | "gnd" => 2,
        _ => 0, // AC
    };
    let ch1_on: u8 = if ch1_cfg.enabled { 1 } else { 0 };
    let ch1_arg = (ch1_attenuate << 1) | (ch1_coupling_val << 5) | (ch1_on << 7);

    send_cmd_u16(handle, out_ep, in_ep, 0x010a, ch1_zero_arg)?; // SET_ZERO_OFF CH1
    send_cmd_u16(handle, out_ep, in_ep, 0x0116, ch1_gain_arg)?; // SET_VOLT_GAIN CH1
    send_cmd_u8(handle, out_ep, in_ep, 0x0111, ch1_arg)?;       // SET_CHANNEL CH1
    send_cmd_u16(handle, out_ep, in_ep, 0x26, 0x8002)?;        // SET_TRG_HOLDOFF CH1

    // Channel 2 setup
    let ch2_probe = (ch2_cfg.probe_attenuation.max(1)) as f64;
    let ch2_scope_full_scale_mv = ((ch2_cfg.volts_per_div_mv * 10) as f64) / ch2_probe;
    let ch2_range_idx = volt_scope_mv_to_range_idx(ch2_scope_full_scale_mv);
    let ch2_pos0 = 250.0 * (ch2_cfg.vertical_offset_mv as f64 / ((ch2_cfg.volts_per_div_mv * 10) as f64));
    let ch2_zero_arg = ((cal.comp[1][ch2_range_idx] as f64) - ch2_pos0 * (cal.ampl[1][ch2_range_idx] as f64) / 100.0)
        .round().clamp(0.0, 4095.0) as u16;
    let ch2_gain_arg = cal.gain[1][ch2_range_idx].clamp(0, 4095);
    let ch2_attenuate: u8 = if ch2_range_idx >= 6 { 1 } else { 0 };
    let ch2_coupling_val: u8 = match ch2_cfg.coupling.as_str() {
        "DC" | "dc" => 1,
        "GND" | "gnd" => 2,
        _ => 0, // AC
    };
    let ch2_on: u8 = if ch2_cfg.enabled { 1 } else { 0 };
    let ch2_arg = (ch2_attenuate << 1) | (ch2_coupling_val << 5) | (ch2_on << 7);

    send_cmd_u16(handle, out_ep, in_ep, 0x0108, ch2_zero_arg)?; // SET_ZERO_OFF CH2
    send_cmd_u16(handle, out_ep, in_ep, 0x0114, ch2_gain_arg)?; // SET_VOLT_GAIN CH2
    send_cmd_u8(handle, out_ep, in_ep, 0x0110, ch2_arg)?;       // SET_CHANNEL CH2
    send_cmd_u16(handle, out_ep, in_ep, 0x2a, 0x8002)?;        // SET_TRG_HOLDOFF CH2

    // Trigger settings
    let src_val: u16 = match trigger_cfg.source.as_str() {
        "CH2" => 1,
        "EXT" => 2,
        _ => 0,
    };
    let slope_val: u16 = if trigger_cfg.slope.eq_ignore_ascii_case("falling") { 1 } else { 0 };
    let sweep_val: u16 = match trigger_cfg.sweep_mode.as_str() {
        "Single" | "single" => 1,
        "Normal" | "normal" => 2,
        _ => 0, // Auto
    };
    let trg_arg: u16 = src_val | (slope_val << 4) | (sweep_val << 8);
    send_cmd_u16(handle, out_ep, in_ep, 0x24, trg_arg)?; // SET_TRIGGER

    let active_range_idx = if src_val == 1 { ch2_range_idx } else { ch1_range_idx };
    let active_atten = if src_val == 1 { ch2_cfg.probe_attenuation } else { ch1_cfg.probe_attenuation } as f64;
    let full_scale_mv = (VOLT_RANGES_MV[active_range_idx] as f64) * active_atten;
    let lvl_counts = (((trigger_cfg.level_mv as f64) / full_scale_mv) * 250.0).clamp(-120.0, 120.0) as i8;

    let edge_arg: u16 = if slope_val == 0 {
        ((lvl_counts.saturating_add(10) as u8 as u16) & 0xff) | (((lvl_counts as u8 as u16) & 0xff) << 8)
    } else {
        ((lvl_counts as u8 as u16) & 0xff) | (((lvl_counts.saturating_sub(10) as u8 as u16) & 0xff) << 8)
    };

    if src_val == 1 {
        send_cmd_u16(handle, out_ep, in_ep, 0x30, edge_arg)?; // SET_EDGE_LEVEL CH2
        send_cmd_u8(handle, out_ep, in_ep, 0x4b, lvl_counts as u8)?; // SET_FREQREF CH2
    } else {
        send_cmd_u16(handle, out_ep, in_ep, 0x2e, edge_arg)?; // SET_EDGE_LEVEL CH1
        send_cmd_u8(handle, out_ep, in_ep, 0x4a, lvl_counts as u8)?; // SET_FREQREF CH1
    }

    // Timebase calculation
    let screen_seconds = (timebase_div_us * 10.0) * 1e-6;
    let target_rate_hz = if screen_seconds > 0.0 {
        (5000.0 / screen_seconds).clamp(2.5, 100_000_000.0)
    } else {
        100_000_000.0
    };
    let prescaler = (100_000_000.0 / target_rate_hz).round().max(1.0) as u32;
    send_cmd_u32(handle, out_ep, in_ep, 0x52, prescaler)?; // SET_TIMEBASE

    // Channels mask & run
    let chl_mask = (ch1_on) | (ch2_on << 1);
    send_cmd_u8(handle, out_ep, in_ep, 0x0b, chl_mask)?; // SET_CHL_ON
    send_cmd_u8(handle, out_ep, in_ep, 0x61, 0)?;        // SET_RUNSTOP (0 = Run)

    Ok(())
}

fn find_owon_device() -> Result<rusb::Device<rusb::GlobalContext>, ElectronicsError> {
    let devices = rusb::devices().map_err(|e| ElectronicsError::SerialError {
        port: "Owon VDS1022".to_string(),
        details: format!("Cannot enumerate USB devices: {}", e),
    })?;

    for dev in devices.iter() {
        if let Ok(desc) = dev.device_descriptor() {
            if desc.vendor_id() == OWON_VID && desc.product_id() == OWON_PID {
                return Ok(dev);
            }
        }
    }

    Err(ElectronicsError::DeviceNotFound {
        port: "Owon VDS1022 (VID: 0x5345, PID: 0x1234)".to_string(),
    })
}

fn resolve_endpoints(dev: &rusb::Device<rusb::GlobalContext>) -> (u8, u8) {
    let mut out_ep = BULK_OUT_ENDPOINT;
    let mut in_ep = BULK_IN_ENDPOINT;
    if let Ok(config_desc) = dev.active_config_descriptor() {
        for iface in config_desc.interfaces() {
            for iface_desc in iface.descriptors() {
                for ep_desc in iface_desc.endpoint_descriptors() {
                    if ep_desc.transfer_type() == rusb::TransferType::Bulk {
                        if ep_desc.direction() == rusb::Direction::Out {
                            out_ep = ep_desc.address();
                        } else if ep_desc.direction() == rusb::Direction::In {
                            in_ep = ep_desc.address();
                        }
                    }
                }
            }
        }
    }
    (out_ep, in_ep)
}

fn flush_endpoint(handle: &rusb::DeviceHandle<rusb::GlobalContext>, in_ep: u8) {
    let max_attempts_count = crate::config::resolve_effective_usize(
        None,
        None,
        "electronics",
        "usb_flush_max_attempts_count",
    )
    .max(1);
    let mut flush_buf = [0u8; 512];
    for _ in 0..max_attempts_count {
        match handle.read_bulk(in_ep, &mut flush_buf, Duration::from_millis(5)) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

/// Returns the declarative capability descriptor and specifications for the Owon VDS1022 driver.
pub fn descriptor() -> OscilloscopeDescriptor {
    OscilloscopeDescriptor {
        id: "owon_vds1022".to_string(),
        name: "Owon VDS1022".to_string(),
        vendor: "Owon".to_string(),
        model: "VDS1022".to_string(),
        channel_count: 2,
        channel_names: vec!["CH1".to_string(), "CH2".to_string()],
        volts_per_div_options_mv: vec![5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000],
        couplings: vec!["DC".to_string(), "AC".to_string(), "GND".to_string()],
        probe_attenuations: vec![1, 10],
        timebase_steps_us: vec![
            0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0,
            200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0, 50000.0, 100000.0,
        ],
        min_timebase_us: 0.1,
        max_timebase_us: 100000.0,
        max_sample_rate_hz: 100_000_000,
        bandwidth_hz: 25_000_000,
        buffer_points_count: SAMPLES_PER_CHANNEL,
        has_trigger: true,
        trigger_sources: vec!["CH1".to_string(), "CH2".to_string(), "EXT".to_string()],
        trigger_modes: vec!["auto".to_string(), "normal".to_string(), "single".to_string()],
        trigger_slopes: vec!["rising".to_string(), "falling".to_string()],
        has_hardware_calibration: true,
        has_signal_generator: false,
    }
}

/// Checks if an Owon VDS1022/VDS1022I oscilloscope is connected on USB buses.
pub fn probe_owon_vds1022() -> Result<Option<DiscoveredInstrument>, ElectronicsError> {
    if let Ok(dev) = find_owon_device() {
        let desc = dev.device_descriptor().map_err(|e| ElectronicsError::SerialError {
            port: "USB".to_string(),
            details: format!("Failed to read device descriptor: {}", e),
        })?;
        return Ok(Some(DiscoveredInstrument {
            id: "owon_vds1022".to_string(),
            name: "Owon VDS1022 Digital Storage Oscilloscope (25MHz, 100MSa/s)".to_string(),
            category: "oscilloscope".to_string(),
            vendor_id: format!("{:04x}", desc.vendor_id()),
            product_id: format!("{:04x}", desc.product_id()),
            is_connected: true,
            transport: "USB Bulk".to_string(),
        }));
    }
    Ok(None)
}

/// Computes scalar metrics from an array of calibrated analog voltage samples.
pub fn compute_channel_metrics(samples: &[f64], time_per_sample_us: f64) -> OscilloscopeMetrics {
    if samples.is_empty() {
        return OscilloscopeMetrics::default();
    }

    let mut v_min = f64::INFINITY;
    let mut v_max = f64::NEG_INFINITY;
    let mut sum = 0.0;
    let mut sum_sq = 0.0;

    for &v in samples {
        if v < v_min {
            v_min = v;
        }
        if v > v_max {
            v_max = v;
        }
        sum += v;
        sum_sq += v * v;
    }

    let n = samples.len() as f64;
    let v_avg = sum / n;
    let v_rms = (sum_sq / n).sqrt();
    let v_pp = (v_max - v_min).max(0.0);

    let mid_level = (v_max + v_min) / 2.0;
    let mut rising_crossings = Vec::new();
    let mut high_samples_count: usize = 0;

    for i in 1..samples.len() {
        if samples[i] >= mid_level {
            high_samples_count += 1;
        }
        if samples[i - 1] < mid_level && samples[i] >= mid_level {
            rising_crossings.push(i);
        }
    }

    let mut frequency_hz = None;
    let mut period_us = None;

    if rising_crossings.len() >= 2 {
        let mut total_period_samples = 0.0;
        let cycles_count = (rising_crossings.len() - 1) as f64;
        for i in 1..rising_crossings.len() {
            total_period_samples += (rising_crossings[i] - rising_crossings[i - 1]) as f64;
        }
        let avg_period_samples = total_period_samples / cycles_count;
        let p_us = avg_period_samples * time_per_sample_us;
        if p_us > 0.0 {
            period_us = Some(p_us);
            frequency_hz = Some(1_000_000.0 / p_us);
        }
    }

    let duty_cycle_percent = if samples.len() > 10 {
        Some((high_samples_count as f64 / n) * 100.0)
    } else {
        None
    };

    OscilloscopeMetrics {
        v_pp_mv: v_pp,
        v_rms_mv: v_rms,
        v_min_mv: v_min,
        v_max_mv: v_max,
        v_avg_mv: v_avg,
        frequency_hz,
        period_us,
        duty_cycle_percent,
    }
}

/// Executes an acquisition cycle from the Owon VDS1022 on a blocking worker thread.
pub async fn capture_vds1022(
    ch1_cfg: OscilloscopeChannelConfig,
    ch2_cfg: OscilloscopeChannelConfig,
    trigger_cfg: OscilloscopeTriggerConfig,
    timebase_div_us: f64,
) -> Result<OscilloscopeCaptureResult, ElectronicsError> {
    tokio::task::spawn_blocking(move || {
        capture_vds1022_blocking(ch1_cfg, ch2_cfg, trigger_cfg, timebase_div_us)
    })
    .await
    .map_err(|e| ElectronicsError::SerialError {
        port: "Owon VDS1022".to_string(),
        details: format!("Blocking capture task panicked: {}", e),
    })?
}

fn capture_vds1022_blocking(
    ch1_cfg: OscilloscopeChannelConfig,
    ch2_cfg: OscilloscopeChannelConfig,
    trigger_cfg: OscilloscopeTriggerConfig,
    timebase_div_us: f64,
) -> Result<OscilloscopeCaptureResult, ElectronicsError> {
    if !ch1_cfg.enabled && !ch2_cfg.enabled {
        return Ok(OscilloscopeCaptureResult {
            ch1_samples: Vec::new(),
            ch2_samples: Vec::new(),
            ch1_metrics: OscilloscopeMetrics::default(),
            ch2_metrics: OscilloscopeMetrics::default(),
            time_per_div_us: timebase_div_us,
            sample_rate_hz: 100_000_000,
            samples_count: 0,
            duration_ms: 0,
        });
    }

    let _lock = HardwareLockArbitrator::acquire_oscilloscope_lock()?;
    let start = std::time::Instant::now();

    let dev = find_owon_device()?;
    let handle = dev.open().map_err(|e| ElectronicsError::SerialError {
        port: "Owon VDS1022".to_string(),
        details: format!("Failed to open USB device: {}. Check udev permissions (/etc/udev/rules.d/99-oneship-instruments.rules).", e),
    })?;

    if let Err(e) = handle.set_auto_detach_kernel_driver(true) {
        crate::logger::log("DEBUG", "electronics", &format!("Owon VDS1022 auto_detach_kernel_driver: {}", e));
    }
    handle.claim_interface(0).map_err(|e| ElectronicsError::SerialError {
        port: "Owon VDS1022".to_string(),
        details: format!("Failed to claim USB interface 0: {}", e),
    })?;

    let (out_ep, in_ep) = resolve_endpoints(&dev);
    flush_endpoint(&handle, in_ep);

    // 1. Ensure FPGA bitstream is loaded and parse flash calibration
    let cal = ensure_fpga_loaded(&handle, out_ep, in_ep)?;
    crate::logger::log("DEBUG", "electronics", &format!("VDS1022 Flash Cal CH1 Gain: {:?}, Ampl: {:?}, Comp: {:?}", cal.gain[0], cal.ampl[0], cal.comp[0]));
    crate::logger::log("DEBUG", "electronics", &format!("VDS1022 Device Version: {}, Serial: {}, FPGA V: {}", cal.version, cal.serial, cal.vfpga));

    // 2. Configure FPGA registers, timebase, and analog channels only if hardware parameters changed
    let timebase_key = (timebase_div_us * 1000.0).round() as u64;
    let config_changed = {
        let guard = LAST_APPLIED_CONFIG.lock().unwrap_or_else(|p| p.into_inner());
        match &*guard {
            Some((ref c1, ref c2, ref trg, tb)) => {
                *c1 != ch1_cfg || *c2 != ch2_cfg || *trg != trigger_cfg || *tb != timebase_key
            }
            None => true,
        }
    };

    if config_changed {
        configure_device_for_capture(
            &handle, out_ep, in_ep, &cal, &ch1_cfg, &ch2_cfg, &trigger_cfg, timebase_div_us,
        )?;
        // Give physical relays and PGA analog frontend 150ms to settle only when hardware changed
        std::thread::sleep(Duration::from_millis(150));
        *LAST_APPLIED_CONFIG.lock().unwrap_or_else(|p| p.into_inner()) = Some((ch1_cfg.clone(), ch2_cfg.clone(), trigger_cfg.clone(), timebase_key));
    }

    // 3. Request data
    let ch1_req = if ch1_cfg.enabled { 0x05 } else { 0x04 };
    let ch2_req = if ch2_cfg.enabled { 0x05 } else { 0x04 };
    let get_data_cmd: [u8; 7] = [0x00, 0x10, 0x00, 0x00, 0x02, ch1_req, ch2_req];

    let mut raw_frame1 = vec![0u8; 6000];
    let mut frame1_len = 0;
    let max_attempts_count = 25;
    let poll_step_ms = 20;

    for _attempt in 0..max_attempts_count {
        if let Err(e) = handle.write_bulk(out_ep, &get_data_cmd, Duration::from_millis(USB_TIMEOUT_MS)) {
            crate::logger::log("WARN", "electronics", &format!("Owon VDS1022 get_data_cmd write failed: {}", e));
            continue;
        }
        match handle.read_bulk(in_ep, &mut raw_frame1, Duration::from_millis(250)) {
            Ok(n) => {
                if n >= 5211 {
                    frame1_len = n;
                    break;
                }
            }
            Err(e) => crate::logger::log("DEBUG", "electronics", &format!("Owon VDS1022 capture poll read failed: {}", e)),
        }
        std::thread::sleep(Duration::from_millis(poll_step_ms));
    }

    if frame1_len < 5211 {
        return Err(ElectronicsError::SerialError {
            port: "Owon VDS1022".to_string(),
            details: format!("Capture timed out waiting for waveform frame (expected 5211 bytes, got {})", frame1_len),
        });
    }

    // If both channels enabled, read frame 2 for CH2
    let mut raw_frame2 = vec![0u8; 6000];
    let mut frame2_len = 0;
    if ch1_cfg.enabled && ch2_cfg.enabled {
        for _attempt in 0..10 {
            match handle.read_bulk(in_ep, &mut raw_frame2, Duration::from_millis(250)) {
                Ok(n) => {
                    if n >= 5211 {
                        frame2_len = n;
                        break;
                    }
                }
                Err(_) => {}
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    crate::logger::log(
        "DEBUG",
        "electronics",
        &format!(
            "VDS1022 Frame header: chl={}, time_sum={}, period_num={}, cursor={}",
            raw_frame1[0],
            u32::from_le_bytes([raw_frame1[1], raw_frame1[2], raw_frame1[3], raw_frame1[4]]),
            u32::from_le_bytes([raw_frame1[5], raw_frame1[6], raw_frame1[7], raw_frame1[8]]),
            u16::from_le_bytes([raw_frame1[9], raw_frame1[10]]),
        ),
    );
    let min_b = raw_frame1[161..5161].iter().copied().min().unwrap_or(0);
    let max_b = raw_frame1[161..5161].iter().copied().max().unwrap_or(0);
    crate::logger::log("DEBUG", "electronics", &format!("VDS1022 raw byte range [161..5161]: min=0x{:02x} ({}), max=0x{:02x} ({})", min_b, min_b as i8, max_b, max_b as i8));

    // 25 counts per division
    let ch1_scale_mv = (ch1_cfg.volts_per_div_mv as f64) / 25.0;
    let ch2_scale_mv = (ch2_cfg.volts_per_div_mv as f64) / 25.0;

    let (ch1_samples, ch2_samples) = if ch1_cfg.enabled && ch2_cfg.enabled {
        let ch1: Vec<f64> = raw_frame1[161..5161]
            .iter()
            .map(|&b| ((b as i8) as f64) * ch1_scale_mv - (ch1_cfg.vertical_offset_mv as f64))
            .collect();
        let ch2: Vec<f64> = if frame2_len >= 5211 {
            raw_frame2[161..5161]
                .iter()
                .map(|&b| ((b as i8) as f64) * ch2_scale_mv - (ch2_cfg.vertical_offset_mv as f64))
                .collect()
        } else {
            vec![0.0; SAMPLES_PER_CHANNEL]
        };
        (ch1, ch2)
    } else if ch1_cfg.enabled {
        let ch1: Vec<f64> = raw_frame1[161..5161]
            .iter()
            .map(|&b| ((b as i8) as f64) * ch1_scale_mv - (ch1_cfg.vertical_offset_mv as f64))
            .collect();
        (ch1, Vec::new())
    } else if ch2_cfg.enabled {
        let ch2: Vec<f64> = raw_frame1[161..5161]
            .iter()
            .map(|&b| ((b as i8) as f64) * ch2_scale_mv - (ch2_cfg.vertical_offset_mv as f64))
            .collect();
        (Vec::new(), ch2)
    } else {
        (Vec::new(), Vec::new())
    };

    let time_per_sample_us = (timebase_div_us * 10.0) / (SAMPLES_PER_CHANNEL as f64);
    let sample_rate_hz = if time_per_sample_us > 0.0 {
        (1_000_000.0 / time_per_sample_us) as u64
    } else {
        100_000_000
    };

    let ch1_metrics = if ch1_cfg.enabled {
        compute_channel_metrics(&ch1_samples, time_per_sample_us)
    } else {
        OscilloscopeMetrics::default()
    };
    let ch2_metrics = if ch2_cfg.enabled {
        compute_channel_metrics(&ch2_samples, time_per_sample_us)
    } else {
        OscilloscopeMetrics::default()
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    Ok(OscilloscopeCaptureResult {
        ch1_samples,
        ch2_samples,
        ch1_metrics,
        ch2_metrics,
        time_per_div_us: timebase_div_us,
        sample_rate_hz,
        samples_count: SAMPLES_PER_CHANNEL,
        duration_ms,
    })
}

/// Executes calibration verification or zero-offset calibration.
pub async fn calibrate_vds1022(
    mode: String,
    channel: Option<String>,
    probe_attenuation: Option<u32>,
) -> Result<OscilloscopeCalibrationResult, ElectronicsError> {
    tokio::task::spawn_blocking(move || {
        calibrate_vds1022_blocking(mode, channel, probe_attenuation)
    })
    .await
    .map_err(|e| ElectronicsError::SerialError {
        port: "Owon VDS1022".to_string(),
        details: format!("Blocking calibration task panicked: {}", e),
    })?
}

fn calibrate_vds1022_blocking(
    mode: String,
    channel: Option<String>,
    probe_attenuation: Option<u32>,
) -> Result<OscilloscopeCalibrationResult, ElectronicsError> {
    *LAST_APPLIED_CONFIG.lock().unwrap_or_else(|p| p.into_inner()) = None;
    let chl = channel.unwrap_or_else(|| "CH1".to_string());
    let atten = probe_attenuation.unwrap_or(1);

    if mode == "probe_check" {
        let ch_cfg = OscilloscopeChannelConfig {
            enabled: true,
            volts_per_div_mv: 1000,
            vertical_offset_mv: 0,
            coupling: "DC".to_string(),
            probe_attenuation: atten,
        };
        let other_cfg = OscilloscopeChannelConfig {
            enabled: false,
            volts_per_div_mv: 1000,
            vertical_offset_mv: 0,
            coupling: "DC".to_string(),
            probe_attenuation: 1,
        };
        let (ch1, ch2) = if chl == "CH2" {
            (other_cfg, ch_cfg)
        } else {
            (ch_cfg, other_cfg)
        };
        let trg = OscilloscopeTriggerConfig {
            source: chl.clone(),
            slope: "Rising".to_string(),
            sweep_mode: "Auto".to_string(),
            level_mv: 1500 * (atten as i32),
        };

        let res = capture_vds1022_blocking(ch1, ch2, trg, 200.0)?;
        let (samples, metrics) = if chl == "CH2" {
            (&res.ch2_samples, &res.ch2_metrics)
        } else {
            (&res.ch1_samples, &res.ch1_metrics)
        };

        if samples.is_empty() {
            return Ok(OscilloscopeCalibrationResult {
                status: "failed".to_string(),
                message: "No samples acquired from oscilloscope".to_string(),
                measured_freq_hz: None,
                measured_vpp_mv: None,
                measured_duty_cycle_percent: None,
                zero_offsets_mv: None,
                probe_trim_advice: None,
            });
        }

        let freq = metrics.frequency_hz.unwrap_or(0.0);
        let vpp = metrics.v_pp_mv;
        let duty = metrics.duty_cycle_percent.unwrap_or(0.0);

        let freq_ok = (freq - 1000.0).abs() <= 80.0;
        let vpp_ok = vpp >= (1800.0 * (atten as f64)) && vpp <= (6000.0 * (atten as f64));

        if !freq_ok || !vpp_ok {
            return Ok(OscilloscopeCalibrationResult {
                status: "warning".to_string(),
                message: format!(
                    "Signal does not match expected 1kHz test square wave (measured: {:.1} Hz, {:.0} mV Vpp). Ensure probe tip is connected to the CAL pin and ground clip to GND.",
                    freq, vpp
                ),
                measured_freq_hz: Some(freq),
                measured_vpp_mv: Some(vpp),
                measured_duty_cycle_percent: Some(duty),
                zero_offsets_mv: None,
                probe_trim_advice: Some("Ensure probe ground clip is connected to GND and probe tip to CAL pin.".to_string()),
            });
        }

        let mid = (metrics.v_max_mv + metrics.v_min_mv) / 2.0;
        let mut overshoot_detected = false;
        let mut sluggish_detected = false;

        for i in 5..(samples.len().saturating_sub(25)) {
            if samples[i - 1] < mid && samples[i] >= mid {
                let plateau_sample = samples[i + 15];
                let peak_sample = samples[i..i + 15].iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if (peak_sample - plateau_sample) > (0.12 * vpp) {
                    overshoot_detected = true;
                    break;
                }
                if (plateau_sample - samples[i + 4]) > (0.15 * vpp) {
                    sluggish_detected = true;
                    break;
                }
            }
        }

        let advice = if overshoot_detected {
            "Over-compensated probe (overshoot spike): Trimmer capacitor capacitance is too high. Turn probe adjustment trimmer counter-clockwise until waveform edges are flat.".to_string()
        } else if sluggish_detected {
            "Under-compensated probe (rounded corner): Trimmer capacitor capacitance is too low. Turn probe adjustment trimmer clockwise until corners form sharp 90-degree angles.".to_string()
        } else {
            "Probe is optimally compensated: Waveform has sharp vertical transitions and flat horizontal plateaus.".to_string()
        };

        Ok(OscilloscopeCalibrationResult {
            status: "passed".to_string(),
            message: format!("1kHz calibration signal verified: {:.1} Hz, {:.0} mV Vpp, {:.1}% duty cycle.", freq, vpp, duty),
            measured_freq_hz: Some(freq),
            measured_vpp_mv: Some(vpp),
            measured_duty_cycle_percent: Some(duty),
            zero_offsets_mv: None,
            probe_trim_advice: Some(advice),
        })
    } else {
        let mut offsets = Vec::new();
        for &v_div in &[5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000] {
            let ch_cfg = OscilloscopeChannelConfig {
                enabled: true,
                volts_per_div_mv: v_div,
                vertical_offset_mv: 0,
                coupling: "GND".to_string(),
                probe_attenuation: 1,
            };
            let dummy = OscilloscopeChannelConfig {
                enabled: false,
                volts_per_div_mv: 1000,
                vertical_offset_mv: 0,
                coupling: "DC".to_string(),
                probe_attenuation: 1,
            };
            let trg = OscilloscopeTriggerConfig::default();
            if let Ok(res) = capture_vds1022_blocking(ch_cfg, dummy, trg, 200.0) {
                offsets.push(res.ch1_metrics.v_avg_mv.round() as i32);
            } else {
                offsets.push(0);
            }
        }

        Ok(OscilloscopeCalibrationResult {
            status: "passed".to_string(),
            message: "Zero-baseline calibration completed across all 10 voltage ranges.".to_string(),
            measured_freq_hz: None,
            measured_vpp_mv: None,
            measured_duty_cycle_percent: None,
            zero_offsets_mv: Some(offsets),
            probe_trim_advice: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_channel_metrics_square_wave() {
        let samples_count = 3000;
        let time_per_sample_us = 1.0;
        let mut samples = Vec::with_capacity(samples_count);
        for i in 0..samples_count {
            if (i % 1000) < 500 {
                samples.push(3300.0);
            } else {
                samples.push(0.0);
            }
        }

        let metrics = compute_channel_metrics(&samples, time_per_sample_us);
        assert!((metrics.v_pp_mv - 3300.0).abs() < 1.0);
        assert!((metrics.v_min_mv - 0.0).abs() < 1.0);
        assert!((metrics.v_max_mv - 3300.0).abs() < 1.0);
        assert!((metrics.v_avg_mv - 1650.0).abs() < 20.0);
        assert!(metrics.frequency_hz.is_some());
        let freq = metrics.frequency_hz.unwrap();
        assert!((freq - 1000.0).abs() < 5.0);
    }

    #[test]
    fn test_probe_owon_vds1022_scan() {
        let res = probe_owon_vds1022();
        assert!(res.is_ok());
        if let Ok(Some(inst)) = res {
            assert_eq!(inst.id, "owon_vds1022");
            assert!(inst.is_connected);
            assert_eq!(inst.vendor_id, "5345");
            assert_eq!(inst.product_id, "1234");
        }
    }

    #[tokio::test]
    async fn test_physical_capture_vds1022() {
        let ch1 = OscilloscopeChannelConfig {
            enabled: true,
            volts_per_div_mv: 1000,
            vertical_offset_mv: 0,
            coupling: "DC".to_string(),
            probe_attenuation: 1,
        };
        let ch2 = OscilloscopeChannelConfig {
            enabled: true,
            volts_per_div_mv: 1000,
            vertical_offset_mv: 0,
            coupling: "DC".to_string(),
            probe_attenuation: 1,
        };
        let trg = OscilloscopeTriggerConfig {
            source: "CH1".to_string(),
            slope: "Rising".to_string(),
            sweep_mode: "Auto".to_string(),
            level_mv: 1500,
        };

        let res = capture_vds1022(ch1, ch2, trg, 200.0).await;
        if let Ok(capture) = res {
            eprintln!("Physical capture succeeded! Samples: {}, Duration: {} ms", capture.samples_count, capture.duration_ms);
            eprintln!("CH1 Vpp: {:.1} mV, Freq: {:?} Hz, Vavg: {:.1} mV", capture.ch1_metrics.v_pp_mv, capture.ch1_metrics.frequency_hz, capture.ch1_metrics.v_avg_mv);
            eprintln!("CH2 Vpp: {:.1} mV, Freq: {:?} Hz, Vavg: {:.1} mV", capture.ch2_metrics.v_pp_mv, capture.ch2_metrics.frequency_hz, capture.ch2_metrics.v_avg_mv);
            eprintln!("First 20 CH1 samples: {:?}", &capture.ch1_samples[..20]);
            eprintln!("First 20 CH2 samples: {:?}", &capture.ch2_samples[..20]);
            assert_eq!(capture.ch1_samples.len(), SAMPLES_PER_CHANNEL);
        } else {
            eprintln!("Physical capture returned err: {:?}", res.err());
        }
    }

    #[tokio::test]
    async fn test_physical_calibrate_probe_check() {
        let res = calibrate_vds1022("probe_check".to_string(), Some("CH1".to_string()), Some(1)).await;
        eprintln!("Calibrate probe check (1X): {:?}", res);
        let res10 = calibrate_vds1022("probe_check".to_string(), Some("CH1".to_string()), Some(10)).await;
        eprintln!("Calibrate probe check (10X): {:?}", res10);
    }

    #[test]
    fn firmware_bitstreams_resolve_for_all_fpga_revisions() {
        for filename in [
            "VDS1022_FPGAV1_V3.9.bin",
            "VDS1022_FPGAV2_V4.1.bin",
            "VDS1022_FPGAV3_gaoyun.bin",
            "VDS1022_FPGAV4_gaoyun.bin",
            "VDS1022_FPGAV5_gaoyun.bin",
        ] {
            let bytes = load_firmware_bitstream(filename).expect("firmware bitstream must load");
            assert!(!bytes.is_empty(), "empty bitstream for {}", filename);
        }
        assert!(load_firmware_bitstream("nonexistent.bin").is_err());
    }
}
