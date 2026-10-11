use std::{collections::HashMap, sync::LazyLock};
static CATALOGS: LazyLock<HashMap<&'static str, HashMap<String, String>>> = LazyLock::new(|| {
    [
        ("en", include_str!("../../src/locales/en.json")),
        ("ru", include_str!("../../src/locales/ru.json")),
        ("bg", include_str!("../../src/locales/bg.json")),
        ("cs", include_str!("../../src/locales/cs.json")),
        ("da", include_str!("../../src/locales/da.json")),
        ("de", include_str!("../../src/locales/de.json")),
        ("el", include_str!("../../src/locales/el.json")),
        ("es", include_str!("../../src/locales/es.json")),
        ("et", include_str!("../../src/locales/et.json")),
        ("fi", include_str!("../../src/locales/fi.json")),
        ("fr", include_str!("../../src/locales/fr.json")),
        ("ga", include_str!("../../src/locales/ga.json")),
        ("hr", include_str!("../../src/locales/hr.json")),
        ("hu", include_str!("../../src/locales/hu.json")),
        ("it", include_str!("../../src/locales/it.json")),
        ("lt", include_str!("../../src/locales/lt.json")),
        ("lv", include_str!("../../src/locales/lv.json")),
        ("mt", include_str!("../../src/locales/mt.json")),
        ("nl", include_str!("../../src/locales/nl.json")),
        ("pl", include_str!("../../src/locales/pl.json")),
        ("pt", include_str!("../../src/locales/pt.json")),
        ("ro", include_str!("../../src/locales/ro.json")),
        ("sk", include_str!("../../src/locales/sk.json")),
        ("sl", include_str!("../../src/locales/sl.json")),
        ("sv", include_str!("../../src/locales/sv.json")),
    ]
    .into_iter()
    .map(|(code, json)| {
        (
            code,
            serde_json::from_str(json).expect("valid locale catalog"),
        )
    })
    .collect()
});
pub fn valid_language(value: &str) -> bool {
    value == "system" || CATALOGS.contains_key(value)
}
pub fn default_language() -> String {
    "system".into()
}
pub fn system_language() -> String {
    #[cfg(windows)]
    {
        let mut locale = [0u16; 85];
        // SAFETY: the buffer holds LOCALE_NAME_MAX_LENGTH UTF-16 code units.
        let length = unsafe {
            windows_sys::Win32::Globalization::GetUserDefaultLocaleName(
                locale.as_mut_ptr(),
                locale.len() as i32,
            )
        };
        if length > 1 {
            let tag = String::from_utf16_lossy(&locale[..length as usize - 1]).to_lowercase();
            let base = tag.split(['-', '_']).next().unwrap_or("en");
            if CATALOGS.contains_key(base) {
                return base.to_string();
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        for language in objc2_foundation::NSLocale::preferredLanguages().iter() {
            let tag = language.to_string().to_lowercase();
            let base = tag.split(['-', '_']).next().unwrap_or("en");
            if CATALOGS.contains_key(base) {
                return base.to_string();
            }
        }
    }
    "en".into()
}
pub fn t(language: &str, key: &str, params: &[(&str, &str)]) -> String {
    let resolved = if language == "system" {
        system_language()
    } else {
        language.into()
    };
    let mut result = CATALOGS
        .get(resolved.as_str())
        .and_then(|c| c.get(key))
        .or_else(|| CATALOGS["en"].get(key))
        .cloned()
        .unwrap_or_else(|| key.into());
    for (name, value) in params {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_catalogs_match_english() {
        let english = &CATALOGS["en"];
        assert_eq!(CATALOGS.len(), 25);
        for catalog in CATALOGS.values() {
            assert_eq!(catalog.len(), english.len());
            for key in english.keys() {
                assert!(!catalog.get(key).expect("missing key").is_empty());
            }
        }
        assert_eq!(
            t("ru", "connect", &[("provider", "Claude")]),
            "Подключить Claude"
        );
        assert_eq!(t("unknown", "settings", &[]), "Settings");
    }
}
