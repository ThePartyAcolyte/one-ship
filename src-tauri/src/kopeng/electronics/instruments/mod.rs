pub mod driver;
pub mod arbitrator;
pub mod owon_vds1022;
pub mod fx2lafw;

use serde::Deserialize;
use ts_rs::TS;
use crate::kopeng::electronics::errors::ElectronicsError;
use crate::kopeng::electronics::instruments::driver::{
    DiscoveredInstrument, OscilloscopeChannelConfig, OscilloscopeTriggerConfig,
    OscilloscopeCaptureResult, OscilloscopeCalibrationResult, LogicCaptureResult,
    OscilloscopeDescriptor,
};

/// Arguments for `capture_oscilloscope`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CaptureOscilloscopeArgs {
    pub ch1: Option<OscilloscopeChannelConfig>,
    pub ch2: Option<OscilloscopeChannelConfig>,
    pub trigger: Option<OscilloscopeTriggerConfig>,
    pub timebase_div_us: f64,
}

/// Arguments for `calibrate_oscilloscope`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CalibrateOscilloscopeArgs {
    pub mode: String, // "probe_check" | "zero_offset"
    pub channel: Option<String>,
    pub probe_attenuation: Option<u32>,
}

/// Arguments for `capture_logic_analyzer`.
#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct CaptureLogicAnalyzerArgs {
    pub sample_rate_hz: u64,
    pub duration_ms: u64,
    pub decoder: Option<String>,
    pub decoder_pins: Option<String>,
}

/// Scans for connected Owon VDS1022 and 24MHz 8-channel logic analyzer instruments.
pub async fn scan_connected_instruments() -> Result<Vec<DiscoveredInstrument>, ElectronicsError> {
    tokio::task::spawn_blocking(|| {
        let mut instruments = Vec::new();

        match owon_vds1022::probe_owon_vds1022() {
            Ok(Some(scope)) => instruments.push(scope),
            Ok(None) => {}
            Err(e) => crate::logger::log("WARN", "electronics", &format!("probe_owon_vds1022 failed: {}", e)),
        }

        match fx2lafw::probe_fx2_logic_analyzer() {
            Ok(Some(la)) => instruments.push(la),
            Ok(None) => {}
            Err(e) => crate::logger::log("WARN", "electronics", &format!("probe_fx2_logic_analyzer failed: {}", e)),
        }

        Ok(instruments)
    })
    .await
    .map_err(|e| ElectronicsError::Io(format!("Instrument scan task failed: {}", e)))?
}

/// Returns capability manifests for all compiled oscilloscope drivers.
pub async fn list_supported_oscilloscopes() -> Result<Vec<OscilloscopeDescriptor>, ElectronicsError> {
    Ok(vec![owon_vds1022::descriptor()])
}

/// Acquires an analog capture from the connected oscilloscope.
pub async fn capture_oscilloscope(
    args: CaptureOscilloscopeArgs,
) -> Result<OscilloscopeCaptureResult, ElectronicsError> {
    let ch1 = args.ch1.unwrap_or(OscilloscopeChannelConfig {
        enabled: false,
        ..Default::default()
    });
    let ch2 = args.ch2.unwrap_or(OscilloscopeChannelConfig {
        enabled: false,
        ..Default::default()
    });
    let trigger = args.trigger.unwrap_or_default();

    owon_vds1022::capture_vds1022(ch1, ch2, trigger, args.timebase_div_us).await
}

/// Executes calibration checks or zero-offset calibration on the oscilloscope.
pub async fn calibrate_oscilloscope(
    args: CalibrateOscilloscopeArgs,
) -> Result<OscilloscopeCalibrationResult, ElectronicsError> {
    owon_vds1022::calibrate_vds1022(args.mode, args.channel, args.probe_attenuation).await
}

/// Acquires digital traces and decodes protocol packets from the logic analyzer.
pub async fn capture_logic_analyzer(
    args: CaptureLogicAnalyzerArgs,
) -> Result<LogicCaptureResult, ElectronicsError> {
    fx2lafw::capture_fx2_logic(
        args.sample_rate_hz,
        args.duration_ms,
        args.decoder,
        args.decoder_pins,
    ).await
}
