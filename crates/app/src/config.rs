//! What the window remembers between runs: the game's RomFS, the output folder, the name, and the
//! mod folders in their order. A plain `key=value` text file, so it can be read and fixed by hand.

use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct Config {
    pub lang: String,
    pub romfs: String,
    pub out: String,
    pub name: String,
    pub all: bool,
    /// The mod folders, lowest priority first.
    pub mods: Vec<String>,
    /// A file forced to one mod: `file`, `mod name`.
    pub wins: Vec<(String, String)>,
}

pub fn path() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    base.join("OOT-ModMerge").join("config.txt")
}

impl Config {
    pub fn load() -> Config {
        let mut c = Config { name: "zzzzzz-merged".to_string(), ..Default::default() };
        let Ok(text) = std::fs::read_to_string(path()) else {
            return c;
        };
        for line in text.lines() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else { continue };
            match key {
                "lang" => c.lang = value.to_string(),
                "romfs" => c.romfs = value.to_string(),
                "out" => c.out = value.to_string(),
                "name" if !value.is_empty() => c.name = value.to_string(),
                "all" => c.all = value == "1" || value.eq_ignore_ascii_case("true"),
                "mod" if !value.is_empty() => c.mods.push(value.to_string()),
                "win" => {
                    if let Some((file, who)) = value.split_once('\t') {
                        c.wins.push((file.to_string(), who.to_string()));
                    }
                }
                _ => {}
            }
        }
        c
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut text = String::new();
        text.push_str("# OoT Mod Merge: what the window remembers\n");
        text.push_str(&format!("lang={}\n", self.lang));
        text.push_str(&format!("romfs={}\n", self.romfs));
        text.push_str(&format!("out={}\n", self.out));
        text.push_str(&format!("name={}\n", self.name));
        text.push_str(&format!("all={}\n", if self.all { 1 } else { 0 }));
        for m in &self.mods {
            text.push_str(&format!("mod={m}\n"));
        }
        for (file, who) in &self.wins {
            text.push_str(&format!("win={file}\t{who}\n"));
        }
        let _ = std::fs::write(p, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_survives_a_round_trip() {
        let dir = std::env::temp_dir().join("oot-mod-merge-config-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("APPDATA", &dir);

        let written = Config {
            lang: "es".to_string(),
            romfs: r"F:\Switch2\Longshot\a+\nightly\nightly-800-6123c76a\romfs".to_string(),
            out: r"F:\Switch2\Longshot\a+\nightly\nightly-800-6123c76a\mods".to_string(),
            name: "zzzzzz-merged".to_string(),
            all: true,
            mods: vec![r"F:\m\08-HylianSurvivors".to_string(), r"F:\m\09-Biggoron=Complete".to_string()],
            wins: vec![("GameData/GameDataList.Product.110.byml.zs".to_string(), "08-HylianSurvivors".to_string())],
        };
        written.save();
        let read = Config::load();
        assert_eq!(read.lang, written.lang);
        assert_eq!(read.romfs, written.romfs);
        assert_eq!(read.out, written.out);
        assert_eq!(read.name, written.name);
        assert!(read.all);
        assert_eq!(read.mods, written.mods, "the order of the mods is kept");
        assert_eq!(read.wins, written.wins);
    }
}
