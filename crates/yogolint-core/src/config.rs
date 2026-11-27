//! 設定ファイル `yogolint.toml` の内容。

use serde::{Deserialize, Deserializer};

use crate::{Kind, Layer, Rule, Severity};

/// 品詞の名前(辞書の `pos` と同じ)
pub const PARTS_OF_SPEECH: [&str; 8] = [
    "verb",
    "prefix",
    "noun",
    "adjective",
    "adverb",
    "preposition",
    "determiner",
    "conjunction",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Config {
    pub languages: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub kinds: Vec<Kind>,
    /// 埋め込み辞書のうち使う層
    pub builtin_dictionaries: Vec<Layer>,
    /// 追加の辞書 TOML(設定ファイルからの相対パス)
    pub dictionaries: Vec<String>,
    /// 規則ごとの重大度
    pub rules: RuleSeverities,
    /// 品詞位置の規則が認める品詞。省いた項目は言語の慣習に従う
    pub part_of_speech: PartOfSpeech,
    pub allow_compound_words: bool,
    pub min_word_length: usize,
    pub allow_single_letter: Vec<String>,
    pub ignore_words: Vec<String>,
    pub ignore_paths: Vec<String>,
    pub baseline: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            languages: vec!["rust".into()],
            include: vec![],
            exclude: vec![],
            kinds: vec![Kind::Function, Kind::Type, Kind::Value, Kind::Constant],
            builtin_dictionaries: vec![Layer::Verbs, Layer::Nouns, Layer::Abbrev, Layer::Deny],
            dictionaries: vec![],
            rules: RuleSeverities::default(),
            part_of_speech: PartOfSpeech::default(),
            allow_compound_words: false,
            min_word_length: 2,
            allow_single_letter: ["i", "j", "k", "n", "x", "y"].map(String::from).to_vec(),
            ignore_words: vec![],
            ignore_paths: vec![],
            baseline: None,
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Config, toml::de::Error> {
        toml::from_str(text)
    }
}

/// `[rules]`: 規則ごとの重大度。`none` で報告しない。
/// deny・avoid は辞書の項目ごとに重大度を持つので、ここでは指定しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct RuleSeverities {
    pub unknown: Severity,
    pub compound: Severity,
    pub short: Severity,
    pub function_head: Severity,
    pub name_ending: Severity,
    pub noun_verb_order: Severity,
    pub non_ascii: Severity,
    pub syntax: Severity,
}

impl Default for RuleSeverities {
    fn default() -> Self {
        RuleSeverities {
            unknown: Severity::Warning,
            compound: Severity::Warning,
            short: Severity::Warning,
            function_head: Severity::Warning,
            name_ending: Severity::Info,
            noun_verb_order: Severity::Warning,
            non_ascii: Severity::Info,
            syntax: Severity::Info,
        }
    }
}

impl RuleSeverities {
    /// 規則の重大度。deny・avoid は辞書の項目で決まるので None
    pub fn get(&self, rule: Rule) -> Option<Severity> {
        match rule {
            Rule::Deny | Rule::Avoid => None,
            Rule::Unknown => Some(self.unknown),
            Rule::Compound => Some(self.compound),
            Rule::Short => Some(self.short),
            Rule::FunctionHead => Some(self.function_head),
            Rule::NameEnding => Some(self.name_ending),
            Rule::NounVerbOrder => Some(self.noun_verb_order),
            Rule::NonAscii => Some(self.non_ascii),
            Rule::Syntax => Some(self.syntax),
        }
    }
}

/// `[partOfSpeech]`: 品詞位置の規則が認める品詞。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PartOfSpeech {
    /// function種別の先頭語(規則 function-head)
    #[serde(deserialize_with = "pos_list")]
    pub function_head: Option<Vec<String>>,
    /// type種別の末尾語(規則 name-ending)
    #[serde(deserialize_with = "pos_list")]
    pub type_ending: Option<Vec<String>>,
    /// value種別の末尾語(規則 name-ending)
    #[serde(deserialize_with = "pos_list")]
    pub value_ending: Option<Vec<String>>,
    /// constant種別の末尾語(規則 name-ending)
    #[serde(deserialize_with = "pos_list")]
    pub constant_ending: Option<Vec<String>>,
}

/// 品詞の一覧を読み、知らない品詞名を誤りにする
fn pos_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<String>>, D::Error> {
    let list = Vec::<String>::deserialize(deserializer)?;
    if let Some(unknown) = list.iter().find(|pos| !PARTS_OF_SPEECH.contains(&pos.as_str())) {
        return Err(serde::de::Error::custom(format!(
            "品詞 {unknown} は無い({} のいずれか)",
            PARTS_OF_SPEECH.join(" / ")
        )));
    }
    Ok(Some(list))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_example() {
        let config = Config::from_toml(
            r#"
languages = ["rust"]
include = ["src/**"]
exclude = ["**/generated/**"]
kinds = ["function", "type", "value", "constant"]
builtinDictionaries = ["verbs", "nouns", "abbrev", "deny"]
dictionaries = ["dicts/domain.toml"]
allowCompoundWords = false
minWordLength = 2
allowSingleLetter = ["i", "j", "k", "n", "x", "y"]
ignoreWords = []
baseline = ".yogolint-baseline.json"

[rules]
unknown = "error"
noun-verb-order = "none"

[partOfSpeech]
functionHead = ["verb", "prefix"]
valueEnding = ["noun"]
"#,
        )
        .unwrap();
        assert_eq!(config.dictionaries, ["dicts/domain.toml"]);
        assert_eq!(config.rules.unknown, Severity::Error);
        assert_eq!(config.rules.noun_verb_order, Severity::None);
        assert_eq!(config.rules.name_ending, Severity::Info);
        assert_eq!(config.part_of_speech.function_head.unwrap(), ["verb", "prefix"]);
        assert_eq!(config.part_of_speech.type_ending, None);
    }

    #[test]
    fn rejects_unknown_rules_and_parts_of_speech() {
        assert!(Config::from_toml("[rules]\ndeny = \"none\"").is_err());
        assert!(Config::from_toml("[rules]\nposition = \"none\"").is_err());
        assert!(Config::from_toml("[partOfSpeech]\nfunctionHead = [\"verbs\"]").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(Config::from_toml("unknwonWord = \"error\"").is_err());
    }
}
