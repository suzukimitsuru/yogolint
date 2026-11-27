use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn yogolint(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_yogolint"))
        .current_dir(directory)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = directory.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    directory
}

#[test]
fn check_reports_and_sets_exit_code() {
    let directory = project(&[("src/lib.rs", "struct OrderMgr;\nfn get_value() {}\n")]);
    let output = yogolint(directory.path(), &["check"]);
    assert_eq!(output.status.code(), Some(0), "warnings only");
    let text = stdout(&output);
    assert!(text.contains("src/lib.rs:1:13"), "{text}");
    assert!(text.contains("[denied-word]"), "{text}");

    let directory = project(&[("src/lib.rs", "fn load() { let foo = 1; }\n")]);
    let output = yogolint(directory.path(), &["check"]);
    assert_eq!(output.status.code(), Some(1), "errors fail");
}

#[test]
fn max_warnings_fails_when_exceeded() {
    let directory = project(&[("src/lib.rs", "struct OrderMgr;\n")]);
    assert_eq!(
        yogolint(directory.path(), &["check", "--max-warnings", "0"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        yogolint(directory.path(), &["check", "--max-warnings", "1"])
            .status
            .code(),
        Some(0)
    );
}

#[test]
fn json_output() {
    let directory = project(&[("src/lib.rs", "fn retrieve_value() {}\n")]);
    let output = yogolint(directory.path(), &["check", "--format", "json"]);
    let report: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let finding = &report["findings"][0];
    assert_eq!(finding["rule"], "avoided-word");
    assert_eq!(finding["suggestions"][0], "get");
    assert_eq!(finding["location"]["line"], 1);
    assert_eq!(report["summary"]["warnings"], 1);
}

#[test]
fn config_include_exclude_and_domain_dictionary() {
    let directory = project(&[
        (
            "yogolint.toml",
            "include = [\"src/**\"]\nexclude = [\"**/generated/**\"]\ndictionaries = [\"dicts/domain.toml\"]\n",
        ),
        (
            "dicts/domain.toml",
            "version = 1\nlayer = \"domain\"\n[[terms]]\nword = \"shukka\"\npos = \"noun\"\nja = \"出荷\"\n",
        ),
        ("src/lib.rs", "struct ShukkaDate;\n"),
        ("src/generated/out.rs", "struct Kingaku;\n"),
        ("other/lib.rs", "struct Kingaku;\n"),
    ]);
    let output = yogolint(directory.path(), &["check"]);
    assert_eq!(stdout(&output), "", "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn rule_settings_change_severity_and_reject_typos() {
    let directory = project(&[
        ("src/lib.rs", "fn user_delete() {}\n"),
        ("yogolint.toml", "[rules]\nfunction-noun-verb-order = \"error\"\n"),
    ]);
    let output = yogolint(directory.path(), &["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", stdout(&output));
    assert!(stdout(&output).contains("[function-noun-verb-order]"));

    fs::write(
        directory.path().join("yogolint.toml"),
        "[rules]\nnoun-verb-order = \"none\"\n",
    )
    .unwrap();
    assert_eq!(yogolint(directory.path(), &["check"]).status.code(), Some(2));
}

#[test]
fn words_lists_unknown_words_by_count() {
    let directory = project(&[(
        "src/lib.rs",
        "struct KingakuTotal;\nfn load(kingaku: u32, shukka: u32) {}\n",
    )]);
    let output = yogolint(directory.path(), &["words", "--format", "json"]);
    let counts: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(counts[0]["word"], "kingaku");
    assert_eq!(counts[0]["count"], 2);
    assert_eq!(counts[1]["word"], "shukka");
}

#[test]
fn dicts_export_respects_force() {
    let directory = project(&[]);
    let output = yogolint(directory.path(), &["dicts", "export", "--layer", "verbs,deny"]);
    assert!(output.status.success());
    let verbs = fs::read_to_string(directory.path().join("dicts/general-verbs.toml")).unwrap();
    assert!(verbs.starts_with("# yogolint "));
    assert!(directory.path().join("dicts/deny.toml").exists());
    assert!(!directory.path().join("dicts/abbrev.toml").exists());

    fs::write(directory.path().join("dicts/deny.toml"), "edited").unwrap();
    let output = yogolint(directory.path(), &["dicts", "export", "--layer", "deny"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(directory.path().join("dicts/deny.toml")).unwrap(),
        "edited"
    );
    assert!(
        yogolint(directory.path(), &["dicts", "export", "--layer", "deny", "--force"])
            .status
            .success()
    );
    assert_ne!(
        fs::read_to_string(directory.path().join("dicts/deny.toml")).unwrap(),
        "edited"
    );
}

#[test]
fn exported_dictionary_replaces_builtin_layer() {
    let directory = project(&[("src/lib.rs", "struct OrderMgr;\n")]);
    assert!(
        yogolint(directory.path(), &["dicts", "export", "--layer", "deny"])
            .status
            .success()
    );
    let path = directory.path().join("dicts/deny.toml");
    let edited = fs::read_to_string(&path)
        .unwrap()
        .replace("avoid = [\"mgr\", \"management\"]", "");
    fs::write(&path, edited).unwrap();
    fs::write(
        directory.path().join("yogolint.toml"),
        "dictionaries = [\"dicts/deny.toml\"]\n",
    )
    .unwrap();
    let text = stdout(&yogolint(directory.path(), &["check"]));
    assert!(
        text.contains("[unknown-word]") && !text.contains("[denied-word]"),
        "{text}"
    );
}
