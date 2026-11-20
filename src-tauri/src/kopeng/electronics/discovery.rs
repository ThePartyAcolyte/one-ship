use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::kopeng::electronics::errors::ElectronicsError;

/// Discovered microcontroller or USB-serial bridge device on host.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiscoveredMcuPort {
    pub port_name: String,
    pub is_usb: bool,
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub manufacturer: Option<String>,
    pub product_name: Option<String>,
    pub serial_number: Option<String>,
    pub description: String,
}

/// Known test equipment VID:PID pairs that must be excluded from target microcontrollers.
const INSTRUMENT_USB_IDS: &[(u16, u16)] = &[
    (0x5345, 0x1234), // Owon VDS1022 / VDS1022I Oscilloscope
    (0x0925, 0x3881), // Saleae / fx2lafw Logic Analyzer
    (0x04b4, 0x8613), // Cypress FX2LP generic
];

/// Enumerates connected physical serial ports and USB-to-UART bridges.
pub fn scan_connected_microcontrollers() -> Result<Vec<DiscoveredMcuPort>, ElectronicsError> {
    let ports = serialport::available_ports().map_err(|e| ElectronicsError::SerialError {
        port: "scan".to_string(),
        details: e.to_string(),
    })?;

    let mut result = Vec::with_capacity(ports.len());

    for p in ports {
        match p.port_type {
            serialport::SerialPortType::UsbPort(info) => {
                // Filter out test instruments (Oscilloscopes, Logic Analyzers)
                if INSTRUMENT_USB_IDS.iter().any(|&(v, pid)| v == info.vid && pid == info.pid) {
                    continue;
                }

                let vid_hex = format!("{:04x}", info.vid);
                let pid_hex = format!("{:04x}", info.pid);
                let mfg = info.manufacturer.clone();
                let prod = info.product.clone();

                let desc = match (&prod, &mfg) {
                    (Some(pr), Some(mf)) => format!("{} - {} ({})", mf, pr, p.port_name),
                    (Some(pr), None) => format!("{} ({})", pr, p.port_name),
                    _ => format!("USB Serial [{}:{}] ({})", vid_hex, pid_hex, p.port_name),
                };

                result.push(DiscoveredMcuPort {
                    port_name: p.port_name,
                    is_usb: true,
                    vendor_id: Some(vid_hex),
                    product_id: Some(pid_hex),
                    manufacturer: mfg,
                    product_name: prod,
                    serial_number: info.serial_number,
                    description: desc,
                });
            }
            _ => {
                // Ignore Linux dummy motherboard UARTs (/dev/ttyS0 through /dev/ttyS31)
                if p.port_name.starts_with("/dev/ttyS") {
                    continue;
                }

                let desc = format!("Standard Serial Port ({})", p.port_name);
                result.push(DiscoveredMcuPort {
                    port_name: p.port_name,
                    is_usb: false,
                    vendor_id: None,
                    product_id: None,
                    manufacturer: None,
                    product_name: None,
                    serial_number: None,
                    description: desc,
                });
            }
        }
    }

    Ok(result)
}
