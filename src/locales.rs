use serde_json::Value;
use std::sync::LazyLock;

const SOURCES: &[(&str, &str, &str)] = &[
    ("en", "English", include_str!("../assets/locales/en.json")),
    ("fi", "Suomi", include_str!("../assets/locales/fi.json")),
    ("es", "Español", include_str!("../assets/locales/es.json")),
    ("de", "Deutsch", include_str!("../assets/locales/de.json")),
    ("fr", "Français", include_str!("../assets/locales/fr.json")),
    (
        "pt-BR",
        "Português (Brasil)",
        include_str!("../assets/locales/pt-BR.json"),
    ),
];

static CATALOGS: LazyLock<Vec<Value>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|(code, _, text)| {
            serde_json::from_str(text).unwrap_or_else(|e| panic!("Invalid {code} catalog: {e}"))
        })
        .collect()
});

pub fn languages() -> impl Iterator<Item = (&'static str, &'static str)> {
    SOURCES.iter().map(|&(code, name, _)| (code, name))
}

pub fn source(code: &str) -> Option<&'static str> {
    SOURCES
        .iter()
        .find(|(id, _, _)| *id == code)
        .map(|&(_, _, raw)| raw)
}

pub fn normalize(tag: &str) -> &'static str {
    let base = tag
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    languages()
        .map(|(code, _)| code)
        .find(|code| code.split('-').next() == Some(base.as_str()))
        .unwrap_or("en")
}

pub fn text(language: &str, key: &str) -> &'static str {
    let normalized = normalize(language);
    let index = SOURCES
        .iter()
        .position(|(code, _, _)| *code == normalized)
        .unwrap_or(0);
    CATALOGS[index][key]
        .as_str()
        .or_else(|| CATALOGS[0][key].as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(mut text: &str) -> Vec<&str> {
        let mut names = Vec::new();
        while let Some((_, rest)) = text.split_once("{{") {
            let (name, tail) = rest.split_once("}}").expect("unclosed placeholder");
            names.push(name);
            text = tail;
        }
        names.sort_unstable();
        names
    }

    #[test]
    fn every_locale_covers_the_same_keys_and_placeholders() {
        let english = CATALOGS[0].as_object().unwrap();
        for ((language, _, _), catalog) in SOURCES.iter().zip(CATALOGS.iter()) {
            let catalog = catalog.as_object().unwrap();
            assert_eq!(
                catalog.keys().collect::<Vec<_>>(),
                english.keys().collect::<Vec<_>>(),
                "{language}"
            );
            for (key, value) in catalog {
                let value = value.as_str().unwrap();
                assert!(!value.trim().is_empty(), "{language}: {key}");
                assert_eq!(
                    placeholders(value),
                    placeholders(english[key].as_str().unwrap()),
                    "{language}: {key}"
                );
            }
        }
        for pid in crate::obd::PIDS {
            assert!(
                english.contains_key(&format!("pid.{}", pid.name)),
                "{}",
                pid.name
            );
        }
    }

    #[test]
    fn language_tags_use_supported_catalogs() {
        assert_eq!(normalize("fr-CA"), "fr");
        assert_eq!(normalize("PT_br"), "pt-BR");
        assert_eq!(normalize("sv-FI"), "en");
    }
}
