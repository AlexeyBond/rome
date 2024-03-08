mod device;
mod device_detector;
mod data_ops;
mod file_io;

use std::io::Write;
use std::num::{NonZeroU8, NonZeroUsize};
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use crate::data_ops::{DataReadRequest, DEFAULT_READ_BUFFER_SIZE, read_data};
use crate::device::DeviceSettings;
use crate::device_detector::DeviceDetectorSettings;
use crate::file_io::open_output_stream;

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

    /// Data read/write operations
    Data {
        #[command(flatten)]
        detector_settings: DeviceDetectorSettings,

        #[command(subcommand)]
        command: DataCommand,
    },
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
    Version {
        #[command(flatten)]
        detector_settings: DeviceDetectorSettings,
    },
    Test {
        #[command(flatten)]
        detector_settings: DeviceDetectorSettings,

        /// Maximal duration of device test. It usually takes about 3 seconds.
        #[arg(long, value_parser = humantime::parse_duration, default_value = "10s")]
        test_timeout: Duration,
    },
}

#[derive(Subcommand)]
enum DataCommand {
    /// Read data from device
    Read {
        /// Address of the first byte to read.
        #[arg(long)]
        offset: Option<u16>,

        /// Number of bytes to read from device memory.
        #[arg(long)]
        size: Option<usize>,

        /// Size of read buffer.
        #[arg(long)]
        buffer_size: Option<u8>,

        /// A file to write the data to.
        /// If not defined, the result will be printed to standard output.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    Write {},
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
        Command::Device(DeviceCommand::Version { detector_settings }) => {
            let mut device = device_detector::detect_device(&detector_settings)?;

            device.send(b"V\n")?;
            let response = device.receive(64)?;

            if !matches!(response.first(), Some(c) if *c == b'V') {
                eprintln!("Received unexpected response: '{}'", String::from_utf8_lossy(response.as_slice()));
                exit(1);
            }

            println!("{}", String::from_utf8_lossy(&response.as_slice()[1..]));
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
        Command::Data { detector_settings, command } => {
            let mut device = device_detector::detect_device(&detector_settings)?;

            match command {
                DataCommand::Read { offset, size, output, buffer_size } => {
                    let mut stream = open_output_stream(output)?;
                    let offset = offset.unwrap_or(0u16);
                    let size = match size {
                        None => {
                            let device_size = device.memory_size()?;
                            NonZeroUsize::new(device_size - (offset as usize))
                        }
                        Some(sz) => NonZeroUsize::new(sz),
                    };
                    let size = match size {
                        None => {
                            return Ok(());
                        }
                        Some(nzsz) => nzsz
                    };
                    let buffer_size = match NonZeroU8::new(buffer_size.unwrap_or(DEFAULT_READ_BUFFER_SIZE)) {
                        Some(nz_bsz) => nz_bsz,
                        None => {
                            return Err(anyhow!("Illegal buffer size"));
                        }
                    };

                    for chunk_result in read_data(&mut device, DataReadRequest {
                        offset,
                        size,
                        buffer_size,
                    })? {
                        let chunk = chunk_result?;

                        stream.write(chunk.as_slice())?;
                    }
                }
                DataCommand::Write { .. } => {
                    todo!()
                }
            }
        }
    }

    Ok(())
}
