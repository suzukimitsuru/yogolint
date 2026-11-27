//! 設定ファイル `yogolint.toml` の内容。項目名は全て kebab-case(規則名と揃える)。

use std::fmt;

use serde::de::{self, IntoDeserializer, MapAccess, Visitor};
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
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct Config {
    pub languages: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub kinds: Vec<Kind>,
    /// 埋め込み辞書のうち使う層
    pub builtin_dictionaries: Vec<Layer>,
    /// 追加の辞書 TOML(設定ファイルからの相対パス)
    pub dictionaries: Vec<String>,
    /// 規則ごとの設定(重大度と、品詞の規則が認める品詞)
    pub rules: Rules,
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
            rules: Rules::default(),
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

/// `[rules]`: 規則ごとの設定。省いた規則は既定の重大度で報告する。
///
/// - 重大度だけなら `unknown-word = "error"` と1行で書く。`none` で報告しない。
/// - 品詞の規則は表にして、重大度(`severity`)と認める品詞(`allow`)を書ける。
/// - denied-word・denied-pattern・avoided-word は既定では辞書の項目ごとの重大度を使い、
///   ここで指定すると全ての項目の重大度を上書きする。
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case", default, deny_unknown_fields)]
pub struct Rules {
    pub denied_word: Option<Severity>,
    pub denied_pattern: Option<Severity>,
    pub avoided_word: Option<Severity>,
    pub unknown_word: Option<Severity>,
    pub compound_word: Option<Severity>,
    pub short_word: Option<Severity>,
    pub function_first_word_part_of_speech: PartOfSpeechRule,
    pub type_last_word_part_of_speech: PartOfSpeechRule,
    pub value_last_word_part_of_speech: PartOfSpeechRule,
    pub constant_last_word_part_of_speech: PartOfSpeechRule,
    pub function_noun_verb_order: Option<Severity>,
    pub non_ascii_identifier: Option<Severity>,
    pub syntax_error: Option<Severity>,
}

impl Rules {
    /// 設定で指定した重大度。指定が無ければ None
    pub fn configured(&self, rule: Rule) -> Option<Severity> {
        match rule {
            Rule::DeniedWord => self.denied_word,
            Rule::DeniedPattern => self.denied_pattern,
            Rule::AvoidedWord => self.avoided_word,
            Rule::UnknownWord => self.unknown_word,
            Rule::CompoundWord => self.compound_word,
            Rule::ShortWord => self.short_word,
            Rule::FunctionFirstWordPartOfSpeech => self.function_first_word_part_of_speech.severity,
            Rule::TypeLastWordPartOfSpeech => self.type_last_word_part_of_speech.severity,
            Rule::ValueLastWordPartOfSpeech => self.value_last_word_part_of_speech.severity,
            Rule::ConstantLastWordPartOfSpeech => self.constant_last_word_part_of_speech.severity,
            Rule::FunctionNounVerbOrder => self.function_noun_verb_order,
            Rule::NonAsciiIdentifier => self.non_ascii_identifier,
            Rule::SyntaxError => self.syntax_error,
        }
    }

    /// 規則の重大度。設定 → 規則の既定 → `fallback`(辞書の項目の重大度)の順に決める
    pub fn severity(&self, rule: Rule, fallback: Severity) -> Severity {
        self.configured(rule)
            .or_else(|| rule.default_severity())
            .unwrap_or(fallback)
    }

    /// 品詞の規則で、設定が認める品詞。指定が無ければ None(言語の慣習に従う)
    pub fn allowed_parts_of_speech(&self, rule: Rule) -> Option<&[String]> {
        let setting = match rule {
            Rule::FunctionFirstWordPartOfSpeech => &self.function_first_word_part_of_speech,
            Rule::TypeLastWordPartOfSpeech => &self.type_last_word_part_of_speech,
            Rule::ValueLastWordPartOfSpeech => &self.value_last_word_part_of_speech,
            Rule::ConstantLastWordPartOfSpeech => &self.constant_last_word_part_of_speech,
            _ => return None,
        };
        setting.allow.as_deref()
    }
}

/// 品詞の規則の設定。`"info"` のような重大度か、`{ severity = "info", allow = ["noun"] }` の表で書く。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartOfSpeechRule {
    pub severity: Option<Severity>,
    /// 認める品詞
    pub allow: Option<Vec<String>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PartOfSpeechTable {
    severity: Option<Severity>,
    #[serde(default, deserialize_with = "part_of_speech_list")]
    allow: Option<Vec<String>>,
}

impl<'de> Deserialize<'de> for PartOfSpeechRule {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct RuleVisitor;
        impl<'de> Visitor<'de> for RuleVisitor {
            type Value = PartOfSpeechRule;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("重大度(\"info\" など)か、severity・allow を持つ表")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<PartOfSpeechRule, E> {
                Ok(PartOfSpeechRule {
                    severity: Some(Severity::deserialize(value.into_deserializer())?),
                    allow: None,
                })
            }

            fn visit_map<M: MapAccess<'de>>(self, map: M) -> Result<PartOfSpeechRule, M::Error> {
                let table = PartOfSpeechTable::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(PartOfSpeechRule {
                    severity: table.severity,
                    allow: table.allow,
                })
            }
        }
        deserializer.deserialize_any(RuleVisitor)
    }
}

