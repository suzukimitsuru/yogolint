//! 照合: 1語ごとに 禁止 → 避ける同義語 → 許可層 → 連結語 → 未知語 の順に判定する。

use std::collections::HashMap;

use crate::dict::{Dictionary, Position};
use crate::extract::{Definition, Extractor, NamingConvention};
use crate::split::{Word, decompose_compound, split_identifier, trim_identifier};
use crate::{Config, Finding, Kind, Language, Location, Rule, Severity};

/// 抑止コメント
const DISABLE_NEXT_LINE: &str = "yogolint-disable-next-line";

/// 設定と辞書を持ち、ソースを検査する。
pub struct Linter {
    config: Config,
    dictionary: Dictionary,
    extractors: HashMap<Language, Extractor>,
}

/// 1語の照合結果
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// 許可。原形と品詞を持つ
    Allowed { pos: Vec<String> },
    Report {
        rule: Rule,
        severity: Severity,
        message: String,
        suggestions: Vec<String>,
    },
}

impl Linter {
    pub fn new(config: Config, dictionary: Dictionary) -> Linter {
        Linter {
            config,
            dictionary,
            extractors: HashMap::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn dictionary(&self) -> &Dictionary {
        &self.dictionary
    }

    /// ソース1つを検査する。重大度 none の報告も含めて返す(呼び出し側で除く)。
    pub fn check_source(&mut self, language: Language, source: &str) -> Result<Vec<Finding>, String> {
        let extractor = match self.extractors.entry(language) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => entry.insert(Extractor::new(language)?),
        };
        let extraction = extractor.extract(source);
        let line_starts = line_starts(source);
        let disabled = disabled_lines(source, language.line_comment());
        let mut findings = Vec::new();
        if extraction.skipped_in_error > 0 {
            let (row, column_byte) = extraction.first_error.unwrap_or((0, 0));
            let start_byte = line_starts.get(row).copied().unwrap_or(0) + column_byte;
            findings.push(Finding {
                location: location(source, &line_starts, start_byte, start_byte),
                identifier: String::new(),
                kind: None,
                word: String::new(),
                rule: Rule::Syntax,
                severity: Severity::Info,
                message: format!("構文エラー内の定義 {} 件を検査しなかった", extraction.skipped_in_error),
                suggestions: vec![],
            });
        }
        for definition in &extraction.definitions {
            if !self.config.kinds.contains(&definition.kind) {
                continue;
            }
            for finding in self.check_definition(definition, language.naming()) {
                let start_byte = definition.start_byte + finding.start;
                let end_byte = definition.start_byte + finding.end;
                let suppressed = disabled.get(&definition.row).is_some_and(|words| {
                    words.is_empty() || words.iter().any(|word| *word == finding.word || *word == finding.base)
                });
                if suppressed {
                    continue;
                }
                findings.push(Finding {
                    location: location(source, &line_starts, start_byte, end_byte),
                    identifier: definition.name.clone(),
                    kind: Some(definition.kind),
                    word: finding.word,
                    rule: finding.rule,
                    severity: finding.severity,
                    message: finding.message,
                    suggestions: finding.suggestions,
                });
            }
        }
        Ok(findings)
    }

    /// 識別子1つを検査する(ファイルに依らない)。範囲は識別子の中のバイト位置。
    fn check_definition(&self, definition: &Definition, naming: NamingConvention) -> Vec<WordFinding> {
        let identifier = definition.name.as_str();
        let kind = definition.kind;
        let (trim_start, trim_end) = trim_identifier(identifier);
        let whole = |rule, severity, message: String, suggestions| WordFinding {
            start: trim_start,
            end: trim_end,
            word: identifier.to_string(),
            base: identifier.to_string(),
            rule,
            severity,
            message,
            suggestions,
        };
        if !identifier.is_ascii() {
            return vec![whole(
                Rule::NonAscii,
                Severity::Info,
                "英字以外を含む識別子は検査しない".into(),
                vec![],
            )];
        }
        let trimmed = identifier[trim_start..trim_end].to_ascii_lowercase();
        if trimmed.is_empty() || self.is_ignored(&trimmed) {
            return vec![];
        }

        // 禁止パターンは識別子全体に照合する
        if !self.dictionary.digit_words.contains(&trimmed)
            && let Some(pattern) = self.dictionary.patterns.iter().find(|pattern| {
                (pattern.kinds.is_empty() || pattern.kinds.contains(&kind)) && pattern.regex.is_match(&trimmed)
            })
        {
            let reason = pattern
                .reason
                .clone()
                .unwrap_or_else(|| format!("禁止パターン {}", pattern.regex));
            let suggestions = pattern.suggest.iter().cloned().collect();
            return vec![whole(Rule::Deny, pattern.severity, reason, suggestions)];
        }

        let words = split_identifier(identifier, &self.dictionary.digit_words);
        let last = words.len().saturating_sub(1);
        let mut findings = Vec::new();
        for (index, word) in words.iter().enumerate() {
            let position = WordPosition {
                first: index == 0,
                last: index == last,
            };
            let verdict = self.judge_word(&word.text, kind, position, naming);
            match verdict {
                Verdict::Report {
                    rule,
                    severity,
                    message,
                    suggestions,
                } => {
                    findings.push(WordFinding::new(word, rule, severity, message, suggestions));
                }
                // テスト関数の名前は文で書くので、品詞位置は検査しない
                Verdict::Allowed { .. } if definition.is_test => {}
                Verdict::Allowed { pos } => {
                    if let Some(finding) = self.check_part_of_speech(word, &pos, kind, position, &words, naming) {
                        findings.push(finding);
                    }
                }
            }
        }
        findings
    }

    fn is_ignored(&self, word: &str) -> bool {
        self.config
            .ignore_words
            .iter()
            .any(|ignored| ignored.eq_ignore_ascii_case(word))
    }

    /// 1語を判定する。
    fn judge_word(&self, text: &str, kind: Kind, position: WordPosition, naming: NamingConvention) -> Verdict {
        if self.is_ignored(text) {
            return Verdict::Allowed { pos: vec![] };
        }
        // 言語の慣習で関数名の先頭に置く語(Rust の new)
        if kind == Kind::Function && position.first && naming.function_head_words.contains(&text) {
            return Verdict::Allowed { pos: vec![] };
        }
        let candidates = base_forms(text, &self.dictionary);
        // 語そのものが許可層にあれば、原形化した形で禁止語・避ける同義語を引かない
        // (string → str、args → arg のような誤った原形を避ける)
        let lookups = if self.dictionary.allowed.contains_key(text) {
            &candidates[..1]
        } else {
            &candidates[..]
        };

        // 1. 禁止語
        for candidate in lookups {
            if let Some(denied) = self.dictionary.denied.get(candidate) {
                let position_ok = match denied.position {
                    Position::Any => true,
                    Position::First => position.first,
                    Position::Last => position.last,
                };
                let kind_ok = denied.kinds.is_empty() || denied.kinds.contains(&kind);
                if position_ok && kind_ok {
                    let reason = denied.reason.clone().unwrap_or_else(|| "禁止語".into());
                    let message = if denied.word == *candidate {
                        format!("{text}: {reason}")
                    } else {
                        format!("{text}: 禁止語 {} の異表記。{reason}", denied.word)
                    };
                    return Verdict::Report {
                        rule: Rule::Deny,
                        severity: denied.severity,
                        message,
                        suggestions: denied.suggest.iter().cloned().collect(),
                    };
                }
            }
        }

        // 2. 避ける同義語。動詞の同義語は、動詞の置き場所(関数の先頭語)でだけ検査する
        for candidate in lookups {
            if let Some(avoid) = self.dictionary.avoid.get(candidate) {
                let verb_like = avoid.canonical_pos.iter().any(|pos| pos == "verb" || pos == "prefix");
                if !verb_like || (kind == Kind::Function && position.first) {
                    return Verdict::Report {
                        rule: Rule::Avoid,
                        severity: avoid.severity,
                        message: format!("{text} → {} を使う", avoid.canonical),
                        suggestions: vec![avoid.canonical.clone()],
                    };
                }
            }
        }

        // 3. 許可層
        for candidate in &candidates {
            if let Some(allowed) = self.dictionary.allowed.get(candidate) {
                return Verdict::Allowed {
                    pos: allowed.pos.clone(),
                };
            }
        }

        // 短すぎる語
        if text.len() < self.config.min_word_length {
            if self.config.allow_single_letter.iter().any(|letter| letter == text) {
                return Verdict::Allowed { pos: vec![] };
            }
            return Verdict::Report {
                rule: Rule::Short,
                severity: Severity::Warning,
                message: format!("{text}: 短すぎて意図を表さない"),
                suggestions: vec![],
            };
        }

        // 4. 連結語
        let is_word = |part: &str| {
            base_forms(part, &self.dictionary)
                .iter()
                .any(|candidate| self.dictionary.allowed.contains_key(candidate))
        };
        if let Some(parts) = decompose_compound(text, is_word) {
            if self.config.allow_compound_words {
                let last_part = parts.last().map(String::as_str).unwrap_or(text);
                let pos = base_forms(last_part, &self.dictionary)
                    .iter()
                    .find_map(|candidate| self.dictionary.allowed.get(candidate))
                    .map(|allowed| allowed.pos.clone())
                    .unwrap_or_default();
                return Verdict::Allowed { pos };
            }
            let joined = parts.join("_");
            return Verdict::Report {
                rule: Rule::Compound,
                severity: Severity::Warning,
                message: format!("{text}: 連結語。区切って書く({})", parts.join(" / ")),
                suggestions: vec![joined],
            };
        }

        // 5. 未知語
        Verdict::Report {
            rule: Rule::Unknown,
            severity: self.config.unknown_word,
            message: format!("{text}: 未知語"),
            suggestions: vec![],
        }
    }

    /// 品詞位置の検査(関数の先頭語は動詞か述語接頭辞、型・値・定数の末尾語は名詞)。
    fn check_part_of_speech(
        &self,
        word: &Word,
        pos: &[String],
        kind: Kind,
        position: WordPosition,
        words: &[Word],
        naming: NamingConvention,
    ) -> Option<WordFinding> {
        // 品詞が分からない語(ignoreWords・1文字の許可語)は検査しない
        if pos.is_empty() {
            return None;
        }
        match kind {
            Kind::Function if position.first => {
                let ok = pos.iter().any(|p| naming.function_head_pos.contains(&p.as_str()));
                (!ok).then(|| {
                    WordFinding::new(
                        word,
                        Rule::Position,
                        Severity::Warning,
                        format!("{}: 関数名の先頭は{}にする", word.text, naming.function_head_hint),
                        vec![],
                    )
                })
            }
            Kind::Type | Kind::Value | Kind::Constant if position.last => {
                let ok = pos.iter().any(|p| p == "noun");
                // 1語だけの名前の真偽値(enabled など)も報告するが、重大度は info
                (!ok).then(|| {
                    let what = if words.len() == 1 { "名前" } else { "名前の末尾" };
                    WordFinding::new(
                        word,
                        Rule::Position,
                        Severity::Info,
                        format!("{}: {what}は名詞にする", word.text),
                        vec![],
                    )
                })
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct WordPosition {
    first: bool,
    last: bool,
}

/// 識別子の中の1語についての報告
struct WordFinding {
    start: usize,
    end: usize,
    word: String,
    /// 抑止コメントとの照合用(現状は word と同じ)
    base: String,
    rule: Rule,
    severity: Severity,
    message: String,
    suggestions: Vec<String>,
}

impl WordFinding {
    fn new(word: &Word, rule: Rule, severity: Severity, message: String, suggestions: Vec<String>) -> WordFinding {
        WordFinding {
            start: word.start,
            end: word.end,
            word: word.text.clone(),
            base: word.text.clone(),
            rule,
            severity,
            message,
            suggestions,
        }
    }
}

/// 原形の候補。語そのもの、forms に登録した原形、規則変化(-s / -es / -ies / -ed / -ing)を外した形の順。
pub fn base_forms(text: &str, dictionary: &Dictionary) -> Vec<String> {
    let mut candidates = vec![text.to_string()];
    let mut push = |candidate: String| {
        if candidate.len() >= 2 && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    };
    if let Some(allowed) = dictionary.allowed.get(text) {
        push(allowed.base.clone());
    }
    if let Some(stem) = text.strip_suffix("ies") {
        push(format!("{stem}y"));
    }
    if let Some(stem) = text.strip_suffix("es") {
        push(stem.to_string());
    }
    if let Some(stem) = text.strip_suffix('s')
        && !text.ends_with("ss")
    {
        push(stem.to_string());
    }
    for suffix in ["ed", "ing"] {
        if let Some(stem) = text.strip_suffix(suffix) {
            push(stem.to_string());
            push(format!("{stem}e"));
            // stopped → stop
            let bytes = stem.as_bytes();
            if bytes.len() >= 2 && bytes[bytes.len() - 1] == bytes[bytes.len() - 2] {
                push(stem[..stem.len() - 1].to_string());
            }
            // copied → copy
            if let Some(inner) = stem.strip_suffix('i') {
                push(format!("{inner}y"));
            }
        }
    }
    candidates
}

/// 行の開始バイト位置
fn line_starts(source: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(source.match_indices('\n').map(|(index, _)| index + 1))
        .collect()
}

fn location(source: &str, line_starts: &[usize], start_byte: usize, end_byte: usize) -> Location {
    let row = line_starts
        .partition_point(|&start| start <= start_byte)
        .saturating_sub(1);
    let line_start = line_starts[row];
    let column = source[line_start..start_byte].chars().count() + 1;
    Location {
        line: row + 1,
        column,
        start_byte,
        end_byte,
    }
}

/// 抑止コメントの次の行(0 始まり)→ 抑止する語(空なら全て)
fn disabled_lines(source: &str, line_comment: &str) -> HashMap<usize, Vec<String>> {
    let mut disabled = HashMap::new();
    for (row, line) in source.lines().enumerate() {
        let Some(comment_at) = line.find(line_comment) else {
            continue;
        };
        let comment = line[comment_at + line_comment.len()..].trim();
        if let Some(rest) = comment.strip_prefix(DISABLE_NEXT_LINE) {
            let words = rest
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|word| !word.is_empty())
                .map(|word| word.to_ascii_lowercase())
                .collect();
            disabled.insert(row + 1, words);
        }
    }
    disabled
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Layer;
    use crate::dict::DictFile;

    fn linter(config: Config) -> Linter {
        let domain = DictFile::parse(
            "domain",
            r#"
version = 1
layer = "domain"
[[terms]]
word = "order"
pos = "noun"
ja = "注文"
forms = ["orders"]
"#,
        )
        .unwrap();
        let dictionary = Dictionary::load(&config.builtin_dictionaries.clone(), &[domain]).unwrap();
        Linter::new(config, dictionary)
    }

    fn check(source: &str) -> Vec<(String, Rule, Severity)> {
        linter(Config::default())
            .check_source(Language::Rust, source)
            .unwrap()
            .into_iter()
            .map(|finding| (finding.word, finding.rule, finding.severity))
            .collect()
    }

    #[test]
    fn clean_code_has_no_findings() {
        assert_eq!(check("fn get_order(order_id: u64) -> u64 { order_id }"), vec![]);
        assert_eq!(check("fn is_empty() -> bool { true }"), vec![]);
        assert_eq!(check("const MAX_RETRY_COUNT: u32 = 3;"), vec![]);
    }

    #[test]
    fn reports_spec_examples() {
        assert_eq!(
            check("fn get_order_info() {}"),
            vec![("info".into(), Rule::Deny, Severity::Warning)]
        );
        assert_eq!(
            check("fn retrieve_orders() {}"),
            vec![("retrieve".into(), Rule::Avoid, Severity::Warning)]
        );
        assert_eq!(
            check("struct OrderMgr;"),
            vec![("mgr".into(), Rule::Deny, Severity::Warning)]
        );
        assert_eq!(
            check("struct ShukkaDate;"),
            vec![("shukka".into(), Rule::Unknown, Severity::Warning)]
        );
    }

    #[test]
    fn deny_position_and_kinds() {
        // manager は型名の末尾だけ
        assert_eq!(
            check("struct OrderManager;"),
            vec![("manager".into(), Rule::Deny, Severity::Warning)]
        );
        assert_eq!(
            check("fn get_manager_order() {}")
                .iter()
                .filter(|f| f.1 == Rule::Deny)
                .count(),
            0
        );
    }

    #[test]
    fn patterns_compound_and_short_words() {
        assert_eq!(
            check("fn load() { let order1 = 0; }"),
            vec![("order1".into(), Rule::Deny, Severity::Error)]
        );
        assert_eq!(
            check("fn savefile() {}"),
            vec![("savefile".into(), Rule::Compound, Severity::Warning)]
        );
        assert_eq!(check("fn load() { let i = 0; }"), vec![]);
        assert_eq!(
            check("fn load() { let q = 0; }"),
            vec![("q".into(), Rule::Short, Severity::Warning)]
        );
    }

    #[test]
    fn verb_synonyms_only_apply_to_function_heads() {
        // new は create の同義語だが、形容詞として使う変数名では報告しない
        assert_eq!(check("fn load() { let new_order = 0; }"), vec![]);
        assert_eq!(
            check("fn make_order() {}"),
            vec![("make".into(), Rule::Avoid, Severity::Warning)]
        );
        // 略語の展開は位置を問わない
        assert_eq!(
            check("fn load() { let order_cnt = 0; }"),
            vec![("cnt".into(), Rule::Avoid, Severity::Info)]
        );
    }

    #[test]
    fn part_of_speech_rules() {
        assert_eq!(
            check("fn only_price() {}"),
            vec![("only".into(), Rule::Position, Severity::Warning)]
        );
        assert_eq!(
            check("struct OrderEmpty;"),
            vec![("empty".into(), Rule::Position, Severity::Info)]
        );
    }

    #[test]
    fn inflections_and_forms() {
        assert_eq!(check("fn load() { let orders = 0; let updated_values = 0; }"), vec![]);
    }

    #[test]
    fn allowed_words_are_not_matched_by_their_stems() {
        // args → arg(避ける同義語)、string → str(禁止語)と原形化しない
        assert_eq!(check("fn load(args: u8, string_value: u8) {}"), vec![]);
        // 語そのものが許可層に無ければ、原形化した形で引く
        assert_eq!(
            check("fn load() { let order_cnts = 0; }"),
            vec![("cnts".into(), Rule::Avoid, Severity::Info)]
        );
    }

    #[test]
    fn rust_naming_conventions() {
        // getter(C-GETTER)とコンストラクタ new・empty
        assert_eq!(
            check("impl Order { fn new() {} fn price(&self) {} fn empty() {} }"),
            vec![]
        );
    }

    #[test]
    fn test_functions_skip_part_of_speech() {
        assert_eq!(check("#[test]\nfn only_price() {}"), vec![]);
        // 未知語は検査する
        assert_eq!(
            check("#[test]\nfn load_shukka() {}"),
            vec![("shukka".into(), Rule::Unknown, Severity::Warning)]
        );
    }

    #[test]
    fn trait_implementations_are_not_checked() {
        assert_eq!(
            check("impl Iterator for Order { type Item = u8; fn next(&mut self) -> Option<u8> { None } }"),
            vec![]
        );
    }

    #[test]
    fn disable_comment_and_ignore_words() {
        assert_eq!(check("// yogolint-disable-next-line mgr\nstruct OrderMgr;"), vec![]);
        assert_eq!(check("// yogolint-disable-next-line\nstruct OrderMgr;"), vec![]);
        let config = Config {
            ignore_words: vec!["shukka".into()],
            ..Config::default()
        };
        let findings = linter(config)
            .check_source(Language::Rust, "struct ShukkaDate;")
            .unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn non_ascii_identifiers_are_info() {
        assert_eq!(
            check("fn load() { let 注文 = 0; }"),
            vec![("注文".into(), Rule::NonAscii, Severity::Info)]
        );
    }

    #[test]
    fn location_is_word_range() {
        let findings = linter(Config::default())
            .check_source(Language::Rust, "\nstruct OrderMgr;")
            .unwrap();
        let location = findings[0].location;
        assert_eq!((location.line, location.column), (2, 13));
        assert_eq!(location.end_byte - location.start_byte, 3);
    }

    #[test]
    fn builtin_layers_can_be_disabled() {
        let config = Config {
            builtin_dictionaries: vec![Layer::Verbs, Layer::Nouns, Layer::Abbrev],
            ..Config::default()
        };
        let findings = linter(config).check_source(Language::Rust, "struct OrderMgr;").unwrap();
        assert_eq!(findings[0].rule, Rule::Unknown);
    }
}
