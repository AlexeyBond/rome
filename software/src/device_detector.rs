use anyhow::{anyhow, Context, Result};
use clap::Args;
use serialport::{available_ports, SerialPortInfo, SerialPortType};
use crate::device::{Device, DeviceSettings};

#[derive(Clone, Args)]
pub struct DeviceDetectorSettings {
    /// Name of known device port to use
    #[arg(long, short)]
    port: Option<String>,

    #[command(flatten)]
    device_settings: DeviceSettings,
}

pub fn list_potential_devices() -> Result<Vec<SerialPortInfo>> {
    let ports = available_ports()
        .context("Error listing available ports")?;

    Ok(ports.into_iter()
        .filter(|port| matches!(port.port_type, SerialPortType::UsbPort(_)))
        .collect())
}

fn create_and_check_device(name: &str, settings: &DeviceSettings) -> Result<Device> {
    let mut device = Device::new(name, settings)?;
    device.check().context("Error checking device")?;
    return Ok(device);
}

pub fn safe_detect_device(settings: &DeviceSettings) -> Result<Device> {
    let candidates = list_potential_devices()?;

    if candidates.len() > 1 {
        return Err(anyhow!("More than one serial device connected"));
    }

    if let Some(port_info) = candidates.first() {
        return create_and_check_device(port_info.port_name.as_str(), settings);
    }

    Err(anyhow!("No devices connected"))
}
