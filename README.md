# yogolint

用語(yōgo)+ lint。名前そのものが、本ツールが検出する悪い命名の例である。

命名規則 lint が書式(camelCase など)を保証した後段で、**定義名を構成する単語が用語辞書にあるか**だけを検査する。設計は [docs/yogolint_minimam_spec.md](docs/yogolint_minimam_spec.md) を参照。

現在は第1段階(Rust のコードのみ、CLI の `check`・`words`・`dicts export`)。

## 使い方

```sh
cargo install --path crates/yogolint-cli

yogolint check [paths]            # 検査する(--format text|json、--max-warnings N)
yogolint words [paths]            # 未知語を出現回数の多い順に一覧する(ドメイン辞書づくりの起点)
yogolint dicts export             # 埋め込みの標準辞書を dicts/ に書き出す(--out、--layer、--force)
```

```text
src/order/model.rs:5:14  warning  OrderMgr    mgr: 禁止語 manager の異表記。…  [deny]
src/order/model.rs:9:3   warning  shukka_date shukka: 未知語                   [unknown]
```

終了コード: エラーが1件以上なら 1、警告だけなら 0(`--max-warnings` を超えたら 1)、設定や辞書の誤りは 2。

## 設定 `yogolint.toml`

```toml
languages = ["rust"]
include = ["src/**"]
exclude = ["**/generated/**"]
kinds = ["function", "type", "value", "constant"]
builtinDictionaries = ["verbs", "nouns", "abbrev", "deny"]   # 埋め込み辞書のうち使う層
dictionaries = ["dicts/domain.toml"]                         # 同じ層の埋め込み辞書を置き換える
unknownWord = "warning"
allowCompoundWords = false
minWordLength = 2
allowSingleLetter = ["i", "j", "k", "n", "x", "y"]
ignoreWords = []
ignorePaths = []
```

行単位の抑止: `// yogolint-disable-next-line mgr`(語を省くと、次の行の全ての報告を抑止する)。

## 構成

```text
crates/yogolint-core   抽出・分割・正規化・照合(入出力なし)
  dicts/               埋め込みの標準辞書(verbs・nouns・abbrev・deny)
  queries/rust/        tree-sitter-rust 同梱の tags.scm を補う names.scm
crates/yogolint-cli    yogolint コマンド
dicts/domain.toml      このリポジトリ自身のドメイン辞書(yogo を例外として登録)
```

## ライセンス

MIT または Apache-2.0 のいずれか([LICENSE-MIT](LICENSE-MIT)、[LICENSE](LICENSE))。
