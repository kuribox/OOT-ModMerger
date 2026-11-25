//! The game's zstd dictionaries (`Pack/ZsDic.pack.zs` -> `bcett.byml.zsdic`, `zs.zsdic`, `pack.zsdic`)
//! and decompress / compress with them, in pure Rust (`structured-zstd`: the C crate needs a compiler
//! this toolchain does not have).

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use crate::sarc;

pub struct Dicts {
    map: HashMap<String, Vec<u8>>,
}

impl Dicts {
    pub fn load(romfs: &Path) -> Result<Dicts, String> {
        let raw = std::fs::read(romfs.join("Pack").join("ZsDic.pack.zs"))
            .map_err(|e| format!("ZsDic.pack.zs: {e}"))?;
        let data = decompress(&raw, None)?;
        let mut map = HashMap::new();
        for (name, bytes) in sarc::read(&data)? {
            let name = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
            map.insert(name, bytes);
        }
        Ok(Dicts { map })
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.map.get(name).map(|v| v.as_slice())
    }
}

/// The dictionaries a file is worth trying, in order: the one its kind implies, then the rest, then none.
pub fn candidates(kind: &str) -> &'static [Option<&'static str>] {
    match kind {
        "bcett" => &[Some("bcett.byml.zsdic"), Some("zs.zsdic"), None],
        "restbl" => &[None],
        "raw" => &[None],
        _ => &[Some("zs.zsdic"), Some("bcett.byml.zsdic"), Some("pack.zsdic"), None],
    }
}

/// zstd frame magic: a file that does not start with it is stored as it is (, some ).
pub fn is_zstd(raw: &[u8]) -> bool {
    raw.len() >= 4 && raw[0] == 0x28 && raw[1] == 0xB5 && raw[2] == 0x2F && raw[3] == 0xFD
}

pub fn decompress(compressed: &[u8], dictionary: Option<&[u8]>) -> Result<Vec<u8>, String> {
    let mut source = compressed;
    let mut decoder: Box<dyn Read + '_> = match dictionary {
        Some(d) => Box::new(
            structured_zstd::decoding::StreamingDecoder::new_with_dictionary_bytes(&mut source, d)
                .map_err(|e| format!("zstd: {e}"))?,
        ),
        None => Box::new(
            structured_zstd::decoding::StreamingDecoder::new(&mut source),
        ),
    };
    let mut out = Vec::new();
    decoder.read_to_end(&mut out).map_err(|e| format!("zstd: {e}"))?;
    Ok(out)
}

pub fn compress(data: &[u8], dictionary: Option<&[u8]>) -> Result<Vec<u8>, String> {
    use structured_zstd::encoding::{CompressionLevel, FrameCompressor};
    let mut compressor: FrameCompressor = FrameCompressor::new(CompressionLevel::from_level(3));
    if let Some(d) = dictionary {
        compressor
            .set_dictionary_from_bytes(d)
            .map_err(|e| format!("zstd dictionary: {e}"))?;
    }
    Ok(compressor.compress_independent_frame(data))
}
