# 0.0.2-alpha

Second alpha of **OoT Mod Merge**. The merge of Banc scenes is finer and the report says who gives what.

## What changed since 0.0.1-alpha

- **Banc actors merge field by field.** An actor both mods changed used to be taken whole from the higher one; now each
  field merges, so the two mods' changes to one actor are both kept.
- **Repeated keys pair by occurrence.** A list with a repeated key (an actor's Links are all named `Enemy`) was taken
  whole from one mod; the second `Enemy` of one file now pairs with the second of the other.
- **A report per mod.** Every file lists each mod with what it adds and how many rows it retunes (before, the count of
  the higher mod's rows was printed as if it were the lower one's), and the merge counts, against the game's own file,
  every field it keeps or drops.
- Measured on eleven mods: 2,147 fields changed by one of them, 2,103 kept, 44 overridden; 139 of 147 merged files are
  identical whatever the order of the mods.

What it merges, priority and what it does not merge are as in 0.0.1-alpha. Windows only; no game data is included.
