mod device;
mod device_detector;

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use crate::device::DeviceSettings;

#[derive(Parser)]
struct TheArgs {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Serial port operations
    #[command(subcommand)]
    Port(PortCommand),
}

#[derive(Subcommand)]
enum PortCommand {
    /// List ports that may be occupied by ROME
    List,
    /// Detect port occupied by ROME
    Detect(DeviceSettings),
}

fn main() -> Result<()> {
    let args: TheArgs = TheArgs::parse();
    match args.command {
        Command::Port(PortCommand::List) => {
            let ports = device_detector::list_potential_devices()?;

            if ports.is_empty() {
                return Err(anyhow!("No ports found"))
            }

            for port_info in ports {
                println!("{}", port_info.port_name);
            }
        },
        Command::Port(PortCommand::Detect(device_settings)) => {
            let device = device_detector::safe_detect_device(&device_settings)?;

            println!("{}", device.name());
        },
    }

    Ok(())
}
