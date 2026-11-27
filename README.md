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
src/order/model.rs:5:14  warning  OrderMgr    mgr: 禁止語 manager の異表記。…  [denied-word]
src/order/model.rs:9:3   warning  shukka_date shukka: 未知語                   [unknown-word]
```

終了コード: エラーが1件以上なら 1、警告だけなら 0(`--max-warnings` を超えたら 1)、設定や辞書の誤りは 2。

## 規則

| 規則 | 報告すること | 既定の重大度 |
| --- | --- | --- |
| `denied-word` | 禁止語(manager、info など) | 辞書の項目ごと |
| `denied-pattern` | 禁止パターン(data1 のような連番など。識別子全体に照合する) | 辞書の項目ごと |
| `avoided-word` | 避ける同義語・展開すべき略語(retrieve → get、cnt → count) | 辞書の項目ごと |
| `unknown-word` | 未知語 | warning |
| `compound-word` | 連結語(savefile → save_file) | warning |
| `short-word` | `min-word-length` 未満の語 | warning |
| `function-first-word-part-of-speech` | 関数名の先頭語の品詞 | warning |
| `type-last-word-part-of-speech` | 型名の末尾語の品詞 | info |
| `value-last-word-part-of-speech` | 値の名前の末尾語の品詞 | info |
| `constant-last-word-part-of-speech` | 定数名の末尾語の品詞 | info |
| `function-noun-verb-order` | 名詞＋動詞の語順の関数名(user_delete → delete_user) | warning |
| `non-ascii-identifier` | 英字以外を含む識別子(検査しない) | info |
| `syntax-error` | 構文エラー内の定義(検査しない) | info |

## 設定 `yogolint.toml`

項目名は全て kebab-case で書く。

```toml
languages = ["rust"]
include = ["src/**"]
exclude = ["**/generated/**"]
kinds = ["function", "type", "value", "constant"]
builtin-dictionaries = ["verbs", "nouns", "abbrev", "deny"]   # 埋め込み辞書のうち使う層
dictionaries = ["dicts/domain.toml"]                          # 同じ層の埋め込み辞書を置き換える
allow-compound-words = false
min-word-length = 2
allow-single-letter = ["i", "j", "k", "n", "x", "y"]
ignore-words = []
ignore-paths = []

# 規則ごとの設定。重大度(error / warning / info / none)だけなら1行で書く。none で報告しない。
# denied-word・denied-pattern・avoided-word は、指定すると辞書の項目ごとの重大度を上書きする
[rules]
unknown-word = "warning"
function-noun-verb-order = "warning"

# 品詞の規則は表にして、重大度と認める品詞(allow)を書ける。省いた項目は言語の慣習に従う(下は Rust の既定値)
[rules.function-first-word-part-of-speech]
severity = "warning"
allow = ["verb", "prefix", "noun", "adjective"]

[rules.value-last-word-part-of-speech]
severity = "info"
allow = ["noun", "preposition"]   # started_at、引数の from / to
```

品詞は verb / prefix / noun / adjective / adverb / preposition / determiner / conjunction のいずれか。

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
