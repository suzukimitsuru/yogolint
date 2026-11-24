//! 報告の整形(text・JSON)。

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::Serialize;
use yogolint_core::{Finding, Rule, Severity};

#[derive(Serialize)]
pub struct FileFinding {
    pub path: PathBuf,
    #[serde(flatten)]
    pub finding: Finding,
}

#[derive(Serialize)]
pub struct Summary {
    pub files: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

impl Summary {
    pub fn new(findings: &[FileFinding], files: usize) -> Summary {
        let count = |severity| {
            findings
                .iter()
                .filter(|finding| finding.finding.severity == severity)
                .count()
        };
        Summary {
            files,
            errors: count(Severity::Error),
            warnings: count(Severity::Warning),
            infos: count(Severity::Info),
        }
    }
}

pub fn print_text(findings: &[FileFinding], summary: &Summary) {
    let rows: Vec<(String, &FileFinding)> = findings
        .iter()
        .map(|item| {
            let location = item.finding.location;
            (
                format!("{}:{}:{}", item.path.display(), location.line, location.column),
                item,
            )
        })
        .collect();
    let location_width = rows.iter().map(|(location, _)| location.len()).max().unwrap_or(0);
    // 長い識別子に引きずられないよう、識別子の列幅は上限を設ける
    let identifier_width = findings
        .iter()
        .map(|item| item.finding.identifier.chars().count())
        .max()
        .unwrap_or(0)
        .min(24);
    for (location, item) in &rows {
        let finding = &item.finding;
        let mut message = finding.message.clone();
        if !finding.suggestions.is_empty() && finding.rule == Rule::Deny {
            message.push_str(&format!("(候補: {})", finding.suggestions.join(", ")));
        }
        println!(
            "{location:<location_width$}  {:<7}  {:<identifier_width$}  {message}  [{}]",
            finding.severity.as_str(),
            finding.identifier,
            finding.rule.as_str(),
        );
    }
    eprintln!(
        "{} ファイル: エラー {} 件、警告 {} 件、情報 {} 件",
        summary.files, summary.errors, summary.warnings, summary.infos
    );
}

pub fn print_json(findings: &[FileFinding], summary: &Summary) -> Result<()> {
    #[derive(Serialize)]
    struct Report<'a> {
        findings: &'a [FileFinding],
        summary: &'a Summary,
    }
    println!("{}", serde_json::to_string_pretty(&Report { findings, summary })?);
    Ok(())
}

#[derive(Serialize)]
pub struct WordCount {
    pub word: String,
    pub rule: Rule,
    pub count: usize,
    /// 最初に見つかった識別子と位置
    pub example: String,
    pub location: String,
}

/// 指定した規則の語を、出現回数の多い順に数える。
pub fn count_words(findings: &[FileFinding], rules: &[Rule]) -> Vec<WordCount> {
    let mut counts: HashMap<(String, Rule), WordCount> = HashMap::new();
    for item in findings.iter().filter(|item| rules.contains(&item.finding.rule)) {
        let finding = &item.finding;
        counts
            .entry((finding.word.clone(), finding.rule))
            .or_insert_with(|| WordCount {
                word: finding.word.clone(),
                rule: finding.rule,
                count: 0,
                example: finding.identifier.clone(),
                location: format!(
                    "{}:{}:{}",
                    item.path.display(),
                    finding.location.line,
                    finding.location.column
                ),
            })
            .count += 1;
    }
    let mut counts: Vec<WordCount> = counts.into_values().collect();
    counts.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
    counts
}

pub fn print_words_text(counts: &[WordCount]) {
    let word_width = counts.iter().map(|count| count.word.len()).max().unwrap_or(0);
    for count in counts {
        println!(
            "{:>5}  {:<word_width$}  {:<8}  {} ({})",
            count.count,
            count.word,
            count.rule.as_str(),
            count.example,
            count.location
        );
    }
    eprintln!("{} 語", counts.len());
}

pub fn print_words_json(counts: &[WordCount]) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(counts)?);
    Ok(())
}
