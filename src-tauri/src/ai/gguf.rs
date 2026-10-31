//! Just enough of the GGUF format to read a model's metadata keys without loading it.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use anyhow::{anyhow, bail, Result};

const STRING: u32 = 8;
const ARRAY: u32 = 9;
/// Arrays of arrays deeper than this only come from a corrupt file.
const MAX_DEPTH: usize = 4;

/// Whether the GGUF file at `path` is an embedding model. Like Ollama, a model counts as one
/// when its metadata sets a pooling type (`bert.pooling_type`, `nomic-bert.pooling_type`, ...).
pub fn is_embedding_model(path: &Path) -> Result<bool> {
    has_pooling_type(&mut BufReader::new(File::open(path)?))
        .map_err(|err| err.context(format!("reading {}", path.display())))
}

fn has_pooling_type<R: Read + Seek>(reader: &mut BufReader<R>) -> Result<bool> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != b"GGUF" {
        bail!("not a GGUF file");
    }
    let version = read_u32(reader)?;
    if version < 2 {
        bail!("GGUF version {version} is not supported");
    }
    let _tensor_count = read_u64(reader)?;
    let kv_count = read_u64(reader)?;
    for _ in 0..kv_count {
        let key = read_string(reader)?;
        if key.ends_with(".pooling_type") {
            return Ok(true);
        }
        let kind = read_u32(reader)?;
        skip_value(reader, kind, 0)?;
    }
    Ok(false)
}

fn skip_value<R: Read + Seek>(reader: &mut BufReader<R>, kind: u32, depth: usize) -> Result<()> {
    match kind {
        STRING => {
            let len = read_u64(reader)?;
            skip(reader, len)?;
        }
        ARRAY => {
            if depth >= MAX_DEPTH {
                bail!("GGUF arrays are nested more than {MAX_DEPTH} deep");
            }
            let item_kind = read_u32(reader)?;
            let count = read_u64(reader)?;
            match fixed_size(item_kind) {
                Some(size) => {
                    let len = size
                        .checked_mul(count)
                        .ok_or_else(|| anyhow!("GGUF array of {count} items is too large"))?;
                    skip(reader, len)?;
                }
                None => {
                    for _ in 0..count {
                        skip_value(reader, item_kind, depth + 1)?;
                    }
                }
            }
        }
        _ => match fixed_size(kind) {
            Some(size) => skip(reader, size)?,
            None => bail!("unknown GGUF value type {kind}"),
        },
    }
    Ok(())
}

fn skip<R: Read + Seek>(reader: &mut BufReader<R>, len: u64) -> Result<()> {
    let len = i64::try_from(len).map_err(|_| anyhow!("GGUF value of {len} bytes is too large"))?;
    reader.seek_relative(len)?;
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    const U64: u32 = 10;

    fn header(kv_count: u64) -> Vec<u8> {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(kv_count.to_le_bytes());
        bytes
    }

    fn key(bytes: &mut Vec<u8>, name: &str, kind: u32) {
        bytes.extend((name.len() as u64).to_le_bytes());
        bytes.extend(name.as_bytes());
        bytes.extend(kind.to_le_bytes());
    }

    fn check(bytes: Vec<u8>) -> Result<bool> {
        has_pooling_type(&mut BufReader::new(Cursor::new(bytes)))
    }

    #[test]
    fn finds_a_pooling_type_after_skipped_values() {
        let mut bytes = header(2);
        key(&mut bytes, "general.name", STRING);
        bytes.extend(3u64.to_le_bytes());
        bytes.extend(b"abc");
        key(&mut bytes, "bert.pooling_type", 4);
        assert!(check(bytes).unwrap());
    }

    #[test]
    fn a_model_without_pooling_is_not_an_embedding_model() {
        let mut bytes = header(1);
        key(&mut bytes, "general.alignment", 4);
        bytes.extend(32u32.to_le_bytes());
        assert!(!check(bytes).unwrap());
    }

    #[test]
    fn an_array_size_that_overflows_is_an_error() {
        let mut bytes = header(1);
        key(&mut bytes, "tokenizer.ggml.scores", ARRAY);
        bytes.extend(U64.to_le_bytes());
        bytes.extend(u64::MAX.to_le_bytes());
        assert!(check(bytes).is_err());
    }

    #[test]
    fn a_string_longer_than_a_seek_is_an_error() {
        let mut bytes = header(1);
        key(&mut bytes, "general.name", STRING);
        bytes.extend(u64::MAX.to_le_bytes());
        assert!(check(bytes).is_err());
    }

    #[test]
    fn deeply_nested_arrays_are_an_error() {
        let mut bytes = header(1);
        key(&mut bytes, "x", ARRAY);
        for _ in 0..MAX_DEPTH {
            bytes.extend(ARRAY.to_le_bytes());
            bytes.extend(1u64.to_le_bytes());
        }
        bytes.extend(4u32.to_le_bytes());
        bytes.extend(1u64.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        let err = check(bytes).unwrap_err();
        assert!(err.to_string().contains("nested"), "{err:#}");
    }

    #[test]
    fn a_file_that_is_not_gguf_is_an_error() {
        assert!(check(b"GGML".to_vec()).is_err());
    }
}
