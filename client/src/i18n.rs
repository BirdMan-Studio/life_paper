use crate::config::Language;
use std::{collections::HashMap, sync::OnceLock};

const ZH_CN: &str = include_str!("..\\i18n\\zh_CN.toml");
const EN_US: &str = include_str!("..\\i18n\\en_US.toml");

struct Catalogs {
    zh_cn: HashMap<String, String>,
    en_us: HashMap<String, String>,
}

static CATALOGS: OnceLock<Catalogs> = OnceLock::new();

pub fn text(language: Language, key: &'static str) -> &'static str {
    let catalogs = CATALOGS.get_or_init(|| Catalogs {
        zh_cn: parse_catalog(ZH_CN, "zh_CN"),
        en_us: parse_catalog(EN_US, "en_US"),
    });

    let selected = match language {
        Language::ZhCn => &catalogs.zh_cn,
        Language::EnUs => &catalogs.en_us,
    };

    selected
        .get(key)
        .or_else(|| catalogs.zh_cn.get(key))
        .map_or(key, String::as_str)
}

fn parse_catalog(content: &str, locale: &str) -> HashMap<String, String> {
    toml::from_str(content)
        .unwrap_or_else(|error| panic!("invalid embedded locale {locale}: {error}"))
}
