//! BYML (v2-v7) reader and writer for the game's `.byml` / `.bcett.byml` files.
//!
//! The model keeps every node's exact BYML type, so a parse + write round trip is byte for byte the
//! same file: dict keys are written sorted by their UTF-8 bytes (what the game's own tool does), the
//! key and string tables are rebuilt in that order, and every container is laid out depth first.
//! That property is what makes it safe to edit a tree and write it back: the game reads it exactly
//! like its own files.

use std::collections::HashMap;

pub const T_HASH32: u8 = 0x20;
pub const T_HASH64: u8 = 0x21;
pub const T_STR: u8 = 0xA0;
pub const T_BIN: u8 = 0xA1;
pub const T_BINA: u8 = 0xA2;
pub const T_ARR: u8 = 0xC0;
pub const T_DICT: u8 = 0xC1;
pub const T_STRTAB: u8 = 0xC2;
pub const T_BOOL: u8 = 0xD0;
pub const T_INT: u8 = 0xD1;
pub const T_FLOAT: u8 = 0xD2;
pub const T_UINT: u8 = 0xD3;
pub const T_I64: u8 = 0xD4;
pub const T_U64: u8 = 0xD5;
pub const T_F64: u8 = 0xD6;
pub const T_NULL: u8 = 0xFF;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Str(String),
    Bin(Vec<u8>),
    BinA(Vec<u8>, u32),
    Bool(bool),
    Int(i32),
    UInt(u32),
    Float(f32),
    I64(i64),
    U64(u64),
    F64(f64),
    Null,
    Arr(Vec<Val>),
    /// A dict with string keys (the usual one).
    Dict(Vec<(String, Val)>),
    /// A dict keyed by a 32-bit hash.
    Hash32(Vec<(u32, Val)>),
    /// A dict keyed by a 64-bit hash.
    Hash64(Vec<(u64, Val)>),
}

impl Val {
    pub fn is_container(&self) -> bool {
        matches!(self, Val::Arr(_) | Val::Dict(_) | Val::Hash32(_) | Val::Hash64(_))
    }

