//! The window's texts, in the languages it ships.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    pub fn all() -> [Lang; 2] {
        [Lang::Es, Lang::En]
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::Es => "ES",
            Lang::En => "EN",
        }
    }

    /// Spanish when the system speaks it, English otherwise.
    pub fn from_system() -> Lang {
        let mut buf = String::new();
        if let Ok(v) = std::env::var("LANG") {
            buf.push_str(&v);
        }
        if let Ok(v) = std::env::var("LANGUAGE") {
            buf.push_str(&v);
        }
        if buf.is_empty() {
            // Windows: the UI language of the session.
            if let Ok(out) = std::process::Command::new("powershell")
                .args(["-NoProfile", "-Command", "(Get-Culture).Name"])
                .output()
            {
                buf.push_str(&String::from_utf8_lossy(&out.stdout));
            }
        }
        if buf.to_lowercase().contains("es") { Lang::Es } else { Lang::En }
    }
}

pub fn t(lang: Lang, key: &str) -> &'static str {
    match (lang, key) {
        (Lang::Es, "title") => "OoT Mod Merge",
        (Lang::En, "title") => "OoT Mod Merge",
        (Lang::Es, "subtitle") => "los archivos de jugabilidad se combinan; los modelos y texturas se toman del último mod",
        (Lang::En, "subtitle") => "gameplay files are combined; models and textures are taken from the last mod",

        (Lang::Es, "game") => "Juego",
        (Lang::En, "game") => "Game",
        (Lang::Es, "romfs") => "RomFS",
        (Lang::En, "romfs") => "RomFS",
        (Lang::Es, "output") => "Salida",
        (Lang::En, "output") => "Output",
        (Lang::Es, "browse") => "Buscar...",
        (Lang::En, "browse") => "Browse...",
        (Lang::Es, "romfs_hint") => "...\\romfs",
        (Lang::En, "romfs_hint") => "...\\romfs",
        (Lang::Es, "out_hint") => "dónde se escribe la carpeta fusionada",
        (Lang::En, "out_hint") => "where the merged folder is written",

        (Lang::Es, "mods") => "Mods",
        (Lang::En, "mods") => "Mods",
        (Lang::Es, "mods_hint") => "el de menos prioridad primero · suelta una carpeta en la ventana · arrastra el asa para reordenar",
        (Lang::En, "mods_hint") => "lowest priority first · drop a folder on the window · drag the handle to reorder",
        (Lang::Es, "add") => "Añadir...",
        (Lang::En, "add") => "Add...",
        (Lang::Es, "pick_mod") => "Una carpeta de mod (con romfs/ dentro)",
        (Lang::En, "pick_mod") => "A mod folder (with romfs/ inside)",
        (Lang::Es, "pick_romfs") => "La carpeta RomFS del juego",
        (Lang::En, "pick_romfs") => "The game's RomFS folder",
        (Lang::Es, "pick_out") => "Dónde se escribe la carpeta fusionada",
        (Lang::En, "pick_out") => "Where the merged mod folder goes",

        (Lang::Es, "output_name") => "Nombre de salida",
        (Lang::En, "output_name") => "Output name",
        (Lang::Es, "copy_all") => "copiar todos los archivos, no sólo los conflictos",
        (Lang::En, "copy_all") => "copy every file, not only the conflicts",
        (Lang::Es, "scan") => "Ver conflictos",
        (Lang::En, "scan") => "Scan conflicts",
        (Lang::Es, "merge") => "Fusionar",
        (Lang::En, "merge") => "Merge",
        (Lang::Es, "name_hint") => "el nombre debe ordenar después de todos los mods: el juego lee la carpeta que ordena última",
        (Lang::En, "name_hint") => "the name has to sort after every mod: a game reads the folder that sorts last",

        (Lang::Es, "conflicts") => "Conflictos",
        (Lang::En, "conflicts") => "Conflicts",
        (Lang::Es, "filter") => "filtrar por nombre",
        (Lang::En, "filter") => "filter by file name",
        (Lang::Es, "cannot_merge") => "no se pueden fusionar",
        (Lang::En, "cannot_merge") => "cannot be merged",
        (Lang::Es, "warn_title") => "En conflicto y sin fusión posible: gana el mod elegido a la derecha (por defecto el último)",
        (Lang::En, "warn_title") => "In conflict and not mergeable: the mod chosen on the right wins (the last one by default)",
        (Lang::Es, "warn_more") => "... y más (todos en MERGE-REPORT.md)",
        (Lang::En, "warn_more") => "... and more (all of them in MERGE-REPORT.md)",
        (Lang::Es, "col_file") => "archivo",
        (Lang::En, "col_file") => "file",
        (Lang::Es, "col_what") => "qué",
        (Lang::En, "col_what") => "what",
        (Lang::Es, "col_detail") => "detalle",
        (Lang::En, "col_detail") => "detail",
        (Lang::Es, "merged") => "fusion",
        (Lang::En, "merged") => "merged",
        (Lang::Es, "kept") => "conflicto",
        (Lang::En, "kept") => "conflict",
        (Lang::Es, "col_state") => "estado",
        (Lang::En, "col_state") => "state",
        (Lang::Es, "none_wins") => "ninguno (gana el ultimo)",
        (Lang::En, "none_wins") => "none (the last mod wins)",
        (Lang::Es, "winner") => "gana",
        (Lang::En, "winner") => "winner",
        (Lang::Es, "done_title") => "Fusion completada",
        (Lang::En, "done_title") => "Merge finished",
        (Lang::Es, "error_title") => "No se pudo fusionar",
        (Lang::En, "error_title") => "The merge failed",
        (Lang::Es, "ok") => "Vale",
        (Lang::En, "ok") => "OK",
        (Lang::Es, "no_conflicts") => "sin conflictos: nada que decidir",
        (Lang::En, "no_conflicts") => "no conflicts: nothing to decide",

        (Lang::Es, "drop_hint") => "Suelta las carpetas de mods aquí (la de menos prioridad primero)",
        (Lang::En, "drop_hint") => "Drop the mod folders here (lowest priority first)",
        (Lang::Es, "need_two") => "Añade al menos dos mods.",
        (Lang::En, "need_two") => "Add at least two mods.",
        (Lang::Es, "need_romfs") => "Indica la carpeta RomFS del juego (lleva los diccionarios zstd).",
        (Lang::En, "need_romfs") => "Set the game's RomFS folder (it carries the zstd dictionaries).",
        (Lang::Es, "need_out") => "Indica la carpeta de salida.",
        (Lang::En, "need_out") => "Set the output folder.",
        (Lang::Es, "not_a_mod") => "Ninguna de las carpetas es un mod (un mod tiene romfs/).",
        (Lang::En, "not_a_mod") => "None of the folders listed is a mod (a mod has a romfs/ folder).",
        (Lang::Es, "scanning") => "Leyendo los conflictos...",
        (Lang::En, "scanning") => "Scanning...",
        (Lang::Es, "merging") => "Fusionando...",
        (Lang::En, "merging") => "Merging...",
        (Lang::Es, "error") => "error",
        (Lang::En, "error") => "error",
        (Lang::Es, "files") => "archivos",
        (Lang::En, "files") => "files",
        (Lang::Es, "in_conflict") => "en conflicto",
        (Lang::En, "in_conflict") => "in conflict",
        (Lang::Es, "written_to") => "escritos en",
        (Lang::En, "written_to") => "written to",
        (Lang::Es, "lang") => "Idioma",
        (Lang::En, "lang") => "Language",
        (Lang::Es, "up") => "Subir",
        (Lang::En, "up") => "Up",
        (Lang::Es, "down") => "Bajar",
        (Lang::En, "down") => "Down",
        (Lang::Es, "remove") => "Quitar",
        (Lang::En, "remove") => "Remove",
        (Lang::Es, "drag") => "Arrastra para reordenar",
        (Lang::En, "drag") => "Drag to reorder",
        (Lang::Es, "missing_path") => "esa carpeta no existe",
        (Lang::En, "missing_path") => "that folder does not exist",
        (Lang::Es, "missing_mod") => "falta",
        (Lang::En, "missing_mod") => "missing",
        (Lang::Es, "warn_prefix") => "Aviso:",
        (Lang::En, "warn_prefix") => "Warning:",
        _ => "",
    }
}
