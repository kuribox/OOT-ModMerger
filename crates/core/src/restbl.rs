//! The resource size table (`ResourceSizeTable.*.rsizetable`): crc32(path) -> bytes the game may use
//! for that resource. Two mods' tables are merged entry by entry, highest value wins, and the files a
//! merge grows get their entry raised.

use std::collections::BTreeMap;

pub struct Restbl {
    pub version: u32,
    pub name_size: u32,
    pub by_crc: BTreeMap<u32, u32>,
    pub by_name: BTreeMap<String, u32>,
}

impl Restbl {
    pub fn parse(data: &[u8]) -> Result<Restbl, String> {
        if data.len() < 0x16 || &data[0..6] != b"RESTBL" {
            return Err("not a RESTBL".into());
        }
        let u = |at: usize| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        let version = u(6);
        let name_size = u(10) as usize;
        let crc_count = u(14) as usize;
        let name_count = u(18) as usize;
        let mut p = 0x16;
        let mut by_crc = BTreeMap::new();
        for _ in 0..crc_count {
            by_crc.insert(u(p), u(p + 4));
            p += 8;
        }
        let mut by_name = BTreeMap::new();
        for _ in 0..name_count {
            let raw = &data[p..p + name_size];
            let len = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
            let name = String::from_utf8_lossy(&raw[..len]).into_owned();
            by_name.insert(name, u(p + name_size));
            p += name_size + 4;
        }
        Ok(Restbl { version, name_size: name_size as u32, by_crc, by_name })
    }

    pub fn dump(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"RESTBL");
        out.extend_from_slice(&self.version.to_le_bytes());
        out.extend_from_slice(&self.name_size.to_le_bytes());
        out.extend_from_slice(&(self.by_crc.len() as u32).to_le_bytes());
        out.extend_from_slice(&(self.by_name.len() as u32).to_le_bytes());
        for (crc, size) in &self.by_crc {
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&size.to_le_bytes());
        }
        for (name, size) in &self.by_name {
            let mut raw = name.as_bytes().to_vec();
            raw.resize(self.name_size as usize, 0);
            out.extend_from_slice(&raw);
            out.extend_from_slice(&size.to_le_bytes());
        }
        out
    }

    pub fn merge(&mut self, other: &Restbl) {
        for (k, v) in &other.by_crc {
            let e = self.by_crc.entry(*k).or_insert(0);
            *e = (*e).max(*v);
        }
        for (k, v) in &other.by_name {
            let e = self.by_name.entry(k.clone()).or_insert(0);
            *e = (*e).max(*v);
        }
    }

    pub fn get(&self, path: &str) -> u32 {
        if let Some(v) = self.by_name.get(path) {
            return *v;
        }
        let crc = crc32(path.as_bytes());
        self.by_crc.get(&crc).copied().unwrap_or(0)
    }

    /// Raise the entry of a resource so a file this merge grew still fits (0x2000 of slack, as the
    /// app's own enemy-pack step does).
    pub fn raise(&mut self, path: &str, size: usize) -> bool {
        let need = ((size as u32) + 0x2000 + 7) & !7;
        if let Some(v) = self.by_name.get_mut(path) {
            if *v < need {
                *v = need;
                return true;
            }
            return false;
        }
        let crc = crc32(path.as_bytes());
        if let Some(v) = self.by_crc.get_mut(&crc) {
            if *v < need {
                *v = need;
                return true;
            }
            return false;
        }
        self.by_crc.insert(crc, need);
        true
    }
}

/// CRC-32 (IEEE), the key the table uses.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c ^= u32::from(*b);
        for _ in 0..8 {
            let mask = (c & 1).wrapping_neg();
            c = (c >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !c
}
