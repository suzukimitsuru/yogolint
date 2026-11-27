//! tree-sitter で定義名を抽出する。文法同梱の tags.scm と、自前の names.scm を使う。

use std::collections::HashMap;

use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::Kind;

/// 対応する言語。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
}

impl Language {
    pub const ALL: [Language; 1] = [Language::Rust];

    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
        }
    }

    pub fn from_name(name: &str) -> Option<Language> {
        Language::ALL.into_iter().find(|language| language.name() == name)
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Language::Rust => &["rs"],
        }
    }

    pub fn from_extension(extension: &str) -> Option<Language> {
        Language::ALL
            .into_iter()
            .find(|language| language.extensions().contains(&extension))
    }

    /// 行コメントの書き出し(抑止コメントの検出に使う)
    pub fn line_comment(self) -> &'static str {
        match self {
            Language::Rust => "//",
        }
    }

    /// 名前が外部で決まる定義か(定義でなく参照に当たるので検査しない)。
    /// Rust では trait 実装(`impl Trait for X`)の中のメソッド・関連型・関連定数。
    fn is_external_name(self, name: Node) -> bool {
        match self {
            Language::Rust => {
                let Some(list) = name.parent().and_then(|item| item.parent()) else {
                    return false;
                };
                list.kind() == "declaration_list"
                    && list.parent().is_some_and(|parent| {
                        parent.kind() == "impl_item" && parent.child_by_field_name("trait").is_some()
                    })
            }
        }
    }

    /// テスト関数か。Rust では `#[test]`・`#[tokio::test]` などの属性が付いた関数。
    fn is_test_function(self, name: Node, source: &str) -> bool {
        match self {
            Language::Rust => {
                let Some(item) = name.parent().filter(|item| item.kind() == "function_item") else {
                    return false;
                };
                let mut sibling = item.prev_named_sibling();
                while let Some(node) = sibling {
                    match node.kind() {
                        "attribute_item" if is_test_attribute(&source[node.byte_range()]) => return true,
                        "attribute_item" | "line_comment" | "block_comment" => {}
                        _ => break,
                    }
                    sibling = node.prev_named_sibling();
                }
                false
            }
        }
    }

    fn grammar(self) -> tree_sitter::Language {
        match self {
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        }
    }

    fn query_source(self) -> String {
        match self {
            Language::Rust => {
                format!(
                    "{}\n{}",
                    tree_sitter_rust::TAGS_QUERY,
                    include_str!("../queries/rust/names.scm")
                )
            }
        }
    }
}

/// 言語ごとの命名の慣習。品詞位置の検査に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamingConvention {
    /// 関数名の先頭語に置ける品詞
    pub function_head_pos: &'static [&'static str],
    /// 関数名の先頭語なら、避ける同義語でも許す語
    pub function_head_words: &'static [&'static str],
    /// 関数名の先頭語の規則の説明(報告のメッセージに使う)
    pub function_head_hint: &'static str,
}

impl Language {
    /// 命名の慣習
    pub fn naming(self) -> NamingConvention {
        match self {
            // getter は get_ を付けずにフィールド名を使い(Rust API Guidelines の C-GETTER)、
            // コンストラクタは new や empty のように名詞・形容詞で書く
            Language::Rust => NamingConvention {
                function_head_pos: &["verb", "prefix", "noun", "adjective"],
                function_head_words: &["new"],
                function_head_hint: "動詞・述語接頭辞(is / has / to / from など)・名詞(getter)・形容詞(コンストラクタ)",
            },
        }
    }
}

/// 抽出した定義名1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub name: String,
    pub kind: Kind,
    pub start_byte: usize,
    pub end_byte: usize,
    /// 0 始まりの行
    pub row: usize,
    /// 行内のバイト位置
    pub column_byte: usize,
    /// テスト関数(`#[test]` など)か。テスト名は文で書くので、品詞位置の検査から外す
    pub is_test: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Extraction {
    pub definitions: Vec<Definition>,
    /// 構文エラー内にあって検査しなかった定義の数
    pub skipped_in_error: usize,
    /// 最初の構文エラーの位置 (row, column_byte)
    pub first_error: Option<(usize, usize)>,
}

/// 言語ごとの構文解析器とクエリ。
pub struct Extractor {
    language: Language,
    parser: Parser,
    query: Query,
    name_capture: u32,
    /// 捕捉番号 → 種別
    kinds: Vec<Option<Kind>>,
}

impl Extractor {
    pub fn new(language: Language) -> Result<Extractor, String> {
        let grammar = language.grammar();
        let mut parser = Parser::new();
        parser.set_language(&grammar).map_err(|error| error.to_string())?;
        let query = Query::new(&grammar, &language.query_source()).map_err(|error| error.to_string())?;
        let name_capture = query
            .capture_index_for_name("name")
            .ok_or("query has no @name capture")?;
        let kinds = query
            .capture_names()
            .iter()
            .map(|name| Kind::from_capture_name(name))
            .collect();
        Ok(Extractor {
            language,
            parser,
            query,
            name_capture,
            kinds,
        })
    }

