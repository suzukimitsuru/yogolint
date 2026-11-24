//! 設定ファイル `yogolint.toml` の内容。

use serde::Deserialize;

use crate::{Kind, Layer, Severity};

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
    pub unknown_word: Severity,
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
            unknown_word: Severity::Warning,
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
unknownWord = "warning"
allowCompoundWords = false
minWordLength = 2
allowSingleLetter = ["i", "j", "k", "n", "x", "y"]
ignoreWords = []
baseline = ".yogolint-baseline.json"
"#,
        )
        .unwrap();
        assert_eq!(config.dictionaries, ["dicts/domain.toml"]);
        assert_eq!(config.unknown_word, Severity::Warning);
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(Config::from_toml("unknwonWord = \"error\"").is_err());
    }
}
