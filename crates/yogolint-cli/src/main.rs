//! yogolint: 定義名を構成する単語が用語辞書にあるかを検査する CLI。

mod files;
mod output;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use yogolint_core::dict::{DictFile, builtin_text};
use yogolint_core::{Config, Dictionary, Language, Layer, Linter, Rule, Severity};

use crate::files::SourceFile;
use crate::output::{FileFinding, Summary};

const CONFIG_FILE: &str = "yogolint.toml";

#[derive(Parser)]
#[command(name = "yogolint", version, about = "定義名の語彙を用語辞書で検査する")]
struct Cli {
    /// 設定ファイル(既定はカレントディレクトリの yogolint.toml。無ければ既定値)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 検査する
    Check {
        /// 検査するファイル・ディレクトリ(既定は設定ファイルのあるディレクトリ)
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// 警告がこの件数を超えたら終了コードを 1 にする
        #[arg(long)]
        max_warnings: Option<usize>,
    },
    /// 未知語を出現回数の多い順に一覧する
    Words {
        paths: Vec<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Text)]
        format: Format,
        /// 表示する件数
        #[arg(long)]
        limit: Option<usize>,
    },
    /// 辞書を扱う
    Dicts {
        #[command(subcommand)]
        command: DictsCommand,
    },
}

#[derive(Subcommand)]
enum DictsCommand {
    /// 埋め込みの標準辞書を TOML ファイルに書き出す
    Export {
        /// 出力先ディレクトリ
        #[arg(long, default_value = "dicts")]
        out: PathBuf,
        /// 書き出す層(カンマ区切り。既定は全て)
        #[arg(long, value_delimiter = ',')]
        layer: Vec<String>,
        /// 同名のファイルを上書きする
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("yogolint: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        Command::Check {
            paths,
            format,
            max_warnings,
        } => {
            let workspace = Workspace::load(cli.config.as_deref())?;
            let (findings, file_count) = workspace.check(&paths)?;
            let findings: Vec<FileFinding> = findings
                .into_iter()
                .filter(|finding| finding.finding.severity != Severity::None)
                .collect();
            let summary = Summary::new(&findings, file_count);
            match format {
                Format::Text => output::print_text(&findings, &summary),
                Format::Json => output::print_json(&findings, &summary)?,
            }
            let too_many_warnings = max_warnings.is_some_and(|limit| summary.warnings > limit);
            Ok(if summary.errors > 0 || too_many_warnings {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            })
        }
        Command::Words { paths, format, limit } => {
            let workspace = Workspace::load(cli.config.as_deref())?;
            let (findings, _) = workspace.check(&paths)?;
            let mut counts = output::count_words(&findings, &[Rule::Unknown, Rule::Compound]);
            if let Some(limit) = limit {
                counts.truncate(limit);
            }
            match format {
                Format::Text => output::print_words_text(&counts),
                Format::Json => output::print_words_json(&counts)?,
            }
            Ok(ExitCode::SUCCESS)
        }
        Command::Dicts {
            command: DictsCommand::Export { out, layer, force },
        } => export_dicts(&out, &layer, force),
    }
}

/// 設定・辞書と、それらを置いたディレクトリ
struct Workspace {
    base_dir: PathBuf,
    config: Config,
    dictionary: Dictionary,
}

impl Workspace {
    fn load(config_path: Option<&Path>) -> Result<Workspace> {
        let (config, base_dir) = match config_path {
            Some(path) => (read_config(path)?, parent_dir(path)),
            None if Path::new(CONFIG_FILE).is_file() => (read_config(Path::new(CONFIG_FILE))?, PathBuf::from(".")),
            None => (Config::default(), PathBuf::from(".")),
        };
        let mut user_files = Vec::new();
        for dictionary in &config.dictionaries {
            let path = base_dir.join(dictionary);
            let text = fs::read_to_string(&path).with_context(|| format!("辞書 {} を読めない", path.display()))?;
            user_files.push(DictFile::parse(&path.display().to_string(), &text)?);
        }
        let dictionary = Dictionary::load(&config.builtin_dictionaries, &user_files)?;
        Ok(Workspace {
            base_dir,
            config,
            dictionary,
        })
    }

    fn languages(&self) -> Result<Vec<Language>> {
        self.config
            .languages
            .iter()
            .map(|name| Language::from_name(name).with_context(|| format!("未対応の言語: {name}")))
            .collect()
    }

    /// 対象ファイルを全て検査し、(報告, ファイル数) を返す。
    fn check(&self, paths: &[PathBuf]) -> Result<(Vec<FileFinding>, usize)> {
        let languages = self.languages()?;
        let roots = if paths.is_empty() {
            vec![self.base_dir.clone()]
        } else {
            paths.to_vec()
        };
        let sources = files::collect(&roots, &self.base_dir, &self.config, &languages)?;
        let mut linter = Linter::new(self.config.clone(), self.dictionary.clone());
        let mut findings = Vec::new();
        for SourceFile { path, language } in &sources {
            let source = match fs::read_to_string(path) {
                Ok(source) => source,
                Err(error) => {
                    eprintln!("yogolint: {} を読めない: {error}", path.display());
                    continue;
                }
            };
            let file_findings = linter.check_source(*language, &source).map_err(anyhow::Error::msg)?;
            findings.extend(file_findings.into_iter().map(|finding| FileFinding {
                path: path.clone(),
                finding,
            }));
        }
        Ok((findings, sources.len()))
    }
}

fn read_config(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).with_context(|| format!("設定 {} を読めない", path.display()))?;
    Config::from_toml(&text).with_context(|| format!("設定 {} が不正", path.display()))
}

fn parent_dir(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn export_dicts(out: &Path, layer_names: &[String], force: bool) -> Result<ExitCode> {
    let layers: Vec<Layer> = if layer_names.is_empty() {
        Layer::ALL
            .into_iter()
            .filter(|layer| builtin_text(*layer).is_some())
            .collect()
    } else {
        layer_names
            .iter()
            .map(|name| {
                let layer = Layer::parse(name.trim()).with_context(|| format!("不明な層: {name}"))?;
                if builtin_text(layer).is_none() {
                    bail!("層 {name} には標準辞書が無い");
                }
                Ok(layer)
            })
            .collect::<Result<_>>()?
    };
    fs::create_dir_all(out).with_context(|| format!("{} を作れない", out.display()))?;
    let mut skipped = HashMap::new();
    for layer in layers {
        let path = out.join(layer.file_name());
        if path.exists() && !force {
            skipped.insert(layer, path);
            continue;
        }
        let text = builtin_text(layer).unwrap_or_default();
        let header = format!(
            "# yogolint {} の標準辞書({} 層)から書き出した。\n# yogolint.toml の dictionaries に指定すると、同じ層の標準辞書の代わりに使われる。\n",
            env!("CARGO_PKG_VERSION"),
            layer.as_str()
        );
        fs::write(&path, format!("{header}{text}")).with_context(|| format!("{} に書けない", path.display()))?;
        println!("{}", path.display());
    }
    for path in skipped.values() {
        eprintln!("yogolint: {} は既にある(上書きするには --force)", path.display());
    }
    Ok(if skipped.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}
