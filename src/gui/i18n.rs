//! Translations: texts are written in English in the code and the QML (`Aeroport.tr("New Tab")`) and translated by a flat JSON
//! catalog {"English text": "translation"} embedded in the binary (i18n/). Language: FILI_LANG, else the choice of
//! fili-lang ($XDG_STATE_HOME/filigrane/locale), else LC_ALL / LC_MESSAGES / LANG. No catalog or no entry: English.
use std::{collections::HashMap, env, fs, sync::OnceLock};

const EMBEDDED: [(&str, &str); 1] = [("fr", include_str!("../../i18n/fr.json"))];

static CATALOG: OnceLock<HashMap<String, String>> = OnceLock::new();

fn lang() -> String {
    let state = env::var("XDG_STATE_HOME")
        .unwrap_or_else(|_| format!("{}/.local/state", env::var("HOME").unwrap_or_default()));
    let choice = fs::read_to_string(format!("{state}/filigrane/locale"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let value = env::var("FILI_LANG")
        .ok()
        .filter(|v| !v.is_empty())
        .or(Some(choice).filter(|v| !v.is_empty()))
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|k| env::var(k).ok().filter(|v| !v.is_empty()))
        })
        .unwrap_or_default();
    let language: String = value
        .chars()
        .take_while(|c| !matches!(c, '_' | '.' | '@'))
        .collect();
    if !language.is_empty() && language.chars().all(|c| c.is_ascii_lowercase()) {
        language
    } else {
        "en".into()
    }
}

fn catalog() -> &'static HashMap<String, String> {
    CATALOG.get_or_init(|| {
        let lang = lang();
        EMBEDDED
            .iter()
            .find(|(l, _)| *l == lang)
            .and_then(|(_, text)| serde_json::from_str(text).ok())
            .unwrap_or_default()
    })
}

pub fn tr(text: &str) -> String {
    catalog()
        .get(text)
        .cloned()
        .unwrap_or_else(|| text.to_owned())
}
