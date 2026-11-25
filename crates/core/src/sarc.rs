//! SARC archive reader (the game's `ZsDic.pack.zs` is one): enough to pull the zstd dictionaries out.

pub fn read(data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    if data.len() < 0x20 || &data[0..4] != b"SARC" {
        return Err("not a SARC".into());
    }
    let header_len = u16::from_le_bytes([data[4], data[5]]) as usize;
    let data_off = u32::from_le_bytes([data[0xC], data[0xD], data[0xE], data[0xF]]) as usize;
    let mut p = header_len;
    if &data[p..p + 4] != b"SFAT" {
        return Err("no SFAT".into());
    }
    let nodes = u16::from_le_bytes([data[p + 6], data[p + 7]]) as usize;
    p += 12;
    let mut entries = Vec::with_capacity(nodes);
    for i in 0..nodes {
        let at = p + 16 * i;
        let attr = u32::from_le_bytes([data[at + 4], data[at + 5], data[at + 6], data[at + 7]]);
        let start = u32::from_le_bytes([data[at + 8], data[at + 9], data[at + 10], data[at + 11]]) as usize;
        let end = u32::from_le_bytes([data[at + 12], data[at + 13], data[at + 14], data[at + 15]]) as usize;
        entries.push(((attr & 0xFFFFFF) as usize * 4, start, end));
    }
    p += 16 * nodes;
    if &data[p..p + 4] != b"SFNT" {
        return Err("no SFNT".into());
    }
    let names = p + 8;
    let mut out = Vec::with_capacity(nodes);
    for (name_off, start, end) in entries {
        let at = names + name_off;
        let len = data[at..].iter().position(|b| *b == 0).ok_or("unterminated name")?;
        let name = String::from_utf8_lossy(&data[at..at + len]).into_owned();
        out.push((name, data[data_off + start..data_off + end].to_vec()));
    }
    Ok(out)
}


/// 32-bit name hash a SARC's file table uses (the same one the game's archives carry).
pub fn hash(name: &str) -> u32 {
    let mut h: u32 = 0;
    for b in name.bytes() {
        let c = b as i8 as i32 as u32;
        h = h.wrapping_mul(0x65).wrapping_add(c);
    }
    h
}

/// Rebuild a SARC from (name, bytes) pairs. The nodes are sorted by name hash and the data is 8-byte
/// aligned, which is what the game's own archives look like.
pub fn write(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut files: Vec<&(String, Vec<u8>)> = files.iter().collect();
    files.sort_by_key(|(n, _)| hash(n));
    let mut sfnt = Vec::new();
    let mut name_offs = Vec::new();
    for (n, _) in &files {
        name_offs.push(sfnt.len());
        sfnt.extend_from_slice(n.as_bytes());
        sfnt.push(0);
        while sfnt.len() % 4 != 0 {
            sfnt.push(0);
        }
    }
    let hdr_len = 0x14 + 0xC + 16 * files.len() + 8 + sfnt.len();
    let data_off = (hdr_len + 7) & !7;
    let mut data = Vec::new();
    let mut nodes = Vec::new();
    for (entry, no) in files.iter().zip(name_offs.iter()) {
        let (name, bytes) = *entry;
        while data.len() % 8 != 0 {
            data.push(0);
        }
        let start = data.len();
        data.extend_from_slice(bytes);
        nodes.extend_from_slice(&hash(name).to_le_bytes());
        nodes.extend_from_slice(&(0x0100_0000u32 | (*no as u32 / 4)).to_le_bytes());
        nodes.extend_from_slice(&(start as u32).to_le_bytes());
        nodes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"SARC");
    out.extend_from_slice(&0x14u16.to_le_bytes());
    out.extend_from_slice(&0xFEFFu16.to_le_bytes());
    out.extend_from_slice(&((data_off + data.len()) as u32).to_le_bytes());
    out.extend_from_slice(&(data_off as u32).to_le_bytes());
    out.extend_from_slice(&0x0100u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(b"SFAT");
    out.extend_from_slice(&0x000Cu16.to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&0x65u32.to_le_bytes());
    out.extend_from_slice(&nodes);
    out.extend_from_slice(b"SFNT");
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&sfnt);
    out.resize(data_off, 0);
    out.extend_from_slice(&data);
    out
}
