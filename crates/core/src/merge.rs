//! The merge itself: a fixed rule per file format, an overlay written where conflicts were, and a
//! report of everything that was merged and everything that had to stay from one mod.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::byml::{self, Val};
use crate::restbl::Restbl;
use crate::sarc;
use crate::zs::{self, Dicts};

/// A mod folder and the name to show for it.
pub struct Mod {
    pub name: String,
    pub romfs: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Row {
    /// "merged" or "kept".
    pub action: &'static str,
    pub file: String,
    pub detail: String,
}

#[derive(Default)]
pub struct Outcome {
    pub rows: Vec<Row>,
    /// Files that are in conflict but cannot be merged: the last mod listed wins.
    pub warnings: Vec<String>,
    /// A name for the merged folder that sorts after every mod (when the requested one does not).
    pub advice: Option<String>,
    pub merged: usize,
    pub kept: usize,
    pub files: usize,
    pub overlaps: usize,
    pub written: usize,
    /// For every file in conflict, the mods (their index, lowest priority first) that ship it.
    pub owners: BTreeMap<String, Vec<usize>>,
}

struct Stats {
    values: usize,
    rows_added: usize,
    whole_rows: usize,
    unkeyed: Vec<String>,
    notes: Vec<String>,
}

const ROW_KEYS: &[&str] = &["Hash", "GroupGUID", "PlgGuid", "Name", "Key", "ID", "__RowId"];
const REFUSE_SUFFIXES: &[&str] = &[
    ".bfevfl.zs", ".ainb", ".bntx.zs", ".blarc.zs", ".blarc", ".asb.zs",
    ".baev.zs", ".bfres.zs", ".bffnt.zs", ".bflim.zs", ".bfstm.zs", ".bars.zs", ".pack",
];
// RSDB tables (rstbl) are plain arrays of rows keyed by __RowId / Hash: those are merged. The
// GameData list has its own rule (see merge_gamedata): rows are joined and the save layout in
// MetaData is grown by 8 bytes a saved variable, which is what every mod builder does.
const REFUSE_NAMES: &[&str] = &["GameDataList"];

pub fn kind_of(rel: &str) -> &'static str {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    if rel.ends_with(".bcett.byml.zs") {
        "bcett"
    } else if rel.ends_with(".sarc.zs") {
        "sarc"
    } else if rel.ends_with(".pack.zs") {
        "pack"
    } else if name.contains("ResourceSizeTable") {
        "restbl"
    } else if rel.ends_with(".zs") {
        "zs"
    } else {
        "raw"
    }
}

fn refused(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    REFUSE_NAMES.iter().any(|n| name.contains(n)) || REFUSE_SUFFIXES.iter().any(|s| rel.ends_with(s))
}

/// Every file, with the mods (lowest priority first) that ship it.
pub fn scan(mods: &[Mod]) -> Result<BTreeMap<String, Vec<usize>>, String> {
    let mut by_rel: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, m) in mods.iter().enumerate() {
        walk(&m.romfs, &m.romfs, &mut |rel| {
            by_rel.entry(rel).or_default().push(i);
        })?;
    }
    Ok(by_rel)
}

