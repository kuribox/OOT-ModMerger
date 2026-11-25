# 0.0.1-alpha

First public build of **OoT Mod Merge**, a Windows program that joins the files two or more mods of
the Ocarina of Time remake both change, so the mods work together instead of one overwriting the
other. It reads the format of each file and the game's own copy of it; no rule knows about a
particular mod.

## What it merges

- **Banc scenes** (`*.bcett.byml.zs`) — actors, AI groups, rails and placement groups unioned by key.
- **RSDB tables** (`*.rstbl.byml.zs`) and **GameParameter** (`*.bgyml`) — rows unioned by
  `__RowId` / `Hash`, dictionaries unioned by key.
- **Text packs** (`Mals/*.sarc.zs`, one per language) — entries unioned by name.
- **Actor and enemy packs** (`*.pack.zs`) — three-way against the game's file, byte run by byte run.
- **GameData list** (`GameData/GameDataList*.byml.zs`) — rows of every mod appended after the game's
  own, and the save layout in `MetaData` grown by 8 bytes per saved variable, which is what every
  mod's builder does.
- **Resource size table** — highest size per entry, raised for the files the merge grew.

## Priority

Mods are listed lowest priority first: where a single value cannot be combined (the same actor, the
same row field, the same run of bytes), the mod that **modifies that thing** and sits lower in the
list wins. One file can be forced to a given mod from the window or with `--win <file>=<mod>`.

## What it does not merge

Event flows (`*.bfevfl.zs`), logic (`*.ainb`), textures (`*.bntx`), models (`*.blarc` / `*.bfres`),
layouts, fonts and sounds: they are graphs or packed data with no key to join them on. The last mod's
copy is used and the report lists every one of them.

## How to use it

Open `oot-mod-merge.exe` with no arguments for the window (Spanish and English): set the game's
`romfs`, drag the mod folders in, reorder them, **Scan conflicts** to see the result without writing,
**Merge**. Or from a terminal:

```
oot-mod-merge.exe merge --romfs <game>/romfs --out <game>/mods --name zzzzzz-merged <mod>... 
```

The merged folder is an overlay named so it sorts after the other mods, holding only the files that
were in conflict; `MERGE-REPORT.md` next to it says what happened to each one.

## Known limits

- Requires the game's own RomFS (it carries the zstd dictionaries and the vanilla files the merge
  compares against).
- A pack whose copies do not decompress to the same size cannot be merged three-way: the last mod's
  copy is used and said so.
- A save written under a single mod's GameData list and then loaded under the union reads the added
  variables shifted; start a fresh save or keep one list per playthrough.
- Windows only (the window uses wgpu); the CLI is the part that would port easily.