    pub fn type_byte(&self) -> u8 {
        match self {
            Val::Str(_) => T_STR,
            Val::Bin(_) => T_BIN,
            Val::BinA(..) => T_BINA,
            Val::Bool(_) => T_BOOL,
            Val::Int(_) => T_INT,
            Val::UInt(_) => T_UINT,
            Val::Float(_) => T_FLOAT,
            Val::I64(_) => T_I64,
            Val::U64(_) => T_U64,
            Val::F64(_) => T_F64,
            Val::Null => T_NULL,
            Val::Arr(_) => T_ARR,
            Val::Dict(_) => T_DICT,
            Val::Hash32(_) => T_HASH32,
            Val::Hash64(_) => T_HASH64,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Dict(d) => d.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Val> {
        match self {
            Val::Dict(d) => d.iter_mut().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&Vec<Val>> {
        match self {
            Val::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_arr_mut(&mut self) -> Option<&mut Vec<Val>> {
        match self {
            Val::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Val::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Val::U64(v) => Some(*v),
            Val::UInt(v) => Some(u64::from(*v)),
            Val::Int(v) => Some(*v as u64),
            Val::I64(v) => Some(*v as u64),
            _ => None,
        }
    }

    /// The first of `keys` this dict carries with a scalar value, as an owned key for matching rows.
    pub fn row_key(&self, keys: &[&str]) -> Option<String> {
        for k in keys {
            if let Some(v) = self.get(k) {
                match v {
                    Val::U64(x) => return Some(format!("{k}={x}")),
                    Val::I64(x) => return Some(format!("{k}={x}")),
                    Val::UInt(x) => return Some(format!("{k}={x}")),
                    Val::Int(x) => return Some(format!("{k}={x}")),
                    Val::Str(s) => return Some(format!("{k}={s}")),
                    _ => {}
                }
            }
        }
        None
    }
}

pub struct Byml {
    pub version: u16,
    pub big: bool,
    pub root: Option<Val>,
}

struct Reader<'a> {
    data: &'a [u8],
    big: bool,
    keys: Vec<String>,
    strs: Vec<String>,
}

impl<'a> Reader<'a> {
    fn u16(&self, off: usize) -> u16 {
        let b = [self.data[off], self.data[off + 1]];
        if self.big { u16::from_be_bytes(b) } else { u16::from_le_bytes(b) }
    }

    fn u32(&self, off: usize) -> u32 {
        let b = [self.data[off], self.data[off + 1], self.data[off + 2], self.data[off + 3]];
        if self.big { u32::from_be_bytes(b) } else { u32::from_le_bytes(b) }
    }

    fn u64(&self, off: usize) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[off..off + 8]);
        if self.big { u64::from_be_bytes(b) } else { u64::from_le_bytes(b) }
    }

    fn u24(&self, off: usize) -> usize {
        let b = [self.data[off], self.data[off + 1], self.data[off + 2]];
        (if self.big { u32::from_be_bytes([0, b[0], b[1], b[2]]) } else { u32::from_le_bytes([b[0], b[1], b[2], 0]) })
            as usize
    }

    fn node_type(&self, off: usize) -> u8 {
        self.data[off]
    }

    fn strtab(&self, off: usize) -> Result<Vec<String>, String> {
        if self.node_type(off) != T_STRTAB {
            return Err(format!("not a string table at {off:#x}"));
        }
        let n = self.u24(off + 1);
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let start = off + self.u32(off + 4 + 4 * i) as usize;
            let end = self.data[start..].iter().position(|b| *b == 0).ok_or("unterminated string")? + start;
            out.push(String::from_utf8_lossy(&self.data[start..end]).into_owned());
        }
        Ok(out)
    }

    /// (key, type, slot offset) for every child of the container at `off`.
    fn children(&self, off: usize) -> Result<Vec<(Option<String>, u8, usize)>, String> {
        let t = self.node_type(off);
        let n = self.u24(off + 1);
        let mut out = Vec::with_capacity(n);
        match t {
            T_ARR => {
                let vbase = off + 4 + ((n + 3) & !3);
                for i in 0..n {
                    out.push((None, self.data[off + 4 + i], vbase + 4 * i));
                }
            }
            T_DICT => {
                for i in 0..n {
                    let p = off + 4 + 8 * i;
                    let key = self.keys.get(self.u24(p)).cloned().ok_or("dict key out of range")?;
                    out.push((Some(key), self.data[p + 3], p + 4));
                }
            }
            T_HASH32 | T_HASH64 => {
                let hs = if t == T_HASH32 { 4 } else { 8 };
                let tbase = off + 4 + (hs + 4) * n;
                for i in 0..n {
                    let p = off + 4 + (hs + 4) * i;
                    let key = if hs == 4 {
                        self.u32(p).to_string()
                    } else {
                        self.u64(p).to_string()
                    };
                    out.push((Some(format!("#{key}")), self.data[tbase + i], p + hs));
                }
            }
            _ => return Err(format!("unknown container 0x{t:02x} at {off:#x}")),
        }
        Ok(out)
    }

    fn scalar(&self, t: u8, slot: usize) -> Result<Val, String> {
        Ok(match t {
            T_STR => Val::Str(self.strs.get(self.u32(slot) as usize).cloned().ok_or("string out of range")?),
            T_BOOL => Val::Bool(self.u32(slot) != 0),
            T_INT => Val::Int(self.u32(slot) as i32),
            T_UINT => Val::UInt(self.u32(slot)),
            T_FLOAT => Val::Float(f32::from_bits(self.u32(slot))),
            T_I64 => Val::I64(self.u64(self.u32(slot) as usize) as i64),
            T_U64 => Val::U64(self.u64(self.u32(slot) as usize)),
            T_F64 => Val::F64(f64::from_bits(self.u64(self.u32(slot) as usize))),
            T_NULL => Val::Null,
            T_BIN => {
                let o = self.u32(slot) as usize;
                let n = self.u32(o) as usize;
                Val::Bin(self.data[o + 4..o + 4 + n].to_vec())
            }
            T_BINA => {
                let o = self.u32(slot) as usize;
                let n = self.u32(o) as usize;
                let align = self.u32(o + 4);
                Val::BinA(self.data[o + 8..o + 8 + n].to_vec(), align)
            }
            other => return Err(format!("unknown scalar 0x{other:02x}")),
        })
    }

    fn node(&self, off: usize) -> Result<Val, String> {
        let t = self.node_type(off);
        match t {
            T_ARR => {
                let mut out = Vec::new();
                for (_k, ct, slot) in self.children(off)? {
                    out.push(if is_container(ct) {
                        self.node(self.u32(slot) as usize)?
                    } else {
                        self.scalar(ct, slot)?
                    });
                }
                Ok(Val::Arr(out))
            }
            T_DICT => {
                let mut out = Vec::new();
                for (k, ct, slot) in self.children(off)? {
                    let key = k.ok_or("dict without a key")?;
                    let v = if is_container(ct) {
                        self.node(self.u32(slot) as usize)?
                    } else {
                        self.scalar(ct, slot)?
                    };
                    out.push((key, v));
                }
                Ok(Val::Dict(out))
            }
            T_HASH32 | T_HASH64 => {
                let mut out = Vec::new();
                for (k, ct, slot) in self.children(off)? {
                    let k = k.ok_or("hash dict without a key")?;
                    let v = if is_container(ct) {
                        self.node(self.u32(slot) as usize)?
                    } else {
                        self.scalar(ct, slot)?
                    };
                    out.push((k, v));
                }
                if t == T_HASH32 {
                    Ok(Val::Hash32(out.into_iter().map(|(k, v)| (k[1..].parse::<u32>().unwrap_or(0), v)).collect()))
                } else {
                    Ok(Val::Hash64(out.into_iter().map(|(k, v)| (k[1..].parse::<u64>().unwrap_or(0), v)).collect()))
                }
            }
            _ => Err(format!("root is not a container (0x{t:02x})")),
        }
    }
}

pub fn is_container(t: u8) -> bool {
    matches!(t, T_ARR | T_DICT | T_HASH32 | T_HASH64)
}

impl Byml {
    pub fn parse(data: &[u8]) -> Result<Byml, String> {
        if data.len() < 16 {
            return Err("too short".into());
        }
        let big = match &data[0..2] {
            b"YB" => false,
            b"BY" => true,
            other => return Err(format!("not BYML ({other:?})")),
        };
        let mut r = Reader { data, big, keys: Vec::new(), strs: Vec::new() };
        let version = r.u16(2);
        let hk = r.u32(4) as usize;
        let st = r.u32(8) as usize;
        let root = r.u32(12) as usize;
        if hk != 0 {
            r.keys = r.strtab(hk)?;
        }
        if st != 0 {
            r.strs = r.strtab(st)?;
        }
        let root = if root == 0 { None } else { Some(r.node(root)?) };
        Ok(Byml { version, big, root })
    }

    pub fn write(&self) -> Vec<u8> {
        write(self.root.as_ref(), self.version, self.big)
    }
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

fn csize(v: &Val) -> usize {
    match v {
        Val::Arr(a) => 4 + align4(a.len()) + 4 * a.len(),
        Val::Dict(d) => 4 + 8 * d.len(),
        Val::Hash32(d) => 4 + 8 * d.len() + align4(d.len()),
        Val::Hash64(d) => 4 + 12 * d.len() + align4(d.len()),
        _ => 0,
    }
}

fn collect_strings(v: &Val, keys: &mut std::collections::BTreeSet<String>, strs: &mut std::collections::BTreeSet<String>) {
    match v {
        Val::Str(s) => {
            strs.insert(s.clone());
        }
        Val::Arr(a) => a.iter().for_each(|x| collect_strings(x, keys, strs)),
        Val::Dict(d) => {
            for (k, x) in d {
                keys.insert(k.clone());
                collect_strings(x, keys, strs);
            }
        }
        Val::Hash32(d) => d.iter().for_each(|(_, x)| collect_strings(x, keys, strs)),
        Val::Hash64(d) => d.iter().for_each(|(_, x)| collect_strings(x, keys, strs)),
        _ => {}
    }
}

/// Children in written order: array by index, dicts by key bytes, hash dicts by their number.
fn ordered_children(v: &Val) -> Option<Vec<(KeyRef, &Val)>> {
    match v {
        Val::Arr(a) => Some(a.iter().map(|x| (KeyRef::Index, x)).collect()),
        Val::Dict(d) => {
            let mut items: Vec<(&String, &Val)> = d.iter().map(|(k, x)| (k, x)).collect();
            items.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            Some(items.into_iter().map(|(k, x)| (KeyRef::Str(k.clone()), x)).collect())
        }
        Val::Hash32(d) => {
            let mut items: Vec<(&u32, &Val)> = d.iter().map(|(k, x)| (k, x)).collect();
            items.sort_by_key(|(k, _)| **k);
            Some(items.into_iter().map(|(k, x)| (KeyRef::H32(*k), x)).collect())
        }
        Val::Hash64(d) => {
            let mut items: Vec<(&u64, &Val)> = d.iter().map(|(k, x)| (k, x)).collect();
            items.sort_by_key(|(k, _)| **k);
            Some(items.into_iter().map(|(k, x)| (KeyRef::H64(*k), x)).collect())
        }
        _ => None,
    }
}

#[derive(Clone)]
enum KeyRef {
    Index,
    Str(String),
    H32(u32),
    H64(u64),
}

fn strtab_bytes(strings: &[String], big: bool) -> Vec<u8> {
    let n = strings.len();
    let hdr = 4 + 4 * (n + 1);
    let mut body = Vec::new();
    let mut offs = Vec::with_capacity(n + 1);
    for s in strings {
        offs.push(hdr + body.len());
        body.extend_from_slice(s.as_bytes());
        body.push(0);
    }
    offs.push(hdr + body.len());
    let mut out = vec![T_STRTAB];
    out.extend_from_slice(&u24(n as u32, big));
    for o in offs {
        out.extend_from_slice(&u32b(o as u32, big));
    }
    out.extend_from_slice(&body);
    while out.len() % 4 != 0 {
        out.push(0);
    }
    out
}

fn u32b(v: u32, big: bool) -> [u8; 4] {
    if big { v.to_be_bytes() } else { v.to_le_bytes() }
}

fn u64b(v: u64, big: bool) -> [u8; 8] {
    if big { v.to_be_bytes() } else { v.to_le_bytes() }
}

fn u24(v: u32, big: bool) -> [u8; 3] {
    let b = u32b(v, big);
    if big { [b[1], b[2], b[3]] } else { [b[0], b[1], b[2]] }
}

pub fn write(root: Option<&Val>, version: u16, big: bool) -> Vec<u8> {
    let mut keys = std::collections::BTreeSet::new();
    let mut strs = std::collections::BTreeSet::new();
    if let Some(r) = root {
        collect_strings(r, &mut keys, &mut strs);
    }
    let keys: Vec<String> = keys.into_iter().collect();
    let strs: Vec<String> = strs.into_iter().collect();
    let kidx: HashMap<&String, usize> = keys.iter().enumerate().map(|(i, k)| (k, i)).collect();
    let sidx: HashMap<&String, usize> = strs.iter().enumerate().map(|(i, s)| (s, i)).collect();

    let mut buf = vec![0u8; 16];
    let mut hk = 0usize;
    let mut st = 0usize;
    if !keys.is_empty() {
        hk = buf.len();
        buf.extend_from_slice(&strtab_bytes(&keys, big));
    }
    if !strs.is_empty() {
        st = buf.len();
        buf.extend_from_slice(&strtab_bytes(&strs, big));
    }

    // Layout: containers depth first (pre-order), then the wide/binary values they point at.
    let mut order: Vec<&Val> = Vec::new();
    let mut offs: HashMap<usize, usize> = HashMap::new();
    let mut pos = buf.len();
    fn lay<'a>(v: &'a Val, pos: &mut usize, order: &mut Vec<&'a Val>, offs: &mut HashMap<usize, usize>) {
        offs.insert(v as *const Val as usize, *pos);
        order.push(v);
        *pos += csize(v);
        if let Some(children) = ordered_children(v) {
            for (_k, c) in children {
                if c.is_container() {
                    lay(c, pos, order, offs);
                }
            }
        }
    }
    if let Some(r) = root {
        lay(r, &mut pos, &mut order, &mut offs);
    }
    let mut extras: HashMap<usize, usize> = HashMap::new();
    fn lay_extra(v: &Val, pos: &mut usize, extras: &mut HashMap<usize, usize>) {
        if let Some(children) = ordered_children(v) {
            for (_k, c) in children {
                if c.is_container() {
                    lay_extra(c, pos, extras);
                    continue;
                }
                match c {
                    Val::I64(_) | Val::U64(_) | Val::F64(_) => {
                        *pos = (*pos + 7) & !7;
                        extras.insert(c as *const Val as usize, *pos);
                        *pos += 8;
                    }
                    Val::Bin(b) => {
                        *pos = (*pos + 3) & !3;
                        extras.insert(c as *const Val as usize, *pos);
                        *pos += 4 + b.len();
                    }
                    Val::BinA(b, _) => {
                        *pos = (*pos + 3) & !3;
                        extras.insert(c as *const Val as usize, *pos);
                        *pos += 8 + b.len();
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(r) = root {
        lay_extra(r, &mut pos, &mut extras);
    }
    let mut out = buf;
    out.resize(align4(pos), 0);

    for node in &order {
        let o = offs[&(*node as *const Val as usize)];
        let children = ordered_children(node).unwrap_or_default();
        let n = children.len();
        out[o] = node.type_byte();
        out[o + 1..o + 4].copy_from_slice(&u24(n as u32, big));
        let mut slots: Vec<(u8, u32)> = Vec::with_capacity(n);
        for (_k, c) in &children {
            let (t, x) = match c {
                Val::Str(s) => (T_STR, sidx[s] as u32),
                Val::Bool(b) => (T_BOOL, u32::from(*b)),
                Val::Int(i) => (T_INT, *i as u32),
                Val::UInt(u) => (T_UINT, *u),
                Val::Float(f) => (T_FLOAT, f.to_bits()),
                Val::Null => (T_NULL, 0),
                Val::I64(_) | Val::U64(_) | Val::F64(_) | Val::Bin(_) | Val::BinA(..) => {
                    (c.type_byte(), extras[&(*c as *const Val as usize)] as u32)
                }
                other => (other.type_byte(), offs[&(*c as *const Val as usize)] as u32),
            };
            slots.push((t, x));
        }
        // Fill the wide values and the binaries the slots point at.
        for ((_k, c), (_t, x)) in children.iter().zip(slots.iter()) {
            let at = *x as usize;
            match c {
                Val::I64(v) => out[at..at + 8].copy_from_slice(&u64b(*v as u64, big)),
                Val::U64(v) => out[at..at + 8].copy_from_slice(&u64b(*v, big)),
                Val::F64(v) => out[at..at + 8].copy_from_slice(&u64b(v.to_bits(), big)),
                Val::Bin(b) => {
                    out[at..at + 4].copy_from_slice(&u32b(b.len() as u32, big));
                    out[at + 4..at + 4 + b.len()].copy_from_slice(b);
                }
                Val::BinA(b, al) => {
                    out[at..at + 4].copy_from_slice(&u32b(b.len() as u32, big));
                    out[at + 4..at + 8].copy_from_slice(&u32b(*al, big));
                    out[at + 8..at + 8 + b.len()].copy_from_slice(b);
                }
                _ => {}
            }
        }
        match node {
            Val::Arr(_) => {
                let vb = o + 4 + align4(n);
                for (i, (t, x)) in slots.iter().enumerate() {
                    out[o + 4 + i] = *t;
                    out[vb + 4 * i..vb + 4 * i + 4].copy_from_slice(&u32b(*x, big));
                }
            }
            Val::Dict(_) => {
                for (i, ((k, _v), (t, x))) in children.iter().zip(slots.iter()).enumerate() {
                    let KeyRef::Str(name) = k else { continue };
                    let p = o + 4 + 8 * i;
                    out[p..p + 3].copy_from_slice(&u24(kidx[name] as u32, big));
                    out[p + 3] = *t;
                    out[p + 4..p + 8].copy_from_slice(&u32b(*x, big));
                }
            }
            Val::Hash32(_) => {
                let tb = o + 4 + 8 * n;
                for (i, ((k, _v), (t, x))) in children.iter().zip(slots.iter()).enumerate() {
                    let KeyRef::H32(key) = k else { continue };
                    let p = o + 4 + 8 * i;
                    out[p..p + 4].copy_from_slice(&u32b(*key, big));
                    out[p + 4..p + 8].copy_from_slice(&u32b(*x, big));
                    out[tb + i] = *t;
                }
            }
            Val::Hash64(_) => {
                let tb = o + 4 + 12 * n;
                for (i, ((k, _v), (t, x))) in children.iter().zip(slots.iter()).enumerate() {
                    let KeyRef::H64(key) = k else { continue };
                    let p = o + 4 + 12 * i;
                    out[p..p + 8].copy_from_slice(&u64b(*key, big));
                    out[p + 8..p + 12].copy_from_slice(&u32b(*x, big));
                    out[tb + i] = *t;
                }
            }
            _ => {}
        }
    }

    out[0..2].copy_from_slice(if big { b"BY" } else { b"YB" });
    out[2..4].copy_from_slice(&if big { version.to_be_bytes() } else { version.to_le_bytes() });
    let root_off = root.map(|r| offs[&(r as *const Val as usize)] as u32).unwrap_or(0);
    out[4..8].copy_from_slice(&u32b(hk as u32, big));
    out[8..12].copy_from_slice(&u32b(st as u32, big));
    out[12..16].copy_from_slice(&u32b(root_off, big));
    out
}

/// Diagnostic: (containers, bytes their headers and slots take, bytes the wide/binary values take).
pub fn stats(v: &Val) -> (usize, usize, usize) {
    fn walk(v: &Val, c: &mut usize, s: &mut usize, e: &mut usize) {
        if !v.is_container() {
            return;
        }
        *c += 1;
        *s += csize(v);
        if let Some(children) = ordered_children(v) {
            for (_k, child) in children {
                match child {
                    Val::I64(_) | Val::U64(_) | Val::F64(_) => *e += 16,
                    Val::Bin(b) => *e += 8 + b.len(),
                    Val::BinA(b, _) => *e += 12 + b.len(),
                    other if other.is_container() => walk(other, c, s, e),
                    _ => {}
                }
            }
        }
    }
    let (mut c, mut s, mut e) = (0, 0, 0);
    walk(v, &mut c, &mut s, &mut e);
    (c, s, e)
}
