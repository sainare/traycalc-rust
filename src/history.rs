use std::fs;

use crate::config;

pub const MAX_ENTRIES: usize = 100;

#[derive(Clone, Debug)]
pub struct Entry {
    pub expression: String,
    pub result: String,
}

pub fn load() -> Vec<Entry> {
    let Ok(text) = fs::read_to_string(path()) else { return Vec::new() };
    text.lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(expression, result)| Entry { expression: expression.into(), result: result.into() })
        .take(MAX_ENTRIES)
        .collect()
}

pub fn save(entries: &[Entry]) {
    let _ = fs::create_dir_all(config::app_dir());
    let text = entries
        .iter()
        .take(MAX_ENTRIES)
        .map(|entry| format!("{}\t{}", entry.expression, entry.result))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = fs::write(path(), text);
}

fn path() -> std::path::PathBuf {
    config::app_dir().join("history.txt")
}

