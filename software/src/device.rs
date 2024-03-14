use std::io::{ErrorKind, Read, Write};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use anyhow::{anyhow, Context, Error, Result};
use clap::Args;
use serialport::SerialPort;

#[derive(Copy, Clone, Args)]
pub struct DeviceSettings {
    /// Port baud rate
    #[arg(long, default_value_t = 250_000)]
    pub baud_rate: u32,

    /// Default I/O operations timeout
    #[arg(long, value_parser = humantime::parse_duration, default_value = "1s")]
    pub timeout: Duration,

    /// Read timeout for first I/O operation.
    ///
    /// For some reason, first operation after connecting the device may take more time than normal,
    /// at least on windows.
    #[arg(long, value_parser = humantime::parse_duration, default_value = "2s")]
    pub initial_timeout: Duration,

    /// Show info (starting with #) messages received from device
    #[arg(long, default_value_t = false)]
    pub show_info_messages: bool,

    /// Show all messages exchange between this program and the device
    #[arg(long)]
    pub show_all_messages: bool,

    /// Timeout for stream synchronization operation
    #[arg(long, value_parser = humantime::parse_duration, default_value = "2s")]
    pub sync_timeout: Duration,
}

pub struct Device {
    name: String,
    settings: DeviceSettings,
    default_timeout_applied: bool,
    port: Box<dyn SerialPort>,
}

fn is_timeout(err: &Error) -> bool {
    if let Some(io_error) = err.root_cause().downcast_ref::<std::io::Error>() {
        return io_error.kind() == ErrorKind::TimedOut;
    }

    return false;
}

impl Device {
    pub fn new(port_name: &str, settings: &DeviceSettings) -> Result<Self> {
        let port = serialport::new(port_name, settings.baud_rate)
            .timeout(settings.initial_timeout)
            .open()
            .context("Error opening port")?;

        Ok(Self {
            name: port_name.to_string(),
            settings: settings.clone(),
            default_timeout_applied: false,
            port,
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn send(&mut self, command: &[u8]) -> Result<()> {
        if self.settings.show_all_messages {
            eprintln!("sending: {}", String::from_utf8_lossy(command).trim_end());
        }

        self.port.write(command)
            .context("Error sending command")?;
        self.port.flush()?;

        Ok(())
    }

    fn show_inbound_message(&self, msg: &[u8]) {
        if self.settings.show_all_messages || (self.settings.show_info_messages && matches!(msg.first(), Some(c) if *c == b'#')) {
            eprintln!("received: {}", String::from_utf8_lossy(msg));
        }
    }

    fn receive_line_raw(&mut self, buffer: &mut Vec<u8>, limit: usize) -> Result<()> {
        let mut b: [u8; 1] = [0; 1];

        loop {
            if buffer.len() > limit {
                return Err(anyhow!("Response size exceeds limit of {} bytes", limit));
            }

            if self.port.read(&mut b)? != 0 {
                if b[0] == b'\n' {
                    self.show_inbound_message(buffer.as_slice());
                    return Ok(());
                } else {
                    buffer.push(b[0]);
                }
            }

            if !self.default_timeout_applied {
                self.port.set_timeout(self.settings.timeout)?;
            }
        }
    }

    pub fn receive(&mut self, limit: usize) -> Result<Vec<u8>> {
        let mut line = vec![];

        loop {
            self.receive_line_raw(&mut line, limit)?;

            match line.first().cloned() {
                None => { continue; }
                Some(b'#') => {
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

    pub fn receive_with_timeout(&mut self, limit: usize, timeout: Duration) -> Result<Vec<u8>> {
        let end_time = Instant::now() + timeout;

        loop {
            match self.receive(limit) {
                Err(e) if is_timeout(&e) && Instant::now() < end_time => {},
                res => { return res; }
            }
        }
    }

    fn sync(&mut self) -> Result<()> {
        let message = format!("\nP{}\n", SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros());
        self.send(message.as_bytes())?;

        let sync_deadline = SystemTime::now() + self.settings.sync_timeout;

        let mut receive_buffer = vec![];

        let expected_payload = &message.trim().as_bytes()[1..];

        loop {
            self.receive_line_raw(&mut receive_buffer, 64)?;

            if matches!(receive_buffer.first(), Some(c) if *c == b'p') {
                if &receive_buffer[1..] == expected_payload {
                    return Ok(());
                }
            }

            if SystemTime::now() > sync_deadline {
                return Err(anyhow!("Sync timeout exceeded"));
            }

            receive_buffer.clear();
        }
    }

    pub fn check(&mut self) -> Result<()> {
        if let Err(e) = self.sync() {
            if is_timeout(&e) {
                eprintln!("Got timeout, trying to synchronize again...");
                self.sync()
                    .context("Error synchronizing with device - it did not respond correctly to ping message")?;
            }
        };

        self.send(b"V\n")?;
        let response = self.receive(24)?;
        if !response.starts_with(b"VROME") {
            return Err(anyhow!(
                "Unexpected response for 'V' command: {}",
                String::from_utf8_lossy(response.as_slice())
            ));
        }

        Ok(())
    }

    pub fn enable_external_control(&mut self) -> Result<()> {
        self.send(b"E\n")?;

        match self.receive(64)?.as_slice() {
            b"EOK" => Ok(()),
            x => Err(anyhow!(
                "Unexpected response received: {}",
                String::from_utf8_lossy(x),
            ))
        }
    }

    pub fn memory_size(&mut self) -> Result<usize> {
        // TODO: Support devices with 32KiB (24257) memory (?)
        Ok(0x10000)
    }
}
