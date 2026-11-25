# The GameData list, and how two mods' lists are joined

`GameData/GameDataList.Product.110.byml.zs` is not a plain table. Besides the rows it holds
`MetaData`, which describes the save layout the game uses for `slot_%02d/progress.sav`,
`option.sav`, `slot_%02d/caption.sav` and `account.sav`:

| field | vanilla | meaning as measured |
|---|---|---|
| `AllDataSaveOffset` | 106216 | shifted by 8 bytes for every saved variable a mod adds |
| `AllDataSaveSize` | 842876 | same shift |
| `SaveDataOffsetPos[0..4]` | 95704, 672, 672, 320 | per save file, shifted by the variables added to that file |
| `SaveDataSize[0..4]` | 703884, 44660, 42356, 320 | same, per save file |
| `SaveDirectory[0..4]` | `slot_%02d/progress.sav`, `option.sav`, … | which file each block is |
| `SaveTypeHash[0..4]` | 2749067540, … | never seen changed by a mod |

## What the mod builders actually do

Measured on three real mods (`08-HylianSurvivors`, `09-Biggoron`, `06-young-link-rides-epona`):

* each type array (`Bool`, `BoolExp`, `Enum`, `Float`, `Int`, …) keeps the **vanilla rows as a
  prefix, in vanilla order**, and the mod's new rows are **appended at the end**;
* the four numbers above grow by **8 bytes per row whose `SaveFileIndex` is 0..3** (a row with
  `-1` is not saved: `BoolExp`, `Float` and most of Hylian's `Int` rows are `-1`).

HylianSurvivors adds 112 Bool + 61 BoolExp + 2 Enum + 1 Float + 12 Int; 56 of them are saved, and
its `MetaData` is vanilla + 448 exactly (56 × 8). Biggoron adds 6 saved Bool → +48. epona adds
3 saved Bool + 1 unsaved BoolExp → +24. The arithmetic is exact, which is the evidence that the
builders do nothing else: no hashes, no index tables, no re-encoding.

## The union this tool builds

1. rows: vanilla prefix, then the added rows of each mod in priority order (deduplicated by the
   row key, so two mods adding the same hash are taken once);
2. `MetaData`: vanilla, plus 8 bytes per added saved row — `AllDataSaveOffset` and
   `AllDataSaveSize` by the total, `SaveDataOffsetPos[i]` and `SaveDataSize[i]` by that save
   file's own share;
3. every other field is left untouched, and the file is written back with the same BYML version
   and compressed with the game's `zs.zsdic`.

Measured on the nine mods of the test install: 198 rows kept (+121 Bool, +62 BoolExp, +2 Enum,
+1 Float, +12 Int), 65 saved variables, `save slot 0 +520 B`, decompressed 1 563 304 bytes.

## What is still not safe

* A save written under a *single* mod's list and then loaded under the union reads the added
  variables 8 × (the other mod's saved rows) bytes away: the vanilla flags are unaffected (their
  order never changes) but the mod flags come back as whatever sits at that offset. Start a fresh
  save, or keep the same list for the whole playthrough.
* `SaveDataOffsetPos` and `AllDataSaveSize` semantics are only observed, not documented: the
  measurements above are for rows added to save file 0, which is what every mod seen so far does.
  A mod adding rows to `option.sav` would need a check against its own file before trusting the
  union of those numbers.
* `Event/EventFlow/*.bfevfl.zs` is a flowchart (events plus entry points that index each other):
  there is no union for it here, so one mod's file has to win.
