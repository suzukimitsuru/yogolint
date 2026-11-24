//! 用語辞書(TOML)の読み込みと、照合用の索引。

use std::collections::HashMap;
use std::fmt;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{Kind, Severity};

/// 辞書の層。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    Domain,
    Verbs,
    Nouns,
    Abbrev,
    Deny,
}

impl Layer {
    pub const ALL: [Layer; 5] = [Layer::Domain, Layer::Verbs, Layer::Nouns, Layer::Abbrev, Layer::Deny];

    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Domain => "domain",
            Layer::Verbs => "verbs",
            Layer::Nouns => "nouns",
            Layer::Abbrev => "abbrev",
            Layer::Deny => "deny",
        }
    }

    pub fn parse(text: &str) -> Option<Layer> {
        Layer::ALL.into_iter().find(|layer| layer.as_str() == text)
    }

    /// 書き出すときの既定のファイル名
    pub fn file_name(self) -> &'static str {
        match self {
            Layer::Domain => "domain.toml",
            Layer::Verbs => "general-verbs.toml",
            Layer::Nouns => "general-nouns.toml",
            Layer::Abbrev => "abbrev.toml",
            Layer::Deny => "deny.toml",
        }
    }

    /// 許可層か
    pub fn is_allow(self) -> bool {
        self != Layer::Deny
    }
}

/// バイナリに埋め込んだ標準辞書(domain 層は持たない)。
pub const BUILTIN: [(Layer, &str); 4] = [
    (Layer::Verbs, include_str!("../dicts/general-verbs.toml")),
    (Layer::Nouns, include_str!("../dicts/general-nouns.toml")),
    (Layer::Abbrev, include_str!("../dicts/abbrev.toml")),
    (Layer::Deny, include_str!("../dicts/deny.toml")),
];

pub fn builtin_text(layer: Layer) -> Option<&'static str> {
    BUILTIN.iter().find(|(l, _)| *l == layer).map(|(_, text)| *text)
}

/// 品詞は1つでも配列でもよい。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(untagged)]
pub enum PartOfSpeech {
    #[default]
    Unspecified,
    One(String),
    Many(Vec<String>),
}

impl PartOfSpeech {
    fn to_vec(&self) -> Vec<String> {
        match self {
            PartOfSpeech::Unspecified => vec![],
            PartOfSpeech::One(pos) => vec![pos.clone()],
            PartOfSpeech::Many(list) => list.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Any,
    First,
    Last,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Term {
    pub word: String,
    #[serde(default)]
    pub pos: PartOfSpeech,
    pub definition: Option<String>,
    pub ja: Option<String>,
    #[serde(default)]
    pub pair: Vec<String>,
    #[serde(default)]
    pub avoid: Vec<String>,
    /// avoid の語を使ったときの重大度
    pub severity: Option<Severity>,
    #[serde(default)]
    pub forms: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DenyEntry {
    pub word: String,
    #[serde(default)]
    pub pos: PartOfSpeech,
    pub reason: Option<String>,
    pub suggest: Option<String>,
    #[serde(default)]
    pub avoid: Vec<String>,
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub kinds: Vec<Kind>,
    pub severity: Option<Severity>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternEntry {
    pub regex: String,
    pub reason: Option<String>,
    pub suggest: Option<String>,
    #[serde(default)]
    pub kinds: Vec<Kind>,
    pub severity: Option<Severity>,
}

/// 辞書 TOML 1ファイルの内容。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DictFile {
    pub version: u32,
    pub layer: Layer,
    #[serde(default)]
    pub terms: Vec<Term>,
    #[serde(default)]
    pub deny: Vec<DenyEntry>,
    #[serde(default)]
    pub patterns: Vec<PatternEntry>,
}

#[derive(Debug, Clone)]
pub struct DictError {
    pub source_name: String,
    pub message: String,
}

impl fmt::Display for DictError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.source_name, self.message)
    }
}

impl std::error::Error for DictError {}

impl DictFile {
    pub fn parse(source_name: &str, text: &str) -> Result<DictFile, DictError> {
        let file: DictFile = toml::from_str(text).map_err(|error| DictError {
            source_name: source_name.to_string(),
            message: error.to_string(),
        })?;
        if file.version != 1 {
            return Err(DictError {
                source_name: source_name.to_string(),
                message: format!("unsupported dictionary version {}", file.version),
            });
        }
        Ok(file)
    }
}

/// 許可された語
#[derive(Debug, Clone)]
pub struct AllowedWord {
    /// 原形(forms から引いたときは登録した語)
    pub base: String,
    pub pos: Vec<String>,
    pub layer: Layer,
}

/// 避ける同義語・展開すべき略語
#[derive(Debug, Clone)]
pub struct AvoidWord {
    pub canonical: String,
    pub canonical_pos: Vec<String>,
    pub severity: Severity,
    pub layer: Layer,
}

/// 禁止語
#[derive(Debug, Clone)]
pub struct DeniedWord {
    /// 辞書に書いた禁止語(異表記で引いたときもこちら)
    pub word: String,
    pub reason: Option<String>,
    pub suggest: Option<String>,
    pub position: Position,
    pub kinds: Vec<Kind>,
    pub severity: Severity,
}

#[derive(Debug, Clone)]
pub struct DeniedPattern {
    pub regex: Regex,
    pub reason: Option<String>,
    pub suggest: Option<String>,
    pub kinds: Vec<Kind>,
    pub severity: Severity,
}

/// 全層を合わせた照合用の索引。
#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    pub allowed: HashMap<String, AllowedWord>,
    pub avoid: HashMap<String, AvoidWord>,
    pub denied: HashMap<String, DeniedWord>,
    pub patterns: Vec<DeniedPattern>,
    /// 数字を含む語(`utf8` など)。長い順
    pub digit_words: Vec<String>,
}

