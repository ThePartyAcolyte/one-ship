use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Discovered physical instrument connected to the host.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiscoveredInstrument {
    pub id: String,
    pub name: String,
    pub category: String, // "oscilloscope" | "logic_analyzer"
    pub vendor_id: String,
    pub product_id: String,
    pub is_connected: bool,
    pub transport: String,
}

/// Declarative capability manifest and physical specifications of an oscilloscope driver.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeDescriptor {
    pub id: String,
    pub name: String,
    pub vendor: String,
    pub model: String,
    pub channel_count: u8,
    pub channel_names: Vec<String>,
    pub volts_per_div_options_mv: Vec<u32>,
    pub couplings: Vec<String>,
    pub probe_attenuations: Vec<u32>,
    pub timebase_steps_us: Vec<f64>,
    pub min_timebase_us: f64,
    pub max_timebase_us: f64,
    pub max_sample_rate_hz: u64,
    pub bandwidth_hz: u64,
    pub buffer_points_count: usize,
    pub has_trigger: bool,
    pub trigger_sources: Vec<String>,
    pub trigger_modes: Vec<String>,
    pub trigger_slopes: Vec<String>,
    pub has_hardware_calibration: bool,
    pub has_signal_generator: bool,
}

/// Channel acquisition configuration for digital oscilloscopes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeChannelConfig {
    pub enabled: bool,
    pub volts_per_div_mv: u32,
    pub vertical_offset_mv: i32,
    pub coupling: String, // "DC" | "AC" | "GND"
    pub probe_attenuation: u32, // 1 or 10
}

impl Default for OscilloscopeChannelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            volts_per_div_mv: 1000,
            vertical_offset_mv: 0,
            coupling: "DC".to_string(),
            probe_attenuation: 1,
        }
    }
}

/// Trigger settings for digital storage oscilloscopes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeTriggerConfig {
    pub source: String, // "CH1" | "CH2" | "EXT"
    pub slope: String, // "rising" | "falling"
    pub sweep_mode: String, // "auto" | "normal" | "single"
    pub level_mv: i32,
}

impl Default for OscilloscopeTriggerConfig {
    fn default() -> Self {
        Self {
            source: "CH1".to_string(),
            slope: "rising".to_string(),
            sweep_mode: "auto".to_string(),
            level_mv: 0,
        }
    }
}

/// Calculated scalar electrical metrics for a captured analog channel.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeMetrics {
    pub v_pp_mv: f64,
    pub v_rms_mv: f64,
    pub v_min_mv: f64,
    pub v_max_mv: f64,
    pub v_avg_mv: f64,
    pub frequency_hz: Option<f64>,
    pub period_us: Option<f64>,
    pub duty_cycle_percent: Option<f64>,
}

impl Default for OscilloscopeMetrics {
    fn default() -> Self {
        Self {
            v_pp_mv: 0.0,
            v_rms_mv: 0.0,
            v_min_mv: 0.0,
            v_max_mv: 0.0,
            v_avg_mv: 0.0,
            frequency_hz: None,
            period_us: None,
            duty_cycle_percent: None,
        }
    }
}

/// Result of an oscilloscope capture including waveform samples and metrics.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeCaptureResult {
    pub ch1_samples: Vec<f64>,
    pub ch2_samples: Vec<f64>,
    pub ch1_metrics: OscilloscopeMetrics,
    pub ch2_metrics: OscilloscopeMetrics,
    pub time_per_div_us: f64,
    pub sample_rate_hz: u64,
    pub samples_count: usize,
    pub duration_ms: u64,
}

/// Result of an oscilloscope calibration check or zero-offset adjustment.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OscilloscopeCalibrationResult {
    pub status: String, // "passed" | "warning" | "failed"
    pub message: String,
    pub measured_freq_hz: Option<f64>,
    pub measured_vpp_mv: Option<f64>,
    pub measured_duty_cycle_percent: Option<f64>,
    pub zero_offsets_mv: Option<Vec<i32>>,
    pub probe_trim_advice: Option<String>,
}

/// Decoded protocol packet or frame from a logic analyzer stream.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DecodedProtocolPacket {
    pub start_sample_index: u64,
    pub end_sample_index: u64,
    pub proto: String,
    pub summary: String,
    pub data_hex: Option<String>,
}

/// Result of a digital logic analyzer acquisition.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LogicCaptureResult {
    pub channels: Vec<Vec<u8>>,
    pub sample_rate_hz: u64,
    pub samples_count: usize,
    pub decoded_packets: Vec<DecodedProtocolPacket>,
    pub wavedrom_json: Option<String>,
    pub duration_ms: u64,
}
