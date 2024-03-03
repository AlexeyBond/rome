use std::io::{BufReader, ErrorKind, Read, Write};
use std::time::Duration;
use anyhow::{anyhow, Context, Result};
use clap::Args;
use serialport::SerialPort;

#[derive(Copy, Clone, Args)]
pub struct DeviceSettings {
    /// Port baud rate
    #[arg(long, default_value_t = 2_000_000)]
    pub baud_rate: u32,

    /// Default read timeout
    #[arg(long, value_parser = humantime::parse_duration, default_value = "1s")]
    pub read_timeout: Duration,

    /// Read timeout for first read operation
    #[arg(long, value_parser = humantime::parse_duration, default_value = "2s")]
    pub initial_read_timeout: Duration,

    /// Show debug messages received from device
    #[arg(long, default_value_t = false)]
    pub show_debug_messages: bool,
}

pub struct Device {
    name: String,
    settings: DeviceSettings,
    default_timeout_applied: bool,
    port: BufReader<Box<dyn SerialPort>>,
}

impl Device {
    pub fn new(port_name: &str, settings: &DeviceSettings) -> Result<Self> {
        let port = serialport::new(port_name, settings.baud_rate)
            .timeout(settings.initial_read_timeout)
            .open()
            .context("Error opening port")?;

        Ok(Self {
            name: port_name.to_string(),
            settings: settings.clone(),
            default_timeout_applied: false,
            port: BufReader::new(port),
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn send(&mut self, command: &[u8]) -> Result<()> {
        self.port.get_mut().write(command)
            .context("Error sending command")?;
        self.port.get_mut().flush()?;

        Ok(())
    }

    pub fn receive(&mut self, limit: usize) -> Result<Vec<u8>> {
        let mut line = vec![];

        loop {
            let mut buf: [u8; 1] = [0; 1];

            loop {
                if buf.len() > limit {
                    return Err(anyhow!("Response size exceeds limit of {} bytes", limit));
                }

                let sz = self.port.get_mut().read(&mut buf)?;
                if sz != 0 {
                    if buf[0] == b'\n' {
                        break;
                    } else {
                        line.push(buf[0]);
                    }
                }

                if !self.default_timeout_applied {
                    self.port.get_mut().set_timeout(self.settings.read_timeout)?;
                }
            }

            match line.first().cloned() {
                None => { continue; }
                Some(b'#') => {
                    if self.settings.show_debug_messages {
                        eprintln!("debug -> {}", String::from_utf8_lossy(line.as_slice()));
                    }
                    line.clear();
                }
                Some(b'!') => {
                    return Err(anyhow!(
                        "Device returned error: {}",
                        String::from_utf8_lossy(&line.as_slice()[1..]).trim(),
                    ));
                }
                Some(_) => {
                    return Ok(line);
                }
            }
        }
    }

    pub fn check(&mut self) -> Result<()> {
        self.send(b"V\n")?;
        let response = match self.receive(24) {
            Ok(r) => r,
            Err(e) => match e.downcast_ref::<std::io::Error>() {
                Some(err) if err.kind() == ErrorKind::TimedOut => {
                    eprintln!("Got timeout, re-sending 'V' command...");
                    self.send(b"V\n")?;
                    self.receive(24)?
                },
                _ => {
                    return Err(e);
                }
            }
        };

        if !response.starts_with(b"VROME") {
            return Err(anyhow!(
                "Unexpected response for 'V' command: {}",
                String::from_utf8_lossy(response.as_slice())
            ));
        }

        Ok(())
    }
}
