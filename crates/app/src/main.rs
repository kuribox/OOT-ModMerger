//! `oot-mod-merge`: merge several OoT remake mod folders into one overlay.
//!
//! ```text
//! oot-mod-merge                       opens the window
//! oot-mod-merge check --romfs <romfs> <file.zs>   read a file and write it back, say if it is identical
//! oot-mod-merge merge --romfs <romfs> --out <dir> [--name zzzz-merged] [--all] [--dry-run] <mods...>
//! ```
//!
//! The last mod listed wins where two mods set the same value; the report says, file by file, what
//! was merged and what had to stay from one of them.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod config;
mod gui;
mod i18n;
mod theme;

use modmerge_core::{byml, merge, zs};

fn usage() -> &'static str {
    "oot-mod-merge: merge OoT remake mod folders into one overlay.\n\n\
     oot-mod-merge                      open the window\n\
     oot-mod-merge check --romfs <romfs> <file.zs>\n\
                                        read a BYML file and write it back; says whether the bytes match\n\
     oot-mod-merge merge --romfs <romfs> --out <dir> [options] <mod> [<mod>...]\n\
        --name <folder>                name of the folder written inside --out (default zzzz-merged)\n\
        --all                          copy every file, not only the ones in conflict\n\
        --dry-run                      report only, write nothing
        --win <file>=<mod>             the mod that wins <file> when it cannot be merged (repeatable)\n\n\
     Mods are given lowest priority first: the last one wins where a file cannot be merged.\n"
}

fn value_of(it: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    it.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some(cmd) = args.first() else {
        gui::main().map_err(|e| format!("window: {e}"))?;
        return Ok(ExitCode::SUCCESS);
    };
    let mut it = args[1..].iter().cloned();
    match cmd.as_str() {
        "-h" | "--help" | "help" => {
            print!("{}", usage());
            Ok(ExitCode::SUCCESS)
        }
        "check" => {
            let mut romfs = None;
            let mut file = None;
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--romfs" => romfs = Some(value_of(&mut it, "--romfs")?),
                    other => file = Some(other.to_string()),
                }
            }
            let (romfs, file) = (romfs.ok_or("--romfs is missing")?, file.ok_or("a file is missing")?);
            check(Path::new(&romfs), Path::new(&file))
        }
        "merge" => {
            let mut romfs = None;
            let mut out = None;
            let mut name = "zzzz-merged".to_string();
            let mut all = false;
            let mut dry = false;
            let mut wins: Vec<(String, String)> = Vec::new();
            let mut mods = Vec::new();
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--romfs" => romfs = Some(value_of(&mut it, "--romfs")?),
                    "--out" => out = Some(value_of(&mut it, "--out")?),
                    "--name" => name = value_of(&mut it, "--name")?,
                    "--all" => all = true,
                    "--dry-run" => dry = true,
                    "--win" => {
                        let v = value_of(&mut it, "--win")?;
                        let (file, who) = v.split_once('=').ok_or("--win wants <file>=<mod name>")?;
                        wins.push((file.to_string(), who.to_string()));
                    }
                    other => mods.push(other.to_string()),
                }
            }
            let romfs = PathBuf::from(romfs.ok_or("--romfs is missing")?);
            let out = PathBuf::from(out.ok_or("--out is missing")?);
            let mods: Vec<merge::Mod> = mods
                .iter()
                .map(|m| {
                    let p = PathBuf::from(m);
                    let romfs = if p.join("romfs").is_dir() { p.join("romfs") } else { p.clone() };
                    let name = p
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| m.clone());
                    merge::Mod { name, romfs }
                })
                .collect();
            let mut overrides = std::collections::BTreeMap::new();
            for (file, who) in &wins {
                let idx = mods
                    .iter()
                    .position(|m| &m.name == who)
                    .ok_or_else(|| format!("--win {file}={who}: no mod is called {who}"))?;
                overrides.insert(file.clone(), idx);
            }
            let outcome = merge::run_with(&romfs, &mods, &out, &name, all, dry, &overrides)?;
            for r in &outcome.rows {
                println!("{:<7} {:<70} {}", r.action, r.file, r.detail);
            }
            println!(
                "\n{} files, {} in conflict: {} merged, {} kept from one mod{}",
                outcome.files,
                outcome.overlaps,
                outcome.merged,
                outcome.kept,
                if dry { String::new() } else { format!("; {} written to {}", outcome.written, out.join(&name).display()) }
            );
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command {other:?}\n\n{}", usage())),
    }
}

fn check(romfs: &Path, file: &Path) -> Result<ExitCode, String> {
    let dicts = zs::Dicts::load(romfs)?;
    let kind = merge::kind_of(&file.to_string_lossy());
    let raw = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut last = String::new();
    for cand in zs::candidates(kind) {
        let dict = cand.and_then(|n| dicts.get(n));
        if !zs::is_zstd(&raw) {
            let parsed = byml::Byml::parse(&raw)?;
            let back = parsed.write();
            let again = byml::Byml::parse(&back)?;
            println!("{}: {} bytes, stored uncompressed, read+write keeps every node: {}", file.display(), raw.len(), parsed.root == again.root);
            return Ok(if parsed.root == again.root { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        let Ok(data) = zs::decompress(&raw, dict) else {
            last = format!("{cand:?} did not decompress");
            continue;
        };
        let parsed = byml::Byml::parse(&data)?;
        let back = parsed.write();
        let again = byml::Byml::parse(&back)?;
        let same_tree = parsed.root == again.root;
        if same_tree {
            println!(
                "{}: {} bytes, BYML v{}, read+write keeps every node ({} bytes out, {}) (dict {cand:?})",
                file.display(),
                data.len(),
                parsed.version,
                back.len(),
                if back == data { "byte identical" } else { "different layout, same tree" }
            );
            return Ok(ExitCode::SUCCESS);
        }
        let hdr = |d: &[u8]| {
            let u = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
            (u(4), u(8), u(12))
        };
        println!(
            "{}: round trip differs ({} vs {} bytes, first at {:?}); headers (key table, strings, root): {:?} vs {:?}",
            file.display(),
            data.len(),
            back.len(),
            data.iter().zip(back.iter()).position(|(a, b)| a != b),
            hdr(&data),
            hdr(&back)
        );
        return Ok(ExitCode::FAILURE);
    }
    Err(last)
}