fn walk(root: &Path, dir: &Path, f: &mut impl FnMut(String)) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, f)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            f(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

/// Merge `low` into `high`: dictionaries are unioned, keyed rows are matched by their key, a scalar
/// both sides set comes from `high`. With `whole_rows` a matched row is taken from `high` untouched
/// instead of field by field (an actor's placement must not be a mix of two mods).
fn merge_nodes(low: &Val, high: &Val, whole_rows: bool, st: &mut Stats) -> Val {
    match (low, high) {
        (Val::Dict(l), Val::Dict(h)) => {
            let mut out = l.clone();
            for (k, v) in h {
                match out.iter_mut().find(|(ok, _)| ok == k) {
                    Some((_, slot)) => *slot = merge_nodes(slot, v, whole_rows, st),
                    None => out.push((k.clone(), v.clone())),
                }
            }
            Val::Dict(out)
        }
        (Val::Arr(l), Val::Arr(h)) => {
            let kl: Option<Vec<String>> = l.iter().map(|x| x.row_key(ROW_KEYS)).collect();
            let kh: Option<Vec<String>> = h.iter().map(|x| x.row_key(ROW_KEYS)).collect();
            match (kl, kh) {
                (Some(kl), Some(kh)) if l.len() == kl.len() && h.len() == kh.len() => {
                    // A key may appear in both lists (that is a row both mods have); it must not
                    // appear twice in the same one, or the rows cannot be matched.
                    let unique = |keys: &[String]| {
                        let mut seen = std::collections::HashSet::new();
                        keys.iter().all(|k| seen.insert(k.clone()))
                    };
                    if !unique(&kl) || !unique(&kh) {
                        st.notes.push(format!("{} rows with a repeated key: kept from the last mod", h.len()));
                        return Val::Arr(h.clone());
                    }
                    let mut out = l.clone();
                    for (k, node) in kh.iter().zip(h.iter()) {
                        match kl.iter().position(|x| x == k) {
                            Some(i) => {
                                if whole_rows {
                                    if out[i] != *node {
                                        st.whole_rows += 1;
                                    }
                                    out[i] = node.clone();
                                } else {
                                    out[i] = merge_nodes(&out[i], node, whole_rows, st);
                                }
                            }
                            None => {
                                st.rows_added += 1;
                                out.push(node.clone());
                            }
                        }
                    }
                    Val::Arr(out)
                }
                _ if l.len() == h.len() => {
                    // No key on the rows but the same length: two copies of one list, merged in place.
                    st.unkeyed.push(format!("{} rows without a key, merged by position", h.len()));
                    Val::Arr(l.iter().zip(h.iter()).map(|(a, b)| merge_nodes(a, b, whole_rows, st)).collect())
                }
                _ => {
                    // Lists of plain values (a scene's simultaneous / far-delete group ids): two
                    // mods that add entries are unioned, nothing is replaced.
                    let scalarish = |a: &Vec<Val>| {
                        a.iter().all(|x| !matches!(x, Val::Dict(_) | Val::Hash32(_) | Val::Hash64(_)))
                    };
                    if scalarish(l) && scalarish(h) && !l.is_empty() && !h.is_empty() {
                        let mut out = l.clone();
                        let mut added = 0;
                        for item in h {
                            if !out.contains(item) {
                                out.push(item.clone());
                                added += 1;
                            }
                        }
                        st.rows_added += added;
                        st.notes.push(format!("{} + {} values unioned", l.len(), added));
                        return Val::Arr(out);
                    }
                    st.notes.push(format!("{} vs {} rows without a key: kept from the last mod", l.len(), h.len()));
                    Val::Arr(h.clone())
                }
            }
        }
        (Val::Hash32(l), Val::Hash32(h)) => Val::Hash32(merge_map(l, h, whole_rows, st)),
        (Val::Hash64(l), Val::Hash64(h)) => Val::Hash64(merge_map(l, h, whole_rows, st)),
        (a, b) if a == b => a.clone(),
        _ => {
            st.values += 1;
            high.clone()
        }
    }
}

fn merge_map<K: Ord + Copy + std::fmt::Debug>(low: &[(K, Val)], high: &[(K, Val)], whole_rows: bool, st: &mut Stats) -> Vec<(K, Val)> {
    let mut out = low.to_vec();
    for (k, v) in high {
        match out.iter_mut().find(|(ok, _)| ok == k) {
            Some((_, slot)) => {
                if whole_rows {
                    if slot != v {
                        st.whole_rows += 1;
                    }
                    *slot = v.clone();
                } else {
                    *slot = merge_nodes(slot, v, whole_rows, st);
                }
            }
            None => {
                st.rows_added += 1;
                out.push((*k, v.clone()));
            }
        }
    }
    out
}

/// Merge one file into the higher-priority mod's copy; `None` means "take that copy as it is".
fn merge_byml(low: &Val, high: &Val, whole_rows: bool) -> (Val, Stats) {
    let mut st = Stats { values: 0, rows_added: 0, whole_rows: 0, unkeyed: Vec::new(), notes: Vec::new() };
    let out = merge_nodes(low, high, whole_rows, &mut st);
    (out, st)
}

/// Decompress with the first dictionary that works, returning the bytes and which one it was.
type Unpacked = (Vec<u8>, Option<String>, bool);

fn unpack(dicts: &Dicts, path: &Path, kind: &str) -> Result<Unpacked, String> {
    let raw = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !zs::is_zstd(&raw) {
        return Ok((raw, None, false));
    }
    let mut last = String::new();
    for cand in zs::candidates(kind) {
        let dict = cand.and_then(|n| dicts.get(n));
        match zs::decompress(&raw, dict) {
            Ok(data) => return Ok((data, cand.map(str::to_string), true)),
            Err(e) => last = e,
        }
    }
    Err(format!("{}: {last}", path.display()))
}

fn pack(data: &[u8], dicts: &Dicts, used: &Option<String>) -> Result<Vec<u8>, String> {
    zs::compress(data, used.as_deref().and_then(|n| dicts.get(n)))
}

/// Write it back the way it came in: compressed files get compressed again (with the dictionary they
/// used), a file the mods ship uncompressed (a plain .byml) is written as it is.
fn pack_like(data: &[u8], dicts: &Dicts, used: &Option<String>, was_compressed: bool) -> Result<Vec<u8>, String> {
    if was_compressed { pack(data, dicts, used) } else { Ok(data.to_vec()) }
}


/// Three-way merge of a fixed-layout binary: every run of bytes a mod changed against the game's own
/// file is applied, and where two mods changed the same run the higher-priority one wins (counted).
fn three_way(vanilla: &[u8], sides: &[&[u8]]) -> (Vec<u8>, usize, usize) {
    let mut out = vanilla.to_vec();
    let mut owner = vec![0u8; vanilla.len()];
    let mut runs = 0;
    let mut conflicts = 0;
    for (i, side) in sides.iter().enumerate() {
        let tag = (i + 1) as u8;
        let mut at = 0;
        while at < vanilla.len() {
            if side[at] == vanilla[at] {
                at += 1;
                continue;
            }
            let start = at;
            while at < vanilla.len() && side[at] != vanilla[at] {
                at += 1;
            }
            let end = at;
            let foreign = owner[start..end].iter().any(|o| *o != 0 && *o != tag);
            if foreign {
                conflicts += 1;
            } else {
                runs += 1;
            }
            out[start..end].copy_from_slice(&side[start..end]);
            for o in &mut owner[start..end] {
                *o = tag;
            }
        }
    }
    (out, runs, conflicts)
}


/// What a mod adds to a byml table against the game's own file: new rows in every keyed array and
/// new keys, a couple of levels down (the GameData list nests its row arrays under `Data`).
fn byml_delta(vanilla: &Val, edited: &Val) -> String {
    fn walk(v: &Val, e: &Val, depth: usize, parts: &mut Vec<String>) {
        if depth > 3 {
            return;
        }
        if let (Val::Dict(v), Val::Dict(e)) = (v, e) {
            for (k, ev) in e {
                let before = v.iter().find(|(vk, _)| vk == k).map(|(_, val)| val);
                match (before, ev) {
                    (Some(Val::Arr(old)), Val::Arr(new)) => {
                        let known: std::collections::HashSet<String> =
                            old.iter().filter_map(|r| r.row_key(ROW_KEYS)).collect();
                        let added = new
                            .iter()
                            .filter(|r| r.row_key(ROW_KEYS).is_some_and(|k| !known.contains(&k)))
                            .count();
                        if added > 0 {
                            parts.push(format!("+{added} {k}"));
                        }
                    }
                    (Some(before), _) => walk(before, ev, depth + 1, parts),
                    (None, _) => parts.push(format!("+{k}")),
                }
            }
        }
    }
    let mut parts = Vec::new();
    walk(vanilla, edited, 0, &mut parts);
    if parts.is_empty() { "no rows added".to_string() } else { parts.join(", ") }
}


/// The GameData list is not a normal table: its `MetaData` carries the save layout, and each saved
/// variable costs 8 bytes in it. Every mod's own builder appends its rows at the end of each type
/// array and grows the layout by 8 bytes a saved row (that is exactly what HylianSurvivors and
/// Biggoron did, measured), so the union is the vanilla rows, then the added ones in mod order, and
/// the same arithmetic applied again.
/// A row's fields, so an edited row can be merged field by field.
fn row_fields(v: &Val) -> Vec<(String, Val)> {
    match v {
        Val::Dict(d) => d.clone(),
        _ => Vec::new(),
    }
}

fn is_gamedata(rel: &str) -> bool {
    rel.starts_with("GameData/") && rel.contains("GameDataList")
}

fn merge_gamedata(
    romfs: &Path,
    dicts: &Dicts,
    mods: &[Mod],
    owners: &[usize],
    rel: &str,
) -> Result<(Vec<u8>, usize, String), String> {
    let (vdata, used, zipped) = unpack(dicts, &romfs.join(rel), "zs")?;
    let vtree = byml::Byml::parse(&vdata)?;
    let mut tree = vtree.root.clone().ok_or("empty GameData list")?;
    let Some(Val::Dict(vanilla_data)) = tree.get("Data").cloned() else {
        return Err("the GameData list has no Data table".into());
    };

    // What the vanilla list knows, so only genuinely new rows are taken from the mods.
    let mut out: Vec<(String, Val)> = vanilla_data.clone();
    let mut added: Vec<(String, usize)> = Vec::new(); // (type, count) for the report
    let mut per_slot = [0i64; 4];
    let mut total = 0i64;
    let mut edited = 0usize;

    for m in owners {
        let (d, _, _) = unpack(dicts, &mods[*m].romfs.join(rel), "zs")?;
        let t = byml::Byml::parse(&d)?;
        let Some(root) = t.root.as_ref() else { continue };
        let Some(Val::Dict(their)) = root.get("Data") else { continue };
        for (kind, theirs) in their {
            let Val::Arr(rows) = theirs else { continue };
            let Some((_, Val::Arr(mine))) = out.iter_mut().find(|(k, _)| k == kind) else { continue };
            for row in rows {
                let Some(k) = row.row_key(ROW_KEYS) else { continue };
                if let Some(at) = mine.iter().position(|r| r.row_key(ROW_KEYS).as_deref() == Some(k.as_str())) {
                    // A row the vanilla list already has: the mod may have changed its default or
                    // its reset value, and the higher-priority mod wins each field it touched.
                    if let Val::Dict(fields) = &mut mine[at] {
                        for (field, value) in row_fields(row) {
                            match fields.iter_mut().find(|(f, _)| f == &field) {
                                Some((_, current)) => {
                                    if current != &value {
                                        *current = value;
                                        edited += 1;
                                    }
                                }
                                None => fields.push((field, value)),
                            }
                        }
                    }
                    continue;
                }
                if let Some(Val::Int(slot)) = row.get("SaveFileIndex") {
                    if (0..4).contains(slot) {
                        per_slot[*slot as usize] += 1;
                        total += 1;
                    }
                }
                mine.push(row.clone());
                match added.iter_mut().find(|(t, _)| t == kind) {
                    Some((_, n)) => *n += 1,
                    None => added.push((kind.clone(), 1)),
                }
            }
        }
    }

    if total > 0 {
        let step = 8 * total;
        if let Some(meta) = tree.get_mut("MetaData").and_then(|m| match m {
            Val::Dict(d) => Some(d),
            _ => None,
        }) {
            for key in ["AllDataSaveOffset", "AllDataSaveSize"] {
                if let Some((_, Val::Int(v))) = meta.iter_mut().find(|(k, _)| k == key) {
                    *v += step as i32;
                }
            }
            for key in ["SaveDataOffsetPos", "SaveDataSize"] {
                if let Some((_, Val::Arr(a))) = meta.iter_mut().find(|(k, _)| k == key) {
                    for (i, slot) in a.iter_mut().enumerate().take(4) {
                        if let Val::Int(v) = slot {
                            *v += (8 * per_slot[i]) as i32;
                        }
                    }
                }
            }
        }
    }
    tree.get_mut("Data").map(|d| *d = Val::Dict(out));

    let bytes = byml::Byml { version: vtree.version, big: vtree.big, root: Some(tree) }.write();
    let packed = pack_like(&bytes, dicts, &used, zipped)?;
    let count = added.iter().map(|(_, n)| *n).sum::<usize>();
    let types = added.iter().map(|(t, n)| format!("+{n} {t}")).collect::<Vec<_>>().join(", ");
    let grown: Vec<String> = (0..4)
        .filter(|i| per_slot[*i] > 0)
        .map(|i| format!("save slot {i} +{} B", 8 * per_slot[i]))
        .collect();
    let detail = if count == 0 && edited == 0 {
        "the mods change nothing against the game's list".to_string()
    } else {
        format!(
            "{count} rows kept from every mod ({types}){}{}",
            if edited > 0 { format!(", {edited} values retuned") } else { String::new() },
            if grown.is_empty() { String::new() } else { format!("; {}", grown.join(", ")) }
        )
    };
    Ok((packed, bytes.len(), detail))
}

fn merge_restbl(dicts: &Dicts, mods: &[Mod], sizes: &[(String, usize)], out: &mut Vec<(String, Vec<u8>)>, rows: &mut Vec<Row>) -> Result<(), String> {
    let mut merged: Option<Restbl> = None;
    let mut count = 0;
    for m in mods {
        let mut found = Vec::new();
        walk(&m.romfs, &m.romfs, &mut |rel| {
            if rel.rsplit('/').next().unwrap_or("").contains("ResourceSizeTable") {
                found.push(rel);
            }
        })?;
        for rel in found {
            let (data, _used, _z) = unpack(dicts, &m.romfs.join(&rel), "restbl")?;
            let table = Restbl::parse(&data)?;
            match merged.as_mut() {
                Some(mr) => mr.merge(&table),
                None => merged = Some(table),
            }
            count += 1;
            let _ = rel;
        }
    }
    let Some(mut table) = merged else {
        return Ok(());
    };
    let mut raised = 0;
    for (rel, size) in sizes {
        let key = rel.strip_suffix(".zs").unwrap_or(rel);
        if table.raise(key, *size) {
            raised += 1;
        }
    }
    let rel = "System/Resource/ResourceSizeTable.Product.111.Nin_Ounce_NVN2.rsizetable.zs";
    out.push((rel.to_string(), zs::compress(&table.dump(), None)?));
    rows.push(Row {
        action: "merged",
        file: "System/Resource/ResourceSizeTable (tables of the mods)".to_string(),
        detail: format!("{count} tables, highest value per entry, {raised} raised for merged files"),
    });
    Ok(())
}

/// Merge the mods into `<out>/<name>/romfs`. The overlay holds the conflicting files only, unless
/// `all` is set; the report says what happened to every one of them.
pub fn run(romfs: &Path, mods: &[Mod], out: &Path, name: &str, all: bool, dry_run: bool) -> Result<Outcome, String> {
    run_with(romfs, mods, out, name, all, dry_run, &BTreeMap::new())
}

/// `overrides` names the mod that wins a file when it cannot be merged (relative path -> mod index).
pub fn run_with(
    romfs: &Path,
    mods: &[Mod],
    out: &Path,
    name: &str,
    all: bool,
    dry_run: bool,
    overrides: &BTreeMap<String, usize>,
) -> Result<Outcome, String> {
    if mods.len() < 2 {
        return Err("give at least two mods".into());
    }
    let dicts = Dicts::load(romfs)?;
    let files = scan(mods)?;
    let mut outcome = Outcome { files: files.len(), ..Default::default() };
    let mut written: Vec<(String, Vec<u8>)> = Vec::new();
    let mut sizes: Vec<(String, usize)> = Vec::new();

    for (rel, owners) in &files {
        if owners.len() < 2 || kind_of(rel) == "restbl" {
            continue;
        }
        outcome.overlaps += 1;
        outcome.owners.insert(rel.clone(), owners.clone());
        if let Some(chosen) = overrides.get(rel) {
            let Some(m) = mods.get(*chosen) else {
                return Err("an override names a mod that is not in the list".into());
            };
            outcome.kept += 1;
            outcome.rows.push(Row {
                action: "kept",
                file: rel.clone(),
                detail: format!("{} (chosen by you)", m.name),
            });
            if let Ok(data) = std::fs::read(m.romfs.join(rel)) {
                written.push((rel.clone(), data));
            }
            continue;
        }
        let high = *owners.last().unwrap();
        let high_name = &mods[high].name;
        if is_gamedata(rel) {
            match merge_gamedata(romfs, &dicts, mods, owners, rel) {
                Ok((bytes, len, detail)) => {
                    outcome.merged += 1;
                    sizes.push((rel.clone(), len));
                    written.push((rel.clone(), bytes));
                    outcome.rows.push(Row { action: "merged", file: rel.clone(), detail });
                }
                Err(e) => {
                    outcome.kept += 1;
                    outcome.rows.push(Row {
                        action: "kept",
                        file: rel.clone(),
                        detail: format!("{high_name}: the union failed ({e})"),
                    });
                    outcome.warnings.push(format!("{rel}: the union failed ({e}); {high_name} is used"));
                }
            }
            continue;
        }
        if refused(rel) {
            outcome.kept += 1;
            let mut said = Vec::new();
            let vanilla_path = romfs.join(rel);
            if vanilla_path.is_file() && (rel.ends_with(".byml.zs") || rel.ends_with(".byml") || rel.ends_with(".bgyml")) {
                if let Ok((vdata, _, _)) = unpack(&dicts, &vanilla_path, kind_of(rel)) {
                    if let Ok(vb) = byml::Byml::parse(&vdata) {
                        if let Some(vroot) = vb.root.as_ref() {
                            for m in owners {
                                if let Ok((d, _, _)) = unpack(&dicts, &mods[*m].romfs.join(rel), kind_of(rel)) {
                                    if let Ok(b) = byml::Byml::parse(&d) {
                                        if let Some(root) = b.root.as_ref() {
                                            said.push(format!("{}: {}", mods[*m].name, byml_delta(vroot, root)));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let why = if said.is_empty() {
                format!("{high_name} (not mergeable on its own: the format needs its own builder)")
            } else {
                format!("{} (not mergeable: its tables carry offsets and sizes only the game's builder can update)", said.join("; "))
            };
            outcome.rows.push(Row { action: "kept", file: rel.clone(), detail: why.clone() });
            outcome.warnings.push(format!("{}: {why}", rel));
            continue;
        }
        let kind = kind_of(rel);
        if kind == "bcett" || rel.ends_with(".bgyml") || rel.ends_with(".byml.zs") {
            let (high_data, used, high_z) = unpack(&dicts, &mods[high].romfs.join(rel), kind)?;
            let high_tree = byml::Byml::parse(&high_data)?;
            let mut tree = high_tree.root.clone().ok_or("empty byml")?;
            let mut notes: Vec<String> = Vec::new();
            let mut ok = true;
            let mut added = 0;
            let mut values = 0;
            for low in &owners[..owners.len() - 1] {
                let (low_data, _, _) = unpack(&dicts, &mods[*low].romfs.join(rel), kind)?;
                let low_tree = byml::Byml::parse(&low_data)?;
                let Some(low_root) = low_tree.root.as_ref() else { continue };
                let (merged_tree, st) = merge_byml(low_root, &tree, kind == "bcett");
                if !st.unkeyed.is_empty() {
                    notes.extend(st.notes.iter().take(3).cloned());
                    ok = false;
                }
                tree = merged_tree;
                notes.extend(st.notes.iter().take(2).cloned());
                added += st.rows_added;
                values += st.values + st.whole_rows;
            }
            let _ = ok;
            let data = byml::Byml { version: high_tree.version, big: high_tree.big, root: Some(tree) }.write();
            sizes.push((rel.clone(), data.len()));
            written.push((rel.clone(), pack_like(&data, &dicts, &used, high_z)?));
            outcome.merged += 1;
            outcome.rows.push(Row {
                action: "merged",
                file: rel.clone(),
                detail: format!("+{added} rows, {values} from {high_name}{}", if notes.is_empty() { String::new() } else { format!(" ({})", notes.join(", ")) }),
            });
        } else if kind == "sarc" {
            let (high_data, used, high_z) = unpack(&dicts, &mods[high].romfs.join(rel), kind)?;
            let mut entries = sarc::read(&high_data)?;
            let mut added = 0;
            let mut replaced = 0;
            for low in &owners[..owners.len() - 1] {
                let (low_data, _, _) = unpack(&dicts, &mods[*low].romfs.join(rel), kind)?;
                for (name, bytes) in sarc::read(&low_data)? {
                    match entries.iter_mut().find(|(n, _)| *n == name) {
                        Some((_, slot)) => {
                            if *slot != bytes {
                                replaced += 1;
                            }
                        }
                        None => {
                            entries.push((name, bytes));
                            added += 1;
                        }
                    }
                }
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let rebuilt = sarc::write(&entries);
            sizes.push((rel.clone(), rebuilt.len()));
            written.push((rel.clone(), pack_like(&rebuilt, &dicts, &used, high_z)?));
            outcome.merged += 1;
            outcome.rows.push(Row {
                action: "merged",
                file: rel.clone(),
                detail: format!("+{added} entries, {replaced} kept from {high_name}"),
            });
        } else if kind == "pack" {
            let vanilla_path = romfs.join(rel);
            let vanilla = if vanilla_path.is_file() {
                unpack(&dicts, &vanilla_path, kind).ok().map(|(d, _, _)| d)
            } else {
                None
            };
            let mut sides: Vec<Vec<u8>> = Vec::new();
            let mut used = None;
            let mut zipped = false;
            let mut ok = true;
            for m in owners {
                let (data, d, z) = unpack(&dicts, &mods[*m].romfs.join(rel), kind)?;
                if let Some(v) = &vanilla {
                    if data.len() != v.len() {
                        ok = false;
                    }
                }
                used = d;
                zipped = z;
                sides.push(data);
            }
            match vanilla {
                Some(v) if ok => {
                    let refs: Vec<&[u8]> = sides.iter().map(|s| s.as_slice()).collect();
                    let (merged, runs, conflicts) = three_way(&v, &refs);
                    sizes.push((rel.clone(), merged.len()));
                    written.push((rel.clone(), pack_like(&merged, &dicts, &used, zipped)?));
                    outcome.merged += 1;
                    outcome.rows.push(Row {
                        action: "merged",
                        file: rel.clone(),
                        detail: format!("{runs} changed runs applied, {conflicts} overlapping (the last mod wins those)"),
                    });
                }
                _ => {
                    outcome.kept += 1;
                    outcome.rows.push(Row {
                        action: "kept",
                        file: rel.clone(),
                        detail: format!("{high_name} (the mods' files have different sizes: the last mod in the list wins)"),
                    });
                    outcome.warnings.push(format!(
                        "{}: the packs of the mods differ in size, so {} (the last mod) is used",
                        rel, high_name
                    ));
                }
            }
        } else {
            outcome.kept += 1;
            let why = if kind == "zs" { "binary format" } else { "not a mergeable format" };
            outcome.rows.push(Row {
                action: "kept",
                file: rel.clone(),
                detail: format!("{high_name} ({why}: the last mod in the list wins)"),
            });
            outcome.warnings.push(format!(
                "{}: cannot be merged ({}), so {} (the last mod in the list) is used",
                rel, why, high_name
            ));
        }
    }

    merge_restbl(&dicts, mods, &sizes, &mut written, &mut outcome.rows)?;
    outcome.merged += 1;

    if let Some(better) = sorts_last(mods, name) {
        outcome.advice = Some(format!(
            "the merged folder is \"{name}\" but \"{better}\" sorts after every mod: a game reads the folder that \
             sorts last, so rename it (or pass --name) or the mods will keep winning",
        ));
    }

    if all {
        for (rel, owners) in &files {
            if owners.len() == 1 && kind_of(rel) != "restbl" {
                let src = mods[owners[0]].romfs.join(rel);
                if let Ok(data) = std::fs::read(&src) {
                    written.push((rel.clone(), data));
                }
            }
        }
    }

    if !dry_run {
        let root = out.join(name).join("romfs");
        for (rel, data) in &written {
            let dst = root.join(rel);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            std::fs::write(&dst, data).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
        outcome.written = written.len();
        let mut md = String::from("# Merged mod\n\n| what | file | detail |\n|---|---|---|\n");
        for r in &outcome.rows {
            md.push_str(&format!("| {} | `{}` | {} |\n", r.action, r.file, r.detail));
        }
        let _ = std::fs::write(out.join(name).join("MERGE-REPORT.md"), md);
    }
    Ok(outcome)
}

/// The name a merged folder needs so it sorts after every mod folder (a game reads the last one). The
/// tool calls its output `zzzz-merged`; that is not enough for a mod whose folder is called `zzzzz-...`.
pub fn sorts_last(mods: &[Mod], name: &str) -> Option<String> {
    let highest = mods.iter().map(|m| m.name.as_str()).max()?;
    if name > highest {
        return None;
    }
    let mut better = name.to_string();
    while better.as_str() <= highest {
        better.insert(0, 'z');
    }
    Some(better)
}