/// 品詞の一覧を読み、知らない品詞名を誤りにする
fn part_of_speech_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Vec<String>>, D::Error> {
    let list = Vec::<String>::deserialize(deserializer)?;
    if let Some(unknown) = list.iter().find(|pos| !PARTS_OF_SPEECH.contains(&pos.as_str())) {
        return Err(de::Error::custom(format!(
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
builtin-dictionaries = ["verbs", "nouns", "abbrev", "deny"]
dictionaries = ["dicts/domain.toml"]
allow-compound-words = false
min-word-length = 2
allow-single-letter = ["i", "j", "k", "n", "x", "y"]
ignore-words = []
baseline = ".yogolint-baseline.json"

[rules]
unknown-word = "error"
function-noun-verb-order = "none"
type-last-word-part-of-speech = "warning"

[rules.function-first-word-part-of-speech]
allow = ["verb", "prefix"]

[rules.value-last-word-part-of-speech]
severity = "warning"
allow = ["noun"]
"#,
        )
        .unwrap();
        let rules = &config.rules;
        assert_eq!(config.dictionaries, ["dicts/domain.toml"]);
        assert_eq!(rules.severity(Rule::UnknownWord, Severity::None), Severity::Error);
        assert_eq!(
            rules.severity(Rule::FunctionNounVerbOrder, Severity::Error),
            Severity::None
        );
        assert_eq!(
            rules.severity(Rule::TypeLastWordPartOfSpeech, Severity::None),
            Severity::Warning
        );
        // 表で重大度を省けば既定の重大度
        assert_eq!(
            rules.severity(Rule::FunctionFirstWordPartOfSpeech, Severity::None),
            Severity::Warning
        );
        assert_eq!(
            rules
                .allowed_parts_of_speech(Rule::FunctionFirstWordPartOfSpeech)
                .unwrap(),
            ["verb", "prefix"]
        );
        assert_eq!(rules.allowed_parts_of_speech(Rule::TypeLastWordPartOfSpeech), None);
        // 辞書で決まる規則は、指定が無ければ辞書の項目の重大度
        assert_eq!(rules.severity(Rule::DeniedWord, Severity::Error), Severity::Error);
    }

    #[test]
    fn rejects_unknown_rules_and_parts_of_speech() {
        assert!(Config::from_toml("[rules]\nposition = \"none\"").is_err());
        assert!(Config::from_toml("[rules]\nunknown = \"none\"").is_err());
        assert!(Config::from_toml("[rules.value-last-word-part-of-speech]\nallow = [\"verbs\"]").is_err());
        assert!(Config::from_toml("[rules.value-last-word-part-of-speech]\nallowed = [\"noun\"]").is_err());
        // allow を持てるのは品詞の規則だけ
        assert!(Config::from_toml("[rules.unknown-word]\nseverity = \"info\"").is_err());
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(Config::from_toml("allowCompoundWords = true").is_err());
        assert!(Config::from_toml("unknwon-word = \"error\"").is_err());
    }
}