    /// 1ファイルを1回だけ構文解析し、全種別の定義名を取る。同じ位置の重複捕捉は1件にまとめる。
    pub fn extract(&mut self, source: &str) -> Extraction {
        let Some(tree) = self.parser.parse(source, None) else {
            return Extraction::default();
        };
        let root = tree.root_node();
        let mut extraction = Extraction {
            first_error: root.has_error().then(|| first_error(root)).flatten(),
            ..Default::default()
        };
        // 名前ノードの開始位置 → (パターン番号, 定義)
        let mut found: HashMap<usize, (usize, Definition)> = HashMap::new();
        let mut skipped: HashMap<usize, ()> = HashMap::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&self.query, root, source.as_bytes());
        while let Some(found_match) = matches.next() {
            let mut name_node = None;
            let mut kind = None;
            for capture in found_match.captures() {
                if capture.index == self.name_capture {
                    name_node = Some(capture.node);
                } else if let Some(Some(capture_kind)) = self.kinds.get(capture.index as usize) {
                    kind = Some(*capture_kind);
                }
            }
            let (Some(node), Some(kind)) = (name_node, kind) else {
                continue;
            };
            if inside_error(node) {
                skipped.insert(node.start_byte(), ());
                continue;
            }
            if self.language.is_external_name(node) {
                continue;
            }
            let definition = Definition {
                name: source[node.byte_range()].to_string(),
                kind,
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                row: node.start_position().row,
                column_byte: node.start_position().column,
                is_test: kind == Kind::Function && self.language.is_test_function(node, source),
            };
            let pattern = found_match.pattern_index;
            match found.get(&definition.start_byte) {
                Some((existing, _)) if *existing <= pattern => {}
                _ => {
                    found.insert(definition.start_byte, (pattern, definition));
                }
            }
        }
        let mut definitions: Vec<Definition> = found.into_values().map(|(_, definition)| definition).collect();
        definitions.sort_by_key(|definition| definition.start_byte);
        extraction.definitions = definitions;
        extraction.skipped_in_error = skipped.len();
        extraction
    }
}

/// `#[test]`・`#[tokio::test]`・`#[test_case(..)]` のように、属性の最後の名前が test で始まるか。
/// `#[cfg(test)]` は含めない。
fn is_test_attribute(text: &str) -> bool {
    let inner = text.trim().trim_start_matches("#[").trim_end_matches(']');
    let path = inner.split(['(', '=', ' ']).next().unwrap_or_default();
    let last = path.rsplit("::").next().unwrap_or_default();
    last == "test" || last.starts_with("test_")
}

fn inside_error(node: Node) -> bool {
    let mut current = Some(node);
    while let Some(node) = current {
        if node.is_error() || node.is_missing() {
            return true;
        }
        current = node.parent();
    }
    false
}

fn first_error(node: Node) -> Option<(usize, usize)> {
    if node.is_error() || node.is_missing() {
        return Some((node.start_position().row, node.start_position().column));
    }
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .filter(|child| child.has_error())
        .find_map(first_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(source: &str) -> Vec<(String, Kind)> {
        let mut extractor = Extractor::new(Language::Rust).unwrap();
        extractor
            .extract(source)
            .definitions
            .into_iter()
            .map(|definition| (definition.name, definition.kind))
            .collect()
    }

    #[test]
    fn extracts_all_kinds() {
        let source = r#"
const MAX_RETRY_COUNT: u32 = 3;
struct Order { order_id: u64 }
enum Status { Active }
trait Repository { fn find_order(&self, id: u64); }
impl Order {
    fn total_price(&self, rate: f64) -> f64 {
        let mut sum = 0.0;
        for item_count in 0..3 { sum += item_count as f64; }
        let apply = |value: f64| value * rate;
        apply(sum)
    }
}
"#;
        let found = names(source);
        let expected = [
            ("MAX_RETRY_COUNT", Kind::Constant),
            ("Order", Kind::Type),
            ("order_id", Kind::Value),
            ("Status", Kind::Type),
            ("Active", Kind::Constant),
            ("Repository", Kind::Type),
            ("find_order", Kind::Function),
            ("id", Kind::Value),
            ("total_price", Kind::Function),
            ("rate", Kind::Value),
            ("sum", Kind::Value),
            ("item_count", Kind::Value),
            ("apply", Kind::Value),
            ("value", Kind::Value),
        ];
        let expected: Vec<(String, Kind)> = expected.iter().map(|(name, kind)| (name.to_string(), *kind)).collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn skips_names_decided_by_traits() {
        let source = r#"
trait Shape { fn area(&self) -> f64; }
impl Iterator for Circle {
    type Item = u8;
    const LIMIT: u8 = 1;
    fn next(&mut self, step_count: u8) -> Option<u8> { None }
}
impl Circle { fn radius(&self) -> f64 { 0.0 } }
"#;
        let found: Vec<String> = names(source).into_iter().map(|(name, _)| name).collect();
        assert_eq!(found, ["Shape", "area", "step_count", "radius"]);
    }

    #[test]
    fn marks_test_functions() {
        let source = "#[test]\n#[ignore]\nfn parses_empty_input() {}\n#[tokio::test]\nasync fn fetches_value() {}\n#[cfg(test)]\nfn build_fixture() {}\n";
        let mut extractor = Extractor::new(Language::Rust).unwrap();
        let tests: Vec<(String, bool)> = extractor
            .extract(source)
            .definitions
            .into_iter()
            .map(|definition| (definition.name, definition.is_test))
            .collect();
        assert_eq!(
            tests,
            [
                ("parses_empty_input".to_string(), true),
                ("fetches_value".to_string(), true),
                ("build_fixture".to_string(), false)
            ]
        );
    }

    #[test]
    fn skips_definitions_inside_errors() {
        let mut extractor = Extractor::new(Language::Rust).unwrap();
        let extraction = extractor.extract("fn good_name() {}\nfn broken( { let inner_value = 1; \n");
        assert!(extraction.first_error.is_some());
        assert!(
            extraction
                .definitions
                .iter()
                .any(|definition| definition.name == "good_name")
        );
    }
}
