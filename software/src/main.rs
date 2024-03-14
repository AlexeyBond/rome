mod device;
mod device_detector;
mod data_ops;
mod file_io;

use std::io::{Read, Write};
use std::num::{NonZeroU8, NonZeroUsize};
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use anyhow::{anyhow, Result};
use clap::{Args, Parser, Subcommand};
use crate::data_ops::{DataChunk, DataReadRequest, DataWriteRequest, read_data, write_data};
use crate::device::{Device, DeviceSettings};
use crate::device_detector::DeviceDetectorSettings;
use crate::file_io::{open_input_stream, open_output_stream};

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

        #[command(flatten)]
        external_control_settings: ExternalControlSettings,
    },
    EnableExternalControl {
        #[command(flatten)]
        detector_settings: DeviceDetectorSettings,
    },
}

#[derive(Subcommand)]
enum DataCommand {
    /// Read data from device
    Read {
        /// Address of the first byte to read.
        #[arg(long, default_value_t = 0u16)]
        offset: u16,

        /// Number of bytes to read from device memory.
        ///
        /// By default, all data from --offset to the end of device address space will be read.
        #[arg(long)]
        size: Option<usize>,

        /// Size of read buffer.
        #[arg(long, default_value_t = crate::data_ops::DEFAULT_READ_BUFFER_SIZE)]
        buffer_size: u8,

        /// A file to write the data to.
        ///
        /// If not defined, the result will be printed to standard output.
        #[arg(long)]
        output: Option<PathBuf>,

        #[command(flatten)]
        external_control_settings: ExternalControlSettings,
    },
    /// Write data to device
    Write {
        /// Address of first byte to write.
        #[arg(long, default_value_t = 0u16)]
        offset: u16,

        /// Size of buffer used during write operation.
        ///
        /// Defaults to a value safe to use with Arduino's default serial receive buffer size.
        #[arg(long, default_value_t = crate::data_ops::DEFAULT_WRITE_BUFFER_SIZE)]
        buffer_size: u8,

        /// Path to input file.
        ///
        /// If not specified, the standard input will be used.
        #[arg(long)]
        input: Option<PathBuf>,

        /// Verify written data after writing.
        ///
        /// If set, the program will read all written data back from the device and compare it with
        /// the data that should have been written.
        /// If the data received from device differs, the program will exit with a non-zero code.
        #[arg(long)]
        verify: bool,

        /// Size of buffer used for read operations during write result validation.
        #[arg(long, default_value_t = crate::data_ops::DEFAULT_READ_BUFFER_SIZE)]
        verification_read_buffer_size: u8,

        #[command(flatten)]
        external_control_settings: ExternalControlSettings,
    },
}

#[derive(Args)]
struct ExternalControlSettings {
    /// Do not switch to external control after operation completion.
    #[arg(long)]
    no_external_control: bool,
}

impl ExternalControlSettings {
    fn apply(&self, device: &mut Device) -> Result<()> {
        if !self.no_external_control {
            device.enable_external_control()?;
        }

        Ok(())
    }
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
        Command::Device(DeviceCommand::Test {
                            test_timeout,
                            detector_settings,
                            external_control_settings,
                        }) => {
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

            external_control_settings.apply(&mut device)?;
        }
        Command::Device(DeviceCommand::EnableExternalControl { detector_settings }) => {
            device_detector::detect_device(&detector_settings)?.enable_external_control()?;
        }
        Command::Data { detector_settings, command } => {
            let mut device = device_detector::detect_device(&detector_settings)?;

            match command {
                DataCommand::Read {
                    offset,
                    size,
                    output,
                    buffer_size,
                    external_control_settings,
                } => {
                    let mut stream = open_output_stream(output)?;
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
                    let buffer_size = match NonZeroU8::new(buffer_size) {
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

                        stream.write(chunk.data.as_slice())?;
                    }

                    external_control_settings.apply(&mut device)?;
                }
                DataCommand::Write {
                    input,
                    offset,
                    buffer_size,
                    verify,
                    verification_read_buffer_size,
                    external_control_settings,
                } => {
                    let buffer_size = match NonZeroU8::new(buffer_size) {
                        None => {
                            return Err(anyhow!("Illegal buffer size"));
                        }
                        Some(bsz) => bsz,
                    };
                    let verification_read_buffer_size = match NonZeroU8::new(verification_read_buffer_size) {
                        None => {
                            return Err(anyhow!("Illegal verification buffer size"));
                        }
                        Some(bsz) => bsz,
                    };

                    let mut data = vec![];
                    open_input_stream(input)?.read_to_end(&mut data)?;

                    if data.is_empty() {
                        eprintln!("Empty input data file or stream provided. Exiting without writing anything.");
                        return Ok(());
                    }

                    if (offset as usize) + data.len() > device.memory_size()? {
                        return Err(anyhow!(
                            "Data file size is too large: 0x{:X} bytes of data supplied at offset 0x{:04X}. Total device memory size is 0x{:X}",
                            data.len(),
                            offset,
                            device.memory_size()?,
                        ));
                    }

                    write_data(&mut device, DataWriteRequest {
                        data: &DataChunk {
                            data: data.as_slice(),
                            offset,
                        },
                        buffer_size,
                    })?;

                    if verify {
                        eprintln!("Verifying written data...");

                        for read_chunk in read_data(
                            &mut device,
                            DataReadRequest {
                                offset,
                                size: NonZeroUsize::new(data.len()).unwrap(),
                                buffer_size: verification_read_buffer_size,
                            },
                        )? {
                            let read_chunk = read_chunk?;
                            let chunk_offset = offset.wrapping_add(read_chunk.offset);
                            let required_data = &data.as_slice()[(chunk_offset as usize)..(chunk_offset as usize + read_chunk.data.len())];

                            if read_chunk.data.as_slice() != required_data {
                                return Err(anyhow!(
                                    "Verification failed in range {:04X}:{:04X}",
                                    chunk_offset,
                                    chunk_offset as usize + read_chunk.data.len(),
                                ));
                            }
                        }
                    }

                    external_control_settings.apply(&mut device)?;
                }
            }
        }
    }

    Ok(())
}
