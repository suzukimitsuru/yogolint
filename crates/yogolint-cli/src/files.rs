//! 検査対象ファイルの収集(.gitignore を尊重し、include / exclude / ignorePaths で絞る)。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use yogolint_core::{Config, Language};

pub struct SourceFile {
    pub path: PathBuf,
    pub language: Language,
}

fn glob_set(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder.add(Glob::new(pattern).with_context(|| format!("不正な glob: {pattern}"))?);
    }
    Ok(builder.build()?)
}

pub fn collect(roots: &[PathBuf], base_dir: &Path, config: &Config, languages: &[Language]) -> Result<Vec<SourceFile>> {
    let include = glob_set(&config.include)?;
    let mut excluded = config.exclude.clone();
    excluded.extend(config.ignore_paths.iter().cloned());
    let exclude = glob_set(&excluded)?;
    let base = base_dir.canonicalize().unwrap_or_else(|_| base_dir.to_path_buf());

    let accepts = |path: &Path, explicit: bool| -> Option<Language> {
        let extension = path.extension()?.to_str()?;
        let language = Language::from_extension(extension).filter(|language| languages.contains(language))?;
        // glob は設定ファイルのあるディレクトリからの相対パスに照合する
        let absolute = path.canonicalize().ok()?;
        let relative = absolute.strip_prefix(&base).unwrap_or(&absolute);
        if exclude.is_match(relative) {
            return None;
        }
        if !explicit && !config.include.is_empty() && !include.is_match(relative) {
            return None;
        }
        Some(language)
    };

    let mut files = Vec::new();
    for root in roots {
        if root.is_file() {
            if let Some(language) = accepts(root, true) {
                files.push(SourceFile {
                    path: root.clone(),
                    language,
                });
            }
            continue;
        }
        if !root.exists() {
            anyhow::bail!("{} が無い", root.display());
        }
        for entry in WalkBuilder::new(root).build() {
            let entry = entry?;
            if !entry.file_type().is_some_and(|file_type| file_type.is_file()) {
                continue;
            }
            if let Some(language) = accepts(entry.path(), false) {
                files.push(SourceFile {
                    path: clean(entry.path()),
                    language,
                });
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok(files)
}

/// `./src/main.rs` → `src/main.rs`
fn clean(path: &Path) -> PathBuf {
    path.strip_prefix(".")
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}
