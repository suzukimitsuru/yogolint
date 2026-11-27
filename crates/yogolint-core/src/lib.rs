//! yogolint の中核: 定義名の抽出・単語分割・正規化・辞書との照合。
//!
//! ファイル入出力は持たない。ソース文字列を受け取り、[`Finding`] の列を返す。

pub mod check;
pub mod config;
pub mod dict;
pub mod extract;
pub mod split;

use serde::{Deserialize, Serialize};

pub use check::Linter;
pub use config::Config;
pub use dict::{Dictionary, Layer};
pub use extract::Language;

/// 報告の重大度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    None,
    Info,
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::None => "none",
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// 定義の種別。照合時の品詞位置の検査に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Function,
    Type,
    Value,
    Constant,
}

impl Kind {
    /// tags.scm 流儀の捕捉名(`definition.function` など)から種別を決める。
    pub fn from_capture_name(name: &str) -> Option<Kind> {
        match name.strip_prefix("definition.")? {
            "function" | "method" => Some(Kind::Function),
            "class" | "interface" | "type" => Some(Kind::Type),
            "variable" | "field" | "parameter" => Some(Kind::Value),
            "constant" => Some(Kind::Constant),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Function => "function",
            Kind::Type => "type",
            Kind::Value => "value",
            Kind::Constant => "constant",
        }
    }
}

/// 報告の規則。LSP の Diagnostic.code に当たる。名前は「何を報告するか」を kebab-case の名詞で表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// 禁止語
    DeniedWord,
    /// 禁止パターン(識別子全体に照合する。連番など)
    DeniedPattern,
    /// 避ける同義語・展開すべき略語
    AvoidedWord,
    /// 未知語
    UnknownWord,
    /// 連結語
    CompoundWord,
    /// 短すぎる語
    ShortWord,
    /// 関数名の先頭語の品詞
    FunctionFirstWordPartOfSpeech,
    /// 型名の末尾語の品詞
    TypeLastWordPartOfSpeech,
    /// 値の名前の末尾語の品詞
    ValueLastWordPartOfSpeech,
    /// 定数名の末尾語の品詞
    ConstantLastWordPartOfSpeech,
    /// 名詞＋動詞の語順の関数名(user_delete)
    FunctionNounVerbOrder,
    /// 英字以外を含む識別子(検査しない)
    NonAsciiIdentifier,
    /// 構文エラー内の定義(検査しない)
    SyntaxError,
}

impl Rule {
    pub const ALL: [Rule; 13] = [
        Rule::DeniedWord,
        Rule::DeniedPattern,
        Rule::AvoidedWord,
        Rule::UnknownWord,
        Rule::CompoundWord,
        Rule::ShortWord,
        Rule::FunctionFirstWordPartOfSpeech,
        Rule::TypeLastWordPartOfSpeech,
        Rule::ValueLastWordPartOfSpeech,
        Rule::ConstantLastWordPartOfSpeech,
        Rule::FunctionNounVerbOrder,
        Rule::NonAsciiIdentifier,
        Rule::SyntaxError,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Rule::DeniedWord => "denied-word",
            Rule::DeniedPattern => "denied-pattern",
            Rule::AvoidedWord => "avoided-word",
            Rule::UnknownWord => "unknown-word",
            Rule::CompoundWord => "compound-word",
            Rule::ShortWord => "short-word",
            Rule::FunctionFirstWordPartOfSpeech => "function-first-word-part-of-speech",
            Rule::TypeLastWordPartOfSpeech => "type-last-word-part-of-speech",
            Rule::ValueLastWordPartOfSpeech => "value-last-word-part-of-speech",
            Rule::ConstantLastWordPartOfSpeech => "constant-last-word-part-of-speech",
            Rule::FunctionNounVerbOrder => "function-noun-verb-order",
            Rule::NonAsciiIdentifier => "non-ascii-identifier",
            Rule::SyntaxError => "syntax-error",
        }
    }

    /// 既定の重大度。辞書の項目ごとに重大度を持つ規則は None
    pub fn default_severity(self) -> Option<Severity> {
        match self {
            Rule::DeniedWord | Rule::DeniedPattern | Rule::AvoidedWord => None,
            Rule::UnknownWord
            | Rule::CompoundWord
            | Rule::ShortWord
            | Rule::FunctionFirstWordPartOfSpeech
            | Rule::FunctionNounVerbOrder => Some(Severity::Warning),
            Rule::TypeLastWordPartOfSpeech
            | Rule::ValueLastWordPartOfSpeech
            | Rule::ConstantLastWordPartOfSpeech
            | Rule::NonAsciiIdentifier
            | Rule::SyntaxError => Some(Severity::Info),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_names_match_serde_names() {
        for rule in Rule::ALL {
            let serialized = serde_json::to_string(&rule).unwrap();
            assert_eq!(serialized, format!("\"{}\"", rule.as_str()));
        }
    }
}

/// ソース上の位置。行・列は 1 始まり、列は文字(Unicode スカラー値)単位。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Location {
    pub line: usize,
    pub column: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// 1件の報告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// 識別子のうち該当する語の部分
    pub location: Location,
    pub identifier: String,
    pub kind: Option<Kind>,
    /// 該当する語(小文字)。識別子全体が対象のときは識別子
    pub word: String,
    pub rule: Rule,
    pub severity: Severity,
    pub message: String,
    /// 置換候補
    pub suggestions: Vec<String>,
}
