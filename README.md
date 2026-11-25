# OoT Mod Merge

A small Windows program that merges the files two (or more) mods of the Ocarina of Time remake both
change, so both mods work at once instead of one overwriting the other.

A recompiled game (Longshot, Hookshot, ReLink) and the emulators read **one** copy of a file: the one
from the mod folder whose name sorts last. When two mods ship the same file, one mod's change is
silently lost. This program combines those files with a fixed rule per format, and says out loud
which ones it cannot combine.

Everything is decided by the format of the file and by comparing it with the game's own (vanilla) copy
from `--romfs`; no rule knows about a particular mod.

## The rule per format

| files | what happens |
|---|---|
| `Banc/**/*.bcett.byml.zs` | **merged**: actors, AI groups, rails, placement groups and the game-data table are matched by their own key (`Hash`, `GroupGUID`, `PlgGuid`) and unioned. An actor two mods changed differently is taken whole from the mod listed last (a field-wise mix of two placements is not a placement). |
| `*rsizetable*` | **merged**: highest value per entry over every mod's table, then raised for the files this merge grew so a bigger file still fits. |
| `*.game__*.bgyml`, plain `.byml` | **merged**: dictionaries are unioned, keyed rows matched, a scalar two mods set differently comes from the last mod; unkeyed lists of values are unioned, an unkeyed list of records is kept from the last mod and said so. |
| `*.sarc.zs` (the `Mals` text packs) | **merged**: entries unioned by name, so each mod's text edits survive. |
| `*.pack.zs` (actor and enemy packs) | **merged three-way against the game's own file**: every run of bytes a mod changed is applied, and where two mods changed the same run the higher-priority one wins. Only when all copies decompress to the same size (a pool of fixed-size records); otherwise the last mod's copy is used and the report says so. |
| `GameData/GameDataList*.byml.zs` | **merged**: the rows of every mod appended after the game's own (the order every mod builder uses), and the save layout in `MetaData` grown by 8 bytes per saved variable. See `docs/GAMEDATA.md`. |
| `RSDB/*.rstbl.byml.zs` | **merged**: the row arrays of every mod unioned by `__RowId` / `Hash`. |
| `*.bfevfl.zs` (event flows), `*.ainb` (logic), `*.bntx` (textures), `*.blarc`/`*.bfres` (models), layouts, fonts, sounds | **not merged**: they are graphs or packed data with no key to join them on. The last mod's copy is used and the report lists it. |

Every merged file is read back after it is written and compared node by node with what was written, so
a merge that would produce a file the game cannot read is not reported as merged.

## Using it

```
oot-mod-merge.exe                      the window
oot-mod-merge.exe merge --romfs <game>/romfs --out <folder> [--name zzzz-merged] [--all] [--dry-run]
                                [--win <file>=<mod>]... <mod>...
oot-mod-merge.exe check --romfs <game>/romfs <file.zs>
```

- `--romfs` is the game's own RomFS: the program reads `Pack/ZsDic.pack.zs` from it for the zstd
  dictionaries the files are compressed with, and the game's own copy of every file it merges.
- Mods are given **lowest priority first**: where a single value cannot be combined, the mod listed
  **last** wins. A file can be forced to a particular mod with `--win <file>=<mod>`, or from the window.
- The output is `<out>/<name>/romfs` holding **only the files that were in conflict** — so it is an
  overlay: leave the mod folders where they are and this folder on top (its name must sort last).
  `--all` copies every file instead, for a single self-contained mod folder.
- `MERGE-REPORT.md` next to `romfs/` lists every file: merged (with what came from which mod) or kept
  from one mod, and why.

For a recompiled game the mods live in `<game>/mods/<name>/romfs`; put the merged folder there under a
name that sorts after the others (`zzzzzz-merged`) and it wins for the files it carries.

In the window: drag the mod folders in, reorder them with the handle, press **Scan conflicts** to see
what would happen without writing anything, and **Merge** to write it. Text in Spanish and English.

## Building

```bash
export PATH="/c/Users/Kuri/.cargo/bin:$PATH"
cargo build --release          # target/release/oot-mod-merge.exe
python tools/make_release.py   # copies it to releases/ with its SHA-256
```

The toolchain here is `stable-x86_64-pc-windows-gnu`; `.cargo/config.toml` links with `rust-lld`
(GNU `ld` produces binaries that crash before `main` when `wgpu`/`egui` are linked in). zstd is the
pure-Rust `structured-zstd` crate (the C-backed one needs a compiler this toolchain does not have).

## Layout

```
crates/core/src/byml.rs     BYML reader/writer (v2-v7, both endiannesses)
crates/core/src/restbl.rs   the resource size table (merge, raise, crc32)
crates/core/src/sarc.rs     SARC reader (ZsDic.pack.zs)
crates/core/src/zs.rs       zstd with the game's dictionaries
crates/core/src/merge.rs    the rules above, the overlay writer and the report
crates/app/src/main.rs      the command line
crates/app/src/gui.rs       the window
crates/app/src/i18n.rs      its texts, Spanish and English
crates/app/src/theme.rs     its palette and the triforce it draws
docs/GAMEDATA.md            what the GameData list is and how two of them are joined
tools/make_release.py       builds and copies the exe into releases/
```
