//! Just enough of the GGUF format to read a model's metadata keys without loading it.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use anyhow::{bail, Result};

const STRING: u32 = 8;
const ARRAY: u32 = 9;

/// Whether the GGUF file at `path` is an embedding model. Like Ollama, a model counts as one
/// when its metadata sets a pooling type (`bert.pooling_type`, `nomic-bert.pooling_type`, ...).
pub fn is_embedding_model(path: &Path) -> Result<bool> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != b"GGUF" {
        bail!("{} is not a GGUF file", path.display());
    }
    let version = read_u32(&mut reader)?;
    if version < 2 {
        bail!("GGUF version {version} is not supported");
    }
    let _tensor_count = read_u64(&mut reader)?;
    let kv_count = read_u64(&mut reader)?;
    for _ in 0..kv_count {
        let key = read_string(&mut reader)?;
        if key.ends_with(".pooling_type") {
            return Ok(true);
        }
        let kind = read_u32(&mut reader)?;
        skip_value(&mut reader, kind)?;
    }
    Ok(false)
}

fn skip_value(reader: &mut BufReader<File>, kind: u32) -> Result<()> {
    match kind {
        STRING => {
            let len = read_u64(reader)?;
            reader.seek_relative(len as i64)?;
        }
        ARRAY => {
            let item_kind = read_u32(reader)?;
            let count = read_u64(reader)?;
            match fixed_size(item_kind) {
                Some(size) => reader.seek_relative((size * count) as i64)?,
                None => {
                    for _ in 0..count {
                        skip_value(reader, item_kind)?;
                    }
                }
            }
        }
        _ => match fixed_size(kind) {
            Some(size) => reader.seek_relative(size as i64)?,
            None => bail!("unknown GGUF value type {kind}"),
        },
    }
    Ok(())
}

fn fixed_size(kind: u32) -> Option<u64> {
    match kind {
        0 | 1 | 7 => Some(1),
        2 | 3 => Some(2),
        4..=6 => Some(4),
        10..=12 => Some(8),
        _ => None,
    }
}

fn read_u32(reader: &mut impl Read) -> Result<u32> {
    let mut bytes = [0u8; 4];
    reader.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64(reader: &mut impl Read) -> Result<u64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_string(reader: &mut impl Read) -> Result<String> {
    let len = read_u64(reader)?;
    if len > 1 << 16 {
        bail!("GGUF key is {len} bytes long");
    }
    let mut bytes = vec![0u8; len as usize];
    reader.read_exact(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
