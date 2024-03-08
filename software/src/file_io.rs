use std::fs::File;
use std::io::{stdout, Write};
use std::path::PathBuf;
use anyhow::Result;

pub fn open_output_stream(path: Option<PathBuf>) -> Result<Box<dyn Write>> {
    Ok(match path {
        None => {
            Box::new(stdout())
        }
        Some(path) => {
            Box::new(File::create(path)?)
        }
    })
}
