//! Guided prompts, shared by the browser and MCP.

use crate::locales;
use serde_json::{json, Map, Value};

const CONTEXT: &[&str] = &[
    "vehicle_id",
    "session_id",
    "from_ms",
    "to_ms",
    "event_id",
    "annotation_id",
];
const PROMPTS: &[&str] = &[
    "check_engine_light",
    "rough_running",
    "fuel_economy",
    "inspection_ready",
    "intermittent",
    "used_car",
    "overview",
];

macro_rules! history {
    ($($id:literal),+ $(,)?) => {
        const HISTORY: &[&str] = &[$($id),+];
        fn history_text(id: &str, language: &str) -> Option<&'static str> {
            match id {
                $($id => Some(match language {
                    "fi" => include_str!(concat!("../assets/prompts/", $id, ".fi.md")),
                    "es" => include_str!(concat!("../assets/prompts/", $id, ".es.md")),
                    "de" => include_str!(concat!("../assets/prompts/", $id, ".de.md")),
                    "fr" => include_str!(concat!("../assets/prompts/", $id, ".fr.md")),
                    "pt-BR" => include_str!(concat!("../assets/prompts/", $id, ".pt-BR.md")),
                    _ => include_str!(concat!("../assets/prompts/", $id, ".en.md")),
                }),)+
                _ => None,
            }
        }
    };
}
history!(
    "identify_symptom",
    "explain_previous_fault",
    "compare_after_clear"
);

fn ids() -> impl Iterator<Item = &'static str> {
    PROMPTS.iter().chain(HISTORY).copied()
}

fn title(id: &str, language: &str) -> &'static str {
    locales::text(language, &format!("prompt.{id}.title"))
}

fn body(id: &str, language: &str) -> &'static str {
    history_text(id, language)
        .unwrap_or_else(|| locales::text(language, &format!("prompt.{id}.text")))
}

fn full_text(id: &str, language: &str) -> String {
    format!(
        "{}\n\n{}",
        body(id, language).trim_end(),
        locales::text(language, "prompt.safety")
    )
}

pub fn for_ui() -> Value {
    Value::Array(
        ids()
            .map(|id| {
                let titles: Map<_, _> = locales::languages()
                    .map(|(lang, _)| (lang.into(), json!(title(id, lang))))
                    .collect();
                let texts: Map<_, _> = locales::languages()
                    .map(|(lang, _)| (lang.into(), json!(full_text(id, lang))))
                    .collect();
                json!({"id": id, "history": HISTORY.contains(&id), "title": titles, "text": texts})
            })
            .collect(),
    )
}

pub fn mcp_list() -> Vec<Value> {
    ids()
        .map(|id| {
            json!({"name": id, "title": title(id, "en"),
                "description": body(id, "en").split('.').next().unwrap_or(""),
                "arguments": std::iter::once("language").chain(CONTEXT.iter().copied())
                    .map(|name| json!({"name": name, "required": false})).collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub fn mcp_get(name: &str, language: &str, args: &Value) -> Option<Value> {
    let id = ids().find(|&id| id == name)?;
    let language = locales::normalize(language);
    let mut text = full_text(id, language);
    for &key in CONTEXT {
        let value = args[key]
            .as_i64()
            .or_else(|| args[key].as_str().and_then(|s| s.parse::<i64>().ok()));
        let replacement = value.map_or_else(
            || locales::text(language, "prompt.notSelected").to_owned(),
            |v| v.to_string(),
        );
        text = text.replace(&format!("{{{{{key}}}}}"), &replacement);
    }
    Some(
        json!({"description": title(id, language), "messages": [{"role": "user", "content": {"type": "text", "text": text}}]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_share_localized_sources_and_history_context() {
        let tools = crate::tools::definitions();
        let ui = for_ui();
        for id in ids() {
            let entry = ui
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == id)
                .unwrap();
            for (language, _) in locales::languages() {
                let context = json!({"session_id":"42", "vehicle_id":"7"});
                let result = mcp_get(id, language, &context).unwrap();
                let text = result["messages"][0]["content"]["text"].as_str().unwrap();
                assert_eq!(result["description"], entry["title"][language]);
                assert!(!title(id, language).is_empty() && !body(id, language).is_empty());
                assert!(text.ends_with(locales::text(language, "prompt.safety")));
                assert!(!text.contains("{{"), "{id}: {language}");
                let template = entry["text"][language].as_str().unwrap();
                if language != "en" {
                    assert_ne!(
                        template,
                        entry["text"]["en"].as_str().unwrap(),
                        "{id}: {language}"
                    );
                }
                if HISTORY.contains(&id) {
                    assert!(text.contains("session_id=42") && text.contains("vehicle_id=7"));
                    assert!(text.contains(locales::text(language, "prompt.notSelected")));
                    for key in CONTEXT {
                        assert!(
                            template.contains(&format!("{{{{{key}}}}}")),
                            "{id}: {key} in {language}"
                        );
                    }
                    for tool in [
                        "get_session",
                        "query_session",
                        "get_session_events",
                        "list_sessions",
                    ] {
                        assert!(
                            text.contains(&format!("`{tool}`"))
                                && tools.iter().any(|d| d["name"] == tool)
                        );
                    }
                }
            }
        }
    }
}
