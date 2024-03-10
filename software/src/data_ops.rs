use std::cmp::min;
use std::num::{NonZeroU8, NonZeroUsize};
use std::str::from_utf8;
use std::io::Write;
use anyhow::{anyhow, Context, Result};
use crate::device::Device;

// (64 bytes of arduino read buffer - 'R' - '\n') / 2 digits per byte of data
pub const DEFAULT_READ_BUFFER_SIZE: u8 = (64 - 2) / 2;

// (64 bytes - 'W' - '\n' - 4 address digits) / 2 digits per byte of data
pub const DEFAULT_WRITE_BUFFER_SIZE: u8 = (64 - 2 - 4) / 2;

pub struct DataChunk<T: AsRef<[u8]>> {
    pub offset: u16,
    pub data: T,
}

#[derive(Copy, Clone)]
pub struct DataReadRequest {
    pub offset: u16,
    pub size: NonZeroUsize,
    pub buffer_size: NonZeroU8,
}

pub fn read_data<'a>(
    device: &'a mut Device,
    request: DataReadRequest,
) -> Result<impl Iterator<Item=Result<DataChunk<Vec<u8>>>> + 'a> {
    if (request.size.get() + request.offset as usize) > device.memory_size()? {
        return Err(anyhow!("Last requested byte address is outside of device address range (offset + size - 1 > total memory size)"));
    }

    let num_segments = request.size.get().div_ceil(request.buffer_size.get().into()) as u16;

    Ok((0..num_segments)
        .map(move |segment_number| {
            let segment_start_address = request.offset + segment_number * (request.buffer_size.get() as u16);
            let remaining_size = request.size.get() - (segment_start_address - request.offset) as usize;
            let segment_size: u8 = min::<usize>(request.buffer_size.get().into(), remaining_size) as u8;

            device.send(format!("R{:04X}{:02X}\n", segment_start_address, segment_size).as_bytes())?;
            let response = device.receive(2 + (segment_size as usize) * 2)?;

            if !response.as_slice().starts_with(b"R") {
                return Err(anyhow!(
                    "Received unexpected response to 'R' command: '{}'",
                    String::from_utf8_lossy(response.as_slice()),
                ));
            }

            let response_payload = &response.as_slice()[1..];

            if response_payload.len() != 2 * (segment_size as usize) {
                return Err(anyhow!(
                    "Received payload of unexpected length ({} instead of {})",
                    response_payload.len(),
                    segment_size * 2,
                ));
            }

            Ok(DataChunk {
                offset: segment_start_address,
                data: response_payload
                    .chunks(2)
                    .map(|chunk| {
                        assert_eq!(chunk.len(), 2);

                        Ok(u8::from_str_radix(from_utf8(chunk)?, 16)?)
                    })
                    .collect::<Result<Vec<u8>>>()
                    .context("Error parsing response payload")?,
            })
        }))
}

pub struct DataWriteRequest<'a, T: AsRef<[u8]>> {
    pub data: &'a DataChunk<T>,
    pub buffer_size: NonZeroU8,
}

pub fn write_data<T: AsRef<[u8]>>(
    device: &mut Device,
    request: DataWriteRequest<T>
) -> Result<()> {
    let mut address = request.data.offset;

    for sub_chunk in request.data.data.as_ref().chunks(request.buffer_size.get() as usize) {
        let end_address = address.wrapping_add(sub_chunk.len() as u16);
        let mut command = format!("W{:04X}", address).into_bytes();

        for b in sub_chunk {
            write!(command, "{:02X}", b)?;
        }

        write!(command, "\n")?;

        device.send(command.as_slice())?;
        let response = device.receive(16)?;
        let expected_response = format!("W{:04X}{:04X}", address, end_address);

        if response.as_slice() != expected_response.as_bytes() {
            return Err(anyhow!(
                "Unexpected write command response: '{}', expected '{}'",
                String::from_utf8_lossy(&response),
                expected_response,
            ));
        }

        address = end_address;
    }

    Ok(())
}