impl Dictionary {
    /// 埋め込み辞書の `builtin` 層と、利用者の辞書ファイルから索引を作る。
    /// 利用者の辞書に同じ層があれば、その層の埋め込み辞書は使わない。
    pub fn load(builtin: &[Layer], user_files: &[DictFile]) -> Result<Dictionary, DictError> {
        let mut files: Vec<DictFile> = Vec::new();
        for (layer, text) in BUILTIN {
            if builtin.contains(&layer) && !user_files.iter().any(|file| file.layer == layer) {
                files.push(DictFile::parse(&format!("builtin:{}", layer.as_str()), text)?);
            }
        }
        files.extend(user_files.iter().cloned());
        Dictionary::from_files(&files)
    }

    pub fn from_files(files: &[DictFile]) -> Result<Dictionary, DictError> {
        let mut dictionary = Dictionary::default();
        // domain の品詞を先に入れ、他の層は品詞を足すだけにする
        let mut ordered: Vec<&DictFile> = files.iter().collect();
        ordered.sort_by_key(|file| file.layer != Layer::Domain);
        for file in ordered {
            dictionary.add_file(file)?;
        }
        let mut digit_words: Vec<String> = dictionary
            .allowed
            .keys()
            .chain(dictionary.denied.keys())
            .filter(|word| word.bytes().any(|byte| byte.is_ascii_digit()))
            .cloned()
            .collect();
        digit_words.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
        digit_words.dedup();
        dictionary.digit_words = digit_words;
        Ok(dictionary)
    }

    fn add_file(&mut self, file: &DictFile) -> Result<(), DictError> {
        for term in &file.terms {
            let word = term.word.to_ascii_lowercase();
            let pos = term.pos.to_vec();
            self.add_allowed(&word, &word, &pos, file.layer);
            for form in &term.forms {
                self.add_allowed(&form.to_ascii_lowercase(), &word, &pos, file.layer);
            }
            for avoid in &term.avoid {
                self.avoid.entry(avoid.to_ascii_lowercase()).or_insert(AvoidWord {
                    canonical: word.clone(),
                    canonical_pos: pos.clone(),
                    severity: term.severity.unwrap_or(Severity::Warning),
                    layer: file.layer,
                });
            }
        }
        for entry in &file.deny {
            let denied = DeniedWord {
                word: entry.word.to_ascii_lowercase(),
                reason: entry.reason.clone(),
                suggest: entry.suggest.clone(),
                position: entry.position,
                kinds: entry.kinds.clone(),
                severity: entry.severity.unwrap_or(Severity::Warning),
            };
            for alias in &entry.avoid {
                self.denied
                    .entry(alias.to_ascii_lowercase())
                    .or_insert_with(|| denied.clone());
            }
            self.denied.insert(denied.word.clone(), denied);
        }
        for entry in &file.patterns {
            let regex = Regex::new(&entry.regex).map_err(|error| DictError {
                source_name: format!("layer {}", file.layer.as_str()),
                message: format!("invalid regex {:?}: {error}", entry.regex),
            })?;
            self.patterns.push(DeniedPattern {
                regex,
                reason: entry.reason.clone(),
                suggest: entry.suggest.clone(),
                kinds: entry.kinds.clone(),
                severity: entry.severity.unwrap_or(Severity::Warning),
            });
        }
        Ok(())
    }

    fn add_allowed(&mut self, word: &str, base: &str, pos: &[String], layer: Layer) {
        let entry = self.allowed.entry(word.to_string()).or_insert_with(|| AllowedWord {
            base: base.to_string(),
            pos: vec![],
            layer,
        });
        for p in pos {
            if !entry.pos.contains(p) {
                entry.pos.push(p.clone());
            }
        }
    }

    pub fn is_allowed(&self, word: &str) -> bool {
        self.allowed.contains_key(word)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_dictionaries_parse() {
        let dictionary = Dictionary::load(&[Layer::Verbs, Layer::Nouns, Layer::Abbrev, Layer::Deny], &[]).unwrap();
        assert!(dictionary.is_allowed("get"));
        assert_eq!(dictionary.avoid["retrieve"].canonical, "get");
        assert_eq!(dictionary.denied["mgr"].word, "manager");
        assert!(dictionary.digit_words.contains(&"utf8".to_string()));
    }

    #[test]
    fn user_layer_replaces_builtin_layer() {
        let verbs = DictFile::parse(
            "test",
            "version = 1\nlayer = \"verbs\"\n[[terms]]\nword = \"frobnicate\"\npos = \"verb\"\n",
        )
        .unwrap();
        let dictionary = Dictionary::load(&[Layer::Verbs], &[verbs]).unwrap();
        assert!(dictionary.is_allowed("frobnicate"));
        assert!(!dictionary.is_allowed("get"));
    }

    #[test]
    fn forms_point_to_base() {
        let file = DictFile::parse(
            "test",
            "version = 1\nlayer = \"domain\"\n[[terms]]\nword = \"order\"\npos = \"noun\"\nforms = [\"orders\"]\n",
        )
        .unwrap();
        let dictionary = Dictionary::from_files(&[file]).unwrap();
        assert_eq!(dictionary.allowed["orders"].base, "order");
    }
}
