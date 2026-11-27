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

/// 報告の規則。LSP の Diagnostic.code に当たる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rule {
    /// 禁止語・禁止パターン
    Deny,
    /// 避ける同義語・展開すべき略語
    Avoid,
    /// 連結語
    Compound,
    /// 未知語
    Unknown,
    /// 品詞位置
    Position,
    /// 語順(関数名が名詞で始まり、後ろに動詞がある)
    WordOrder,
    /// 短すぎる語
    Short,
    /// 英字以外を含む識別子(検査しない)
    NonAscii,
    /// 構文エラー内の定義(検査しない)
    Syntax,
}

impl Rule {
    pub fn as_str(self) -> &'static str {
        match self {
            Rule::Deny => "deny",
            Rule::Avoid => "avoid",
            Rule::Compound => "compound",
            Rule::Unknown => "unknown",
            Rule::Position => "position",
            Rule::WordOrder => "word-order",
            Rule::Short => "short",
            Rule::NonAscii => "non-ascii",
            Rule::Syntax => "syntax",
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
