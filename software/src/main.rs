mod device;
mod device_detector;

use std::process::exit;
use std::time::Duration;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use crate::device::DeviceSettings;
use crate::device_detector::DeviceDetectorSettings;

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

    /// Device management operations
    #[command(subcommand)]
    Device(DeviceCommand),
}

#[derive(Subcommand)]
enum PortCommand {
    /// List ports that may be occupied by ROME
    List,
    /// Detect port occupied by ROME
    Detect(DeviceSettings),
}

#[derive(Subcommand)]
enum DeviceCommand {
    Test {
        #[command(flatten)]
        detector_settings: DeviceDetectorSettings,

        /// Maximal duration of device test. It usually takes about 3 seconds.
        #[arg(long, value_parser = humantime::parse_duration, default_value = "10s")]
        test_timeout: Duration,
    },
}

fn main() -> Result<()> {
    let args: TheArgs = TheArgs::parse();
    match args.command {
        Command::Port(PortCommand::List) => {
            let ports = device_detector::list_potential_devices()?;

            if ports.is_empty() {
                return Err(anyhow!("No ports found"));
            }

            for port_info in ports {
                println!("{}", port_info.port_name);
            }
        }
        Command::Port(PortCommand::Detect(device_settings)) => {
            let device = device_detector::safe_detect_device(&device_settings)?;

            println!("{}", device.name());
        }
        Command::Device(DeviceCommand::Test { test_timeout, detector_settings }) => {
            let mut device = device_detector::detect_device(&detector_settings)?;
            device.send(b"T\n")?;
            match device.receive_with_timeout(128, test_timeout)?.as_slice() {
                b"TOK" => {
                    eprintln!("Test passed");
                }
                b"TFAIL" => {
                    eprintln!("Test failed");
                    exit(1);
                }
                response => {
                    eprintln!("Received unexpected response: '{}'", String::from_utf8_lossy(response));
                    exit(1);
                }
            }
        }
    }

    Ok(())
}
