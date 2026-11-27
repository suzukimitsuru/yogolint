# 命名用語lint 最小構成設計書(tree-sitter + cspell辞書形式)

Nov 20, 2026 · @鈴木 充

## 目的と範囲

命名規則lintが書式(camelCaseなど)を保証した後段で、**定義名を構成する単語が用語辞書にあるか**だけを検査するCLIを作る。書式は検査せず、語彙だけを見る。名称は `yogolint` とする。

提供形態は **CLI** と **VSCode拡張(「問題」パネルへの報告)** の2つで、検査の中核は共通とする。実装言語は Rust とする(理由は「実装言語の比較」)。

- 対象: 関数・メソッド・クラス・インタフェース・型・変数・フィールド・引数・定数の**定義箇所**。
- 対象外: 参照箇所(外部ライブラリの名前を含む)、コメント、文字列リテラル。これらはcspellに任せる。
- 最小構成(第1段階)の言語: Rust のみ(yogolint 自身を検査できる)。第2段階で TypeScript、Python、Java を足す。他の言語はクエリファイルを足して増やす。

前段に置く命名規則lint(ここで書式は保証済みとみなす):

| 言語 | 前段の命名規則lint |
| --- | --- |
| TypeScript / JavaScript | @typescript-eslint/naming-convention |
| Python | Ruff の pep8-naming 規則(N) |
| Java | Checkstyle の命名チェック |
| Go | golangci-lint(revive の var-naming) |
| Rust | rustc の non\_snake\_case / non\_camel\_case\_types 警告 |
| C# | .editorconfig の命名規則 |

前段の無い言語では、後述の分割規則をそのまま使う。書式の乱れは分割結果の未知語として現れる。

## 全体構成

&#91;embedded content: termlint の処理の流れ · 5段 + 設定・辞書\]

処理は上から下へ一方向に流れ、辞書が効くのは④照合だけである。辞書はTOMLを原本とし、cspell用の .txt はそこから生成する。

## 定義名の抽出

tree-sitterの**タグ用クエリ(tags.scm)の流儀**をそのまま使い、足りない変数・引数・フィールドだけを自前の `names.scm` で補う。

- 多くのtree-sitter文法は `tags.scm` を同梱し、`@definition.function`・`@definition.class` などと名前の `@name` を捕捉する。これを再利用すれば、関数・クラス・メソッドのクエリは書かずに済む。
- 変数・引数・フィールド・定数は `tags.scm` に無いことが多い。`queries/<言語>/names.scm` に同じ捕捉名で追加する。
- 捕捉名から「種別(kind)」を決め、照合時の品詞位置の検査に使う。

| 捕捉名 | 種別 | 先頭語の期待 | 末尾語の期待 |
| --- | --- | --- | --- |
| @definition.function / .method | function | 動詞または述語接頭辞 | — |
| @definition.class / .interface / .type | type | — | 名詞 |
| @definition.variable / .field / .parameter | value | — | 名詞 |
| @definition.constant | constant | — | 名詞 |

`names.scm` の例(TypeScript):

```scheme
(variable_declarator name: (identifier) @name) @definition.variable
(required_parameter pattern: (identifier) @name) @definition.parameter
(public_field_definition name: (property_identifier) @name) @definition.field
```

- 実装はNode.js + `web-tree-sitter`(WASM文法)と Rust + tree-sitter(ネイティブ)のうち、Rust + tree-sitter を採る。クエリファイルは各文法のcrateと一緒にバイナリへ組み込む。
- 1ファイルを1回だけ構文解析し、全種別を取る。同じ位置の重複捕捉は1件にまとめる。
- 名前が外部で決まる定義は抽出しない。Rust では trait 実装(`impl Trait for X`)の中のメソッド・関連型・関連定数で、名前は trait 側で決まり、実装側は参照に当たる。trait の定義側(`trait Shape { fn area(&self); }`)は検査する。

## 単語分割

cspellと同じく「区切り文字 → 大小文字の境界 → 数字」の順に分割する。それでも辞書に無い塊だけを、辞書を使って連結語として分解する。

分割規則(上から順に適用):

1. 前後の `_` と `$` を除く(`__init__` → `init`)。
2. 辞書に数字込みで登録された語(`utf8`・`i18n`・`md5` など)を先に丸ごと取り出す。
3. 区切り文字 `_` `-` `$` `.` で分割する。
4. 小文字の後に大文字が来たら分割する(`parseJson` → parse / Json)。
5. 大文字の連続の後に「大文字+小文字」が来たら、最後の大文字の前で分割する(`HTMLInput` → HTML / Input)。
6. 数字は区切りとして捨てる(`camel2snake` → camel / snake)。
7. 全語を小文字にする。そ元の列位置は報告用に保持する。
8. 辞書に無い塊は、連結語として分解を試みる(下記)。

| 識別子 | 分割結果 |
| --- | --- |
| parseJson | parse / json |
| HTMLInput | html / input |
| getUtf8Octets | get / utf8 / octets |
| MAX\_RETRY\_COUNT | max / retry / count |
| **init** | init |
| savefileas | save / file / as(連結語) |

連結語の扱い:

- 前段の命名lintで書式は揃っているので、連結語は分割後の1つの塊の中にしか現れない。
- 全層の辞書を使って動的計画法で分解する(語数が最小、1語は2文字以上)。分解できたら連結語とみなす。
- 扱いは設定 `allow-compound-words` で決める。既定は `false` で、「区切って書く」よう警告する。
- 分解できない塊は未知語とする。

識別子の分割は研究上も完全な解法がなく、略語や固有名詞で誤ることがある([Spiral](https://www.theoj.org/joss-papers/joss.00653/10.21105.joss.00653.pdf))。誤分割は、その語を辞書に登録して直す運用とする。

## 辞書形式

**TOMLを唯一の原本**とし、品詞・定義・対語・避ける同義語・重大度を持たせる。cspell互換の `.txt` はTOMLから生成し、エディタの綴り検査と同じ語彙を共有する。

辞書は5層に分ける。

| 層 | ファイル例 | 内容 |
| --- | --- | --- |
| domain | dicts/domain.toml | ユビキタス言語。仕様書の日本語用語(ja)との対応も持つ |
| verbs | dicts/general-verbs.toml | 汎用動詞(付録A) |
| nouns | dicts/general-nouns.toml | 汎用・技術名詞(id, url, json, list など) |
| abbrev | dicts/abbrev.toml | 許す略語と、展開すべき略語(cnt → count) |
| deny | dicts/deny.toml | 禁止語と禁止パターン(付録B) |

TOMLの形式:

```toml
# dicts/general-verbs.toml
version = 1
layer = "verbs"

[[terms]]
word = "get"
pos = "verb"
definition = "手元にある値を返す。副作用なし"
pair = ["set"]
avoid = ["retrieve", "obtain"]
severity = "warning"    # avoid の語を使ったときの重大度

[[terms]]
word = "order"
pos = "noun"
ja = "注文"
definition = "顧客が確定した購入の申し込み"
forms = ["orders"]

[[deny]]
word = "manager"
pos = "noun"
reason = "責務が不明で、無関係な処理が集まる"
suggest = "責務を表す名詞(Repository, Pool など)"
avoid = ["mgr"]
position = "last"       # 末尾語のときだけ検査する
kinds = ["type"]
severity = "warning"

[[patterns]]
regex = '^[a-z]+\d+$'
reason = "連番は意図を表さない"
severity = "error"
```

生成されるcspell用 `.txt`(1行1語、先頭 `!` は禁止語):

```text
# generated from dicts/*.toml — 手で編集しない
get
set
order
orders
!retrieve
!manager
!mgr
```

- 生成は `yogolint build-dict` で行う。
- 対語(pair)と品詞(pos)は.txtには出ず、yogolintだけが使う。
- 同じ語が複数の層にあれば、denyが優先、次にdomainが優先する。

## 照合規則と重大度

1語ごとに「禁止 → 避ける同義語 → 許可層 → 連結語 → 未知語」の順に判定し、最初に当たった規則で結果を決める。禁止は常に許可より優先する。

照合前の正規化:

- 小文字にする。
- 原形に戻す。`forms` に登録した変化形を先に見て、次に規則変化(-s / -es / -ies / -ed / -ing)を外して試す。ただし語そのものが許可層にあれば、原形化した形で禁止語・避ける同義語を引かない(string を str、args を arg として扱わない)。
- 展開すべき略語(cnt → count)は、展開後の語を示して報告する。

判定順:

1. deny に一致 → 規則 `denied-word`、patterns に一致 → 規則 `denied-pattern`。重大度はその項目のもの。`position`・`kinds` の条件が合わなければ次へ進む。
2. いずれかの層の `avoid` に一致 → 規則 `avoided-word`。正規の語を提案し、その項目の重大度。ただし正規の語が動詞・述語接頭辞のもの(`new` → create など)は、function種別の先頭語のときだけ検査する。略語の展開(cnt → count)など名詞の同義語は位置を問わない(実装時に変更。下記「実装で設計書から変えた点」)。
3. 許可層(domain・verbs・nouns・abbrev)に一致 → 合格。
4. 連結語として分解できた → 設定 `allow-compound-words` に従う。許さなければ規則 `compound-word`(既定は warning)。
5. どれにも当たらない → 未知語として規則 `unknown-word`(既定は warning)。ローマ字の語(kingaku など)もここで捕まる。

許可層に無く `min-word-length` 未満の語は、`allow-single-letter` に無ければ連結語・未知語の判定の前に規則 `short-word`(既定は warning)で報告する。

`denied-word`・`denied-pattern`・`avoided-word` の重大度は辞書の項目ごとに決める。設定 `[rules]` で指定すると、全ての項目の重大度を上書きする(`none` で報告しない)。

品詞と語順の検査(5規則。重大度と認める品詞は設定 `[rules]` で変えられる):

| 規則 | 検査すること | 既定の重大度 | 認める品詞(Rust の既定値) |
| --- | --- | --- | --- |
| `function-first-word-part-of-speech` | function種別の**先頭語**の品詞 | warning | verb / prefix / noun / adjective |
| `type-last-word-part-of-speech` | type種別の**末尾語**の品詞 | info | noun |
| `value-last-word-part-of-speech` | value種別の**末尾語**の品詞 | info | noun / preposition |
| `constant-last-word-part-of-speech` | constant種別の**末尾語**の品詞 | info | noun |
| `function-noun-verb-order` | function種別の**名詞＋動詞の語順** | warning | — |

- 先頭語は `pos: verb` または述語接頭辞(is / has / can / should / to、Rust の変換慣習の from / as / into / with)にする。
  - 言語の慣習で広げる。Rust では getter をフィールド名で書き(API Guidelines の C-GETTER)、コンストラクタを `new`・`empty` のように書くので、名詞・形容詞で始まる関数名と、先頭語の `new` を許す。動詞だけに戻すには、`function-first-word-part-of-speech` の `allow` を `["verb", "prefix"]` にする。
- 末尾語は `pos: noun` にする。value種別は前置詞で終わってもよい(`started_at`、引数の `from` / `to`)。前置詞を許さないなら、`value-last-word-part-of-speech` の `allow` を `["noun"]` にする。
- 名詞＋動詞の語順: 先頭語が名詞(動詞・述語接頭辞を兼ねない語)で、2語目以降に動詞(名詞を兼ねない語)があれば報告し、動詞を先頭へ移した名前を置換候補に出す(`user_delete` → `delete_user`)。名詞で始まる関数名を許した代わりに、日本語の語順(ユーザー削除)をそのまま英語にした名前を捕まえる。`to` の後ろの動詞は不定詞(`worker_to_notify`)なので数えない。
- テスト関数(Rust では `#[test]`・`#[tokio::test]` などの属性が付いた関数)は、名前を文で書くので5規則とも検査しない。語彙の検査はする。
- 品詞は verb / prefix / noun / adjective / adverb / preposition / determiner / conjunction のいずれか。知らない品詞名は設定の誤り(終了コード 2)とする。
- 英字以外を含む識別子は規則 `non-ascii-identifier`(info)、構文エラー内の定義の件数は規則 `syntax-error`(info)で報告する。

規則名は「何を報告するか」を kebab-case の名詞で表す。品詞は part-of-speech と略さずに書く(pos は標準辞書で position の略として避ける語にしているため)。

| 重大度 | 意味 | 終了コード |
| --- | --- | --- |
| error | 必ず直す | 1 |
| warning | 原則直す。理由があれば抑止する | 0(`--max-warnings` で変更可) |
| info | 参考として示す | 0 |
| none | 報告しない | 0 |

抑止の方法:

- 行単位: `// yogolint-disable-next-line manager`(コメント記法は言語に合わせる)。
- 設定の `ignore-words`・`ignore-paths`。
- 既存コード: `yogolint baseline` で現状の違反を記録し、以後は新しい違反だけを報告する。CIでは `--diff origin/main` で変更した定義だけを検査する。

## 設定・CLI・出力

設定は `yogolint.toml` 1つ、コマンドは5つ、出力は text・JSON・SARIF の3形式とする。

```toml
# yogolint.toml(項目名は全て kebab-case)
languages = ["rust"]
include = ["src/**"]
exclude = ["**/generated/**"]
kinds = ["function", "type", "value", "constant"]
builtin-dictionaries = ["verbs", "nouns", "abbrev", "deny"]   # 埋め込み辞書のうち使う層
dictionaries = ["dicts/domain.toml"]
allow-compound-words = false
min-word-length = 2
allow-single-letter = ["i", "j", "k", "n", "x", "y"]
ignore-words = []
baseline = ".yogolint-baseline.json"

# 規則ごとの設定。重大度(error / warning / info / none)だけなら1行で書く。none で報告しない
[rules]
unknown-word = "warning"
function-noun-verb-order = "warning"

# 品詞の規則は表にして、重大度(severity)と認める品詞(allow)を書ける。省いた項目は言語の慣習に従う
[rules.function-first-word-part-of-speech]
severity = "warning"
allow = ["verb", "prefix", "noun", "adjective"]

[rules.value-last-word-part-of-speech]
severity = "info"
allow = ["noun", "preposition"]
```

- 判定の仕方を変えたいときは、コマンドの引数ではなく `yogolint.toml` の設定に書く。命名の方針はリポジトリで共有するもので、言語サーバー(VSCode)も同じファイルを読むため。引数は出力形式(`--format`)など実行ごとに変わるものに限る。
- 項目名は、規則名と同じく kebab-case で揃える。Rust のツールの設定(`Cargo.toml` の `rust-version`、`clippy.toml` の `avoid-breaking-exported-api`)に合わせる。
- `allow` を書けるのは品詞の規則だけで、他の規則に書けば設定の誤りとする。

| コマンド | 働き |
| --- | --- |
| yogolint check \[paths\] | 検査する。`--format text/json/sarif`、`--diff <ref>` |
| yogolint words \[paths\] | 未知語を出現回数の多い順に一覧する。ドメイン辞書づくりの起点 |
| yogolint build-dict | TOML辞書からcspell用 .txt を生成する |
| yogolint baseline | 現状の違反を記録する |
| yogolint dicts export | 埋め込みの標準辞書をTOMLファイルに書き出す。--out、--layer、--force |

text出力の例:

```text
src/order/service.ts:12:10  warning  getOrderInfo    info: 曖昧語。何の情報かを名前にする      [denied-word]
src/order/service.ts:30:7   warning  retrieveOrders  retrieve → get を使う                    [avoided-word]
src/order/model.ts:5:14     warning  OrderMgr        mgr: 禁止語 manager の略記               [denied-word]
src/order/model.ts:9:3      warning  shukkaDate      shukka: 未知語                           [unknown-word]
```

- SARIFはGitHubのコードスキャン画面にそのまま載る。
- JSONはエディタ連携や自作ツール用とする。

## VSCode連携(「問題」パネルへの報告)

検査の中核を1つにして、CLIと言語サーバー(LSP)の2つの入口から呼ぶ。VSCode拡張は言語サーバーを起動するだけの薄いクライアントにする。

- 中核(core): 抽出・分割・正規化・照合を行い、`Finding`(ファイル、範囲、語、規則、重大度、メッセージ、置換候補)の列を返す。ファイル入出力を持たない。
- CLI: ファイルを読み、core の結果を text・JSON・SARIF に整形する。
- 言語サーバー `yogolint lsp`: 開いた文書の内容を受け取り、core の結果を `textDocument/publishDiagnostics` で送る。VSCodeはこれを「問題」パネルと波線で表示する。
- VSCode拡張: `vscode-languageclient` で `yogolint lsp` を起動する。設定は `yogolint.toml` を共有し、拡張の設定は実行ファイルのパスと有効・無効だけにする。

VSCodeのAPIを直接使わずLSPにするのは、同じサーバーをNeovimなど他のエディタでも使え、Rustで実装した場合も構成が変わらないためである。

| Finding の項目 | LSP の Diagnostic |
| --- | --- |
| 範囲 | range(識別子のうち該当する語の部分だけ) |
| 重大度 error / warning / info | severity Error / Warning / Information(none は送らない) |
| 規則(denied-word、unknown-word など。「照合規則と重大度」の各規則) | code |
| — | source は常に yogolint |
| 置換候補 | メッセージに表示。クイックフィックス(codeAction)は将来の拡張 |

再検査の契機:

- 文書が変わるたび(didChange、300ms程度まとめて)、その文書だけを再検査する。tree-sitterの増分構文解析が使える。
- `yogolint.toml` と辞書TOMLの変更を監視し、変わったら開いている全文書を再検査する。
- ワークスペース全体の検査はCLIに任せ、言語サーバーは開いている文書だけを扱う。
- ベースラインに記録済みの違反は、エディタでは情報として表示する(CIでは報告しない)。

## 実装言語の比較

**Rust で実装する(2026年11月20日決定)。** CIで大きなリポジトリを毎回検査するなら、解析速度・並列化・単一バイナリ配布の差が効く。代わりに、VSCode拡張にはOS・CPUごとのバイナリを同梱する手間が増える。VSCode中心で小中規模のリポジトリだけなら、Node.js + web-tree-sitter でも十分に成り立つ。

| 観点 | Node.js + web-tree-sitter | Rust + tree-sitter |
| --- | --- | --- |
| 構文解析の速度 | WASMのため、Node.jsのネイティブ束縛より相当遅い(公式READMEの注意書き) | ネイティブで最速 |
| 並列処理 | 単一スレッド。並列化は worker\_threads ごとに文法を読み込む | ファイル単位で素直に並列化できる(rayon) |
| 文法の組み込み | 各文法の .wasm を同梱。tree-sitter.wasm と版を揃える必要がある | 各文法のcrateを依存に足し、静的リンクする |
| CLIの配布 | npm。Node.jsの実行環境が必要 | 単一バイナリ(cargo / Homebrew / GitHub Releases) |
| VSCode拡張の配布 | 中核を拡張の中で動かせる。全OS共通のVSIX 1つ | 言語サーバーのバイナリをOS・CPUごとに作り、プラットフォーム別VSIXで同梱する |
| 言語サーバーの部品 | vscode-languageserver(成熟) | tower-lsp 系、または lsp-server |
| 開発の速さ | 速い。拡張と同じ言語で書ける | 初期は遅い。型と所有権で設計の誤りは早く出る |
| 先例 | 多くのVSCode拡張 | ast-grep(Rustの中核 + LSP + VSCode拡張 + Node向けnapi束縛)、rust-analyzer |

Rustでの構成:

```text
yogolint/
  crates/yogolint-core   抽出・分割・正規化・照合(入出力なし)
  crates/yogolint-cli    check / words / build-dict / baseline / lsp を持つ単一バイナリ
  editors/vscode         TypeScript の薄いクライアント(vscode-languageclient)
  queries/<言語>/names.scm
```

参考: Node.jsでの構成(不採用):

```text
yogolint/
  packages/core     抽出・分割・正規化・照合
  packages/cli      CLI と lsp サブコマンド
  packages/vscode   拡張。LSP経由でも core の直接呼び出しでもよい
  queries/<言語>/names.scm
```

どちらでも、クエリファイル・辞書TOML・設定ファイル・LSPの外向き仕様は同じになる。後から言語を移しても、利用者側は変わらない。

## 名称 yogolint について

正式名を `yogolint` とする(2026年11月20日決定)。日本語「用語(yōgo)」のローマ字 + lint で、用語辞書で定義名を検査するツールであることを表す。

**命名の悪い例として、意図的に採用する。** 本ツールの方針では、ローマ字の語は未知語として検出される。そのため自身のリポジトリでは、`yogolint` や `YogolintConfig` のような定義名をツール自身が未知語として報告する。これを導入時の実演と、例外登録の手本に使う。

- 綴りはヘボン式から長音記号を省いた形(yōgo → yogo)とする。かな入力式の yougolint は、英語圏で「you go」と読まれるので採らない。
- READMEの冒頭に「用語(yōgo)+ lint。名前そのものが、本ツールが検出する悪い命名の例である」と記す。
- 自リポジトリの `dicts/domain.toml` に `yogo` を登録し、`definition` に例外にした理由を残す。

| 公開先 | yogolint の状況(2026年11月20日) |
| --- | --- |
| npm | 未登録 |
| crates.io | 未登録 |
| PyPI | 未登録 |
| GitHub | 該当なし |
| VSCode Marketplace | 該当なし |

検討した他の名称: termlint(PyPIに目的の近い同名ツールあり)、termslint(termlint と1文字違い)、symbollint(用語辞書の意味が伝わらない)、glosslint・horolint・yougolint。

## 付録A 汎用動詞辞書

44語を対で揃える。語そのものは許可で、「重大度」は**避ける同義語を使ったとき**の重大度を表す。同義語を置かない考え方はPowerShellの承認動詞に倣う。

| 語 | 品詞 | 定義 | 対語 | 避ける同義語 | 重大度 |
| --- | --- | --- | --- | --- | --- |
| get | 動詞 | 手元にある値・属性を返す。副作用なし、失敗しない | set | retrieve, obtain | 警告 |
| set | 動詞 | 値を丸ごと置き換える | get | assign | 警告 |
| find | 動詞 | 条件に合う1件を探す。無ければ null や空を返す | — | locate, lookup | 警告 |
| search | 動詞 | 条件に合う複数件を探し、集合を返す | — | seek | 情報 |
| fetch | 動詞 | ネットワークなど外部から取り込む。遅延・失敗あり | — | download, pull | 警告 |
| load | 動詞 | ファイル・DBなどの保存先から読み込む | save | — | — |
| save | 動詞 | 保存先へ書き出して永続化する | load | persist, store | 警告 |
| read | 動詞 | ストリームなどから順に読む | write | — | — |
| write | 動詞 | ストリームなどへ順に書く | read | — | — |
| create | 動詞 | 新しい資源を生成して返す | delete | make, generate, new | 警告 |
| build | 動詞 | 部品を組み立てて作る(Builder) | — | construct, assemble | 情報 |
| copy | 動詞 | 同じ内容の別物を作る | — | clone, duplicate | 情報 |
| add | 動詞 | 入れ物に要素を加える(位置は問わない) | remove | — | — |
| remove | 動詞 | 入れ物から要素を外す。要素自体は残る | add | eliminate, drop | 警告 |
| insert | 動詞 | 位置を指定して入れる | — | — | — |
| delete | 動詞 | 資源そのものを消す(永続データを含む) | create | erase, destroy, kill | 警告 |
| update | 動詞 | 既存の資源を部分的に変える | — | modify, change, edit, alter | 警告 |
| clear | 動詞 | 中身を空にする。入れ物は残る | — | wipe, purge | 警告 |
| reset | 動詞 | 初期状態に戻す | — | reinit | 情報 |
| init | 動詞 | 初期化して使える状態にする | dispose | initialize, setup | 警告 |
| dispose | 動詞 | 保持している資源を解放する | init | release, free | 警告 |
| open | 動詞 | 接続やハンドルを得て使える状態にする | close | — | — |
| close | 動詞 | 接続やハンドルを閉じる | open | shut | 情報 |
| start | 動詞 | 処理・タスクを開始する | stop | begin, launch | 情報 |
| stop | 動詞 | 処理・タスクを止める | start | halt | 情報 |
| enable | 動詞 | 機能を有効にする | disable | activate | 情報 |
| disable | 動詞 | 機能を無効にする | enable | deactivate | 情報 |
| lock | 動詞 | 排他を得る | unlock | acquire | 情報 |
| show | 動詞 | 表示する | hide | display | 情報 |
| attach | 動詞 | 対象に付ける・関連づける | detach | — | — |
| register | 動詞 | 一覧や仕組みに登録する | unregister | enroll | 情報 |
| subscribe | 動詞 | 通知を購読する | unsubscribe | listen | 情報 |
| push | 動詞 | スタックの端に積む | pop | — | — |
| send | 動詞 | 相手へ送る | receive | transmit | 情報 |
| parse | 動詞 | 文字列を構造へ変える | format | — | — |
| format | 動詞 | 構造を文字列へ変える | parse | stringify | 情報 |
| to | 前置詞 | 型や形式を変える(toXxx の形で使う) | — | convert, transform | 情報 |
| validate | 動詞 | 検証し、不正なら例外やエラー結果を返す | — | verify, check | 警告 |
| compute | 動詞 | 入力から値を計算する。副作用なし | — | calc, calculate | 警告 |
| apply | 動詞 | 設定や変更を対象に反映する | revert | — | — |
| run | 動詞 | 処理を実行する | — | execute, exec, perform | 情報 |
| notify | 動詞 | 変化を相手に知らせる | — | inform | 情報 |
| is | 述語接頭辞 | 状態の真偽を返す(isEmpty) | — | — | — |
| has / can / should | 述語接頭辞 | 所有・可否・要否の真偽を返す(hasChildren, canEdit) | — | — | — |

create と new、init と initialize はどちらを正にするか組織ごとに決め、この表の「語」と「避ける同義語」を入れ替えて使う。

## 付録B 禁止語辞書

曖昧語は警告、紛らわしい命名はエラー、配慮語は方針次第とする。付録Bでは「定義」に**禁止の理由**、「対語」に**置換候補**を入れる(TOMLでは `reason`・`suggest`)。「避ける同義語」は同じ理由で禁止する異表記である。

| 語 | 品詞 | 定義(禁止の理由) | 対語(置換候補) | 避ける同義語 | 重大度 |
| --- | --- | --- | --- | --- | --- |
| manager | 名詞 | 「管理するもの」以上の意味がなく、無関係な処理が集まる | 責務を表す名詞(Repository, Pool, Registry) | mgr, management | 警告(型名の末尾) |
| helper | 名詞 | 何でも「助け」になり、責務が散らばる | 操作対象の型へメソッドを移す | helpers | 警告 |
| util | 名詞 | 雑多な関数の置き場になる | 機能ごとのモジュール名 | utils, utility, misc | 警告 |
| processor | 名詞 | 何を処理するか語らない | 処理内容を表す名詞(Parser, Validator) | processing | 警告 |
| data | 名詞 | Product と ProductData の違いが分からない雑音語 | 取り除く、または中身を表す語 | — | 警告(末尾) |
| info | 名詞 | data と同じ雑音語 | 取り除く、または中身を表す語 | information | 警告 |
| object | 名詞 | すべてが当てはまり区別にならない | 型の名前そのもの | obj, thing, stuff | 警告 |
| item | 名詞 | 要素であること以外を語らない | 要素の種類を表す名詞 | — | 情報 |
| base | 名詞 | 継承関係を名前に持ち込む | 抽象の意味を表す名詞(Shape) | — | 情報 |
| common | 形容詞 | 雑多な共有物の置き場になる | 機能名 | shared, general | 警告 |
| temp | 形容詞 | 何の一時値か語らない | 値の意味を表す名詞 | tmp | 警告(ローカル変数は情報) |
| flag | 名詞 | 何が真なのか語らない | isXxx, hasXxx | — | 警告 |
| do | 動詞 | 何をするか語らない | 付録Aの具体的な動詞 | process(動詞として) | 警告 |
| handle | 動詞 | いつ呼ばれるかを示すだけで、何をするか語らない | 処理内容を表す動詞 | on(接頭辞) | 情報 |
| foo | 名詞 | 意味のない仮名 | 意味のある名前 | bar, baz, qux, hoge, fuga, piyo | エラー |
| (連番) | パターン | data1, a2 のような連番は意図を表さない | 違いを表す語 | — | エラー |
| (1文字) | パターン | 意図を表さない | 意味のある名前 | — | 警告(i, j, k, n, x, y は許可) |
| klass | 名詞 | 予約語を避けた綴り換えで紛らわしい | 対象を表す語 | clazz | 情報 |
| str | 接頭辞 | 型を名前に重ねるハンガリアン記法。型は型システムが示す | 取り除く | int, arr, sz, lp, psz | 警告(先頭語) |
| whitelist | 名詞 | 白=良・黒=悪の比喩を持ち込む | allowlist | white-list | 警告(方針次第) |
| blacklist | 名詞 | 同上 | denylist, blocklist | black-list | 警告(方針次第) |
| master | 名詞 | master/slave の比喩 | primary, main | — | 警告(方針次第) |
| slave | 名詞 | 奴隷制の比喩 | replica, secondary, worker | — | 警告(方針次第) |
| sanity | 名詞 | 精神状態を比喩に使う | confidence, coherence | — | 情報(方針次第) |
| dummy | 名詞 | 配慮語とする指針がある | placeholder, stub | — | 情報(方針次第) |

運用上の注意:

- 曖昧語でも設計パターン名として定着したもの(Factory, Repository, EventHandler)や、React の onClick のようなフレームワークの慣習名は、`ignore-words` や `kinds` で外す。
- 業務システムの「マスタ(商品マスタなど)」は master/slave と別義である。禁止は許可より優先するので、domain層に `master` を登録しても禁止は外れない。`ignore-words` に入れるか、`dicts export` で書き出した deny 辞書から master の行を外す。
- 配慮語の置き換えには業界でも賛否がある。採用するかは組織の方針で決める。
- ローマ字の語はこの表ではなく、未知語として検出する。

## 最小構成の決定事項

2026年11月20日に決定。第1段階は Rust のコードだけを対象に、CLIの `check`・`words`・`dicts export` を作る。

| 項目 | 決定 |
| --- | --- |
| 範囲 | 第1段階: CLI の check・words・dicts export。第2段階: LSP(VSCode)、baseline、SARIF、build-dict |
| 対象言語 | 第1段階は Rust のみ(yogolint 自身を検査できる)。第2段階で TypeScript・Python・Java |
| 完成の判定 | 実在するコードベース1つに words を掛け、未知語の数と誤検出の内訳を記録する。2026年11月27日に、分野の違う複数のリポジトリで誤検出率 20% 以下を第1段階を終える基準とした(暫定。今後の検討で変わりうる) |
| 辞書・設定の形式 | TOML(保守が終わった serde\_yaml への依存を避ける) |
| 標準辞書 | バイナリに埋め込む。`yogolint dicts export` でファイルに書き出せる(下記) |
| tree-sitter | 本体と tree-sitter-rust の版を固定。同梱の tags.scm が使えるかを最初に確かめる |
| リポジトリ | suzukimitsuru/yogolint、MIT と Apache-2.0 の併用、edition 2024 |
| 構文エラー | ERRORノード内の定義は検査せず、件数だけ報告する |
| 英字以外の識別子 | 検査せず、情報として報告する |
| 照合 | 小文字で照合。原形化は -s / -es / -ies / -ed / -ing と forms のみ。連結語は警告 |
| 終了コード | エラーが1件以上なら1、警告だけなら0 |

標準辞書の埋め込みと書き出し:

- 標準辞書(verbs・nouns・abbrev・deny の4層)は `include_str!` でバイナリに埋め込む。辞書ファイルを置かなくても動く。
- `yogolint dicts export [--out dicts/] [--layer verbs,deny] [--force]` で、埋め込み辞書をTOMLファイルに書き出す。既定の出力先は `dicts/`。同名のファイルがあれば、`--force` が無い限り上書きしない。
- 書き出したファイルの先頭に、yogolint の版と「標準辞書から書き出した」旨のコメントを入れる。版を上げたときの差分の確認に使う。
- 同じ層のファイルを `dictionaries` に指定すると、その層の埋め込み辞書は使わない(結合せず置き換える)。書き出して編集し、指定する、という手順で標準辞書を作り変えられる。
- 埋め込み辞書を使わない層は、`builtin-dictionaries` から外す。

## 実装で設計書から変えた点

2026年11月25日、第1段階の実装時に決めた。

| 項目 | 設計書の記述 | 実装 | 理由 |
| --- | --- | --- | --- |
| 避ける同義語の検査位置 | 位置を問わず報告する | 正規の語が動詞・述語接頭辞なら、function種別の先頭語のときだけ報告する。名詞の同義語・略語の展開は位置を問わない | `new_value` の形容詞 new、名詞の store・change・display などまで報告されるため |
| 関数名の先頭に置ける語 | 動詞と is / has / can / should | to(付録Aの表どおり)と from / as / into / with を述語接頭辞(`pos = "prefix"`)として verbs 層に加えた | Rust の変換関数の命名慣習(from_str、as_bytes、into_iter、with_capacity) |
| 禁止語と domain 層 | 「禁止は常に許可より優先」と、付録Bの「domain層に master を登録する」が食い違う | 禁止を優先する。付録Bの注意書きを `ignore-words` か deny 辞書の行を外す運用に改めた | 照合規則の原則を優先した |
| 規則の追加 | deny / avoid / unknown / compound / position | `short`(minWordLength 未満の語、warning)、`non-ascii`(英字以外を含む識別子、info)、`syntax`(構文エラー内の定義の件数、info)を加えた。のちに `short-word`・`non-ascii-identifier`・`syntax-error` に改名した | 付録Bの(1文字)と、決定事項の「英字以外の識別子」「構文エラー」を報告する規則名が無かったため |
| 禁止パターンの照合対象 | 語ごとの判定に含む | 識別子全体(前後の `_` を除き小文字にしたもの)に照合する。辞書に数字込みで登録した語(utf8 など)だけの識別子は除く | 数字は分割で捨てられ、語単位では data1 のような連番を検出できないため |
| 標準の verbs 辞書 | 付録Aの44語 | 対語(unlock・hide・detach・unregister・unsubscribe・pop・receive・revert)を加えた。名詞としても使う語(start・stop・format・set など)は `pos = ["verb", "noun"]` とした | 対語が未知語になり、値の名前の末尾で品詞位置の誤検出が出るため |
| 辞書の品詞 | `pos` は1つ | 文字列か配列のどちらでもよい | 動詞と名詞を兼ねる語のため |
| 連結語の分解 | 語数が最小 | 語数が同じなら語長の二乗和が大きい分け方を採る | savefileas が save / fil / eas に分かれるのを防ぐ |
| 抑止コメント | 語を指定する | 語を省くと、次の行の全ての報告を抑止する | 一時的な抑止を書きやすくするため |
| 終了コード | エラー1件以上で1 | 設定・辞書の誤りは2 | 検査結果と実行失敗を区別するため |
| trait 実装の扱い | 定義箇所を検査する | trait 実装の中のメソッド・関連型・関連定数を抽出しない | 名前は trait 側で決まる参照で、完成判定で誤検出の最大要因(1,320 件)だったため |
| 原形化と禁止・避ける同義語 | 原形化した形で照合する | 語そのものが許可層にあれば、原形化した形で禁止語・避ける同義語を引かない | args → arg(avoid)、string → str(deny)の誤検出(716 件)のため |
| 関数名の先頭語(言語の慣習) | 動詞か述語接頭辞 | 言語ごとに広げる。Rust は名詞・形容詞と先頭語の new を許す | Rust の getter・コンストラクタの慣習(誤検出 361 件)のため。これで自リポジトリの `ignore-words = ["new"]` も不要になった |
| テスト関数 | — | 品詞位置を検査しない | テスト名は文で書くため(誤検出 189 件) |
| 語順の規則 | — | 規則 `function-noun-verb-order` を加えた(関数名が名詞で始まり、後ろに動詞がある) | 名詞で始まる関数名を許すと、`user_delete` のような日本語の語順の名前を見逃すため(2026年11月27日に決定。今後の検討で変わりうる) |
| 値の名前の末尾語 | 名詞 | value種別は前置詞で終わってもよい。to / from / as / into / with の品詞に preposition を加えた | `started_at`、引数の `from` / `to` の慣用名(誤検出 57 件) |
| 規則名 | deny / avoid / unknown / compound / position | 「何を報告するか」を kebab-case の名詞で表す名前に改めた。deny は禁止語(`denied-word`)と禁止パターン(`denied-pattern`)に、position は関数名の先頭語と、型・値・定数の末尾語の4規則に分けた(下の対応表) | 動詞(deny・avoid)や形容詞(unknown・short)だけでは報告の中身を表さず、品詞の規則名に「品詞」が入っていなかったため。分けた規則は別々に重大度を設定できる(2026年11月27日) |
| 規則の設定 | `unknownWord` で未知語の重大度だけを変えられる | `[rules]` で規則ごとの重大度を、品詞の規則では認める品詞(`allow`)も変えられる | 言語の慣習で緩めた規則(名詞で始まる関数名、末尾の前置詞)を、リポジトリの方針で厳しくも緩くもできるようにするため(2026年11月27日) |
| 設定の項目名 | camelCase(`allowCompoundWords`。cspell の設定名に合わせていた) | kebab-case(`allow-compound-words`) | `[rules]` の規則名(kebab-case)と1つのファイルで混ざるため。`yogolint.toml` は cspell に読まれないので、cspell に合わせる利点が無い。未公開のうちに書き方を改めた(2026年11月27日) |

## 第1段階の完成判定

この節と次の節の規則名は、計測したときの名前(deny・avoid・unknown・compound・position・short)で記す。今の名前との対応:

| 計測時 | 今 |
| --- | --- |
| deny | denied-word、denied-pattern(連番など) |
| avoid | avoided-word |
| unknown | unknown-word |
| compound | compound-word |
| position | function-first-word-part-of-speech、type- / value- / constant-last-word-part-of-speech |
| short | short-word |
| word-order | function-noun-verb-order |

2026年11月26日に実施。「実在するコードベース1つに words を掛け、未知語の数と誤検出の内訳を記録する」の結果を記す。

### 対象と条件

| 項目 | 内容 |
| --- | --- |
| 対象 | [ripgrep](https://github.com/BurntSushi/ripgrep) 16.0.0(コミット 669cb4c)。`.rs` 105 ファイル、58,485 行 |
| 抽出した定義 | 10,148 件(function 3,034、type 435、value 6,322、constant 357)。うち trait 実装のメソッド 1,025 件、`#[test]` 関数 552 件 |
| 設定 | 既定値(`yogolint.toml` なし)。標準辞書の4層だけを使い、ドメイン辞書は置かない |
| コマンド | `yogolint check --format json`、`yogolint words --format json`(release ビルド) |
| 所要時間 | 約 0.5 秒(Apple Silicon の MacBook、単一スレッド) |

実施中に、数字込みの辞書語(`utf8` など)で終わる識別子で分割処理が範囲外を読んで落ちる不具合が見つかった。これを直してから計測した(`split.rs`、回帰テストを追加)。

### 結果

報告は 8,985 件(エラー 78、警告 7,212、情報 1,695)。

| 規則 | 件数 |
| --- | --- |
| unknown | 5,663 |
| avoid | 1,678(info 1,538、warning 140) |
| position | 829(warning 688、info 141) |
| short | 478 |
| deny | 193(error 78、warning 99、info 16) |
| compound | 144 |

**未知語は 743 語、出現 5,663 回。** trait 実装のメソッド名に出るものを除くと 733 語・4,983 回で、内訳は次のとおり。

| 区分 | 語数 | 出現数 | 例 |
| --- | --- | --- | --- |
| 辞書不足(英単語) | 469 | 3,329 | matcher, printer, haystack, expected, searcher, binary, separator |
| 辞書不足(技術用語・固有名詞・機能語) | 117 | 712 | glob, stdin, stdout, symlink, gitignore, unicode, mmap, it, if |
| 正しい検出(略語・略語の連結・誤字) | 147 | 942 | rdr, wtr, td, ig, pat, linebuf, bufwtr, uft(utf の誤字) |

### 誤検出の内訳

全 8,985 件を1件ずつ次の区分に分けた。**正しい検出は 2,161 件(24%)、誤検出は 6,824 件(76%)。**

| 区分 | 件数 | 主な原因(件数) |
| --- | --- | --- |
| 辞書不足 | 4,056 | 一般的な英単語 3,329、技術用語・固有名詞・機能語 712、数字込みの技術用語(PCRE2, sse2, avx2, u64)が連番パターンに当たる 15 |
| 規則の誤り | 1,368 | 原形化で args → arg が avoid に当たる 705、Rust の getter・コンストラクタ名の先頭が名詞 261、テスト関数名の先頭が動詞でない 189、Rust のコンストラクタ new 100、前置詞で終わる慣用名(started_at、from / to)57、CLI のフラグ(flag)を禁止 37、原形化で string → str が禁止語に当たる 10、1文字の a が冠詞として許可され short にならない 6、連番パターンが issue 番号(regression_106)に当たる 2、lens → len 1 |
| 抽出誤り | 1,320 | trait 実装のメソッド名(fmt, next, from_str など)。名前は trait 側で決まり、実装側は参照に当たる |
| 分割誤り | 80 | 辞書に無い英単語を連結語として分解した(terminator → term / in / at / or、append、stepper、nothing、overflow) |
| (正しい検出) | 2,161 | 略語・略語の連結・誤字 942、避ける同義語・略語の展開(err, dir, buf, len)478、1文字の名前 478、禁止語・連番(ig0, index1, whitelist, data)124、末尾が名詞でない 78、連結語(filesize, hostname, lineno)61 |

分類の方法:

- 未知語は macOS の英語辞書(`/usr/share/dict/words`)に原形が載るかで英単語かを判定し、略語(pat, caps, iter など)は目視で正しい検出へ移した。
- trait 実装のメソッドと `#[test]` 関数は、tree-sitter で構文木を辿って判定した。
- 「正しい検出」は設計書の方針どおりの報告を指す。err・buf・len や1文字の名前は Rust では慣用だが、方針どおりなので正しい検出に数えた。
- 大小文字・区切り文字・数字による分割の誤りは見つからなかった。分割誤りはすべて連結語の分解による。

### 判定と改善の優先順位

第1段階の機能は全て動き、58,000 行を 0.5 秒で検査できた。一方で**誤検出が 76% あり、このままでは実用にならない。** 第2段階へ進む前に、次の改善を行う。

| 順 | 改善 | 減る誤検出 |
| --- | --- | --- |
| 1(済) | trait 実装(`impl Trait for X`)のメソッドを抽出から外す。外部の名前は対象外とする方針に合う | 1,320 |
| 2(済) | 原形化の修正。語そのものが許可層にあれば、原形化した形で avoid・deny を引かない | 716 |
| 3(済) | 言語ごとの品詞規則。Rust では名詞で始まる関数名(getter・コンストラクタ)と `new` を許し、テスト関数は品詞位置の検査から外す | 550 |
| 4 | 標準辞書を広げる。一般的な英単語、技術用語(stdin, glob, symlink, unicode)、数字込みの語(pcre2, sse2, avx2, u64, utf16)を足す。一般の英単語は連結語として分解しない | 4,136 の大部分 |
| 5 | 前置詞で終わる慣用名(_at、from / to)の扱いと、1文字の冠詞 a を short にすることを決める。前置詞は value の名前の末尾で許すことにした(2026年11月27日に実施) | 63 |

1〜3 は規則と抽出の直しで済み、合わせて約 2,600 件(誤検出の 38%)が減る見込み(2026年11月27日に実施。結果は次の「改善1〜3の後の再計測」)。4 は辞書の量の問題で、どこまで標準辞書に持たせ、どこからドメイン辞書に任せるかを決める必要がある。flag のような文脈で意味が変わる禁止語(37 件)は、ドメイン辞書で禁止を外せるようにするか(付録B の master と同じ問題)と合わせて決める。

### 改善1〜3の後の再計測

2026年11月27日、改善1〜3を入れて同じ条件で計測し直し、同じ基準で分類した。

| 項目 | 改善前 | 改善後 |
| --- | --- | --- |
| 抽出した定義 | 10,148 | 9,085(trait 実装のメソッド 1,025 件と関連型・関連定数 38 件を外した) |
| 報告 | 8,985 | 6,397 |
| 未知語 | 743 語・5,663 回 | 735 語・4,980 回 |
| 正しい検出 | 2,161(24%) | 2,154(34%) |
| 誤検出 | 6,824(76%) | 4,243(66%) |
| 辞書不足 | 4,056 | 4,050 |
| 規則の誤り | 1,368 | 113 |
| 抽出誤り | 1,320 | 0 |
| 分割誤り | 80 | 80 |
| 所要時間 | 約 0.5 秒 | 約 0.5 秒 |

残った規則の誤り 113 件の内訳:

| 原因 | 件数 | 対応 |
| --- | --- | --- |
| 前置詞で終わる慣用名(started_at、from / to) | 57 | 改善5 |
| CLI のフラグ(flag)を禁止 | 37 | 文脈で意味が変わる禁止語の扱い(付録Bの master と同じ問題) |
| 副詞・前置詞・接続詞で始まる getter・コンビネータ名(only_matching、per_match、and_then、or) | 10 | getter はフィールド名と同じなので、Rust では先頭語の品詞を問わないかを検討する |
| 1文字の a が冠詞として許可され short にならない | 6 | 改善5 |
| 連番パターンが issue 番号(regression_106)に当たる | 2 | 数字の前に `_` がある名前をパターンから外すかを検討する |
| 原形化で lens → len が avoid に当たる | 1 | 辞書不足(lens を英単語として足せば消える) |

注:

- 改善前の分類では、trait 実装の関連型(`type Err`・`type Item`)の報告を「正しい検出」に数えていた(約 10 件)。改善後はこれらを抽出しないので、正しい検出が 2,161 件から 2,154 件に減った。
- 誤検出の 95%(4,050 件)は辞書不足になった。残りの改善は辞書の方針(改善4)が中心になる。

### 分野の違う複数のリポジトリでの計測

2026年11月27日、ripgrep だけでは偏りがあるため、分野の違う3つのリポジトリを加えて計測した。語順の規則と、値の名前の末尾の前置詞の許可を入れた版で測った。第1段階を終える基準は、**誤検出率 20% 以下**とする(暫定)。

| リポジトリ | 分野 | 版(コミット) | `.rs` ファイル | 行 | 所要時間 |
| --- | --- | --- | --- | --- | --- |
| [ripgrep](https://github.com/BurntSushi/ripgrep) | CLI の検索ツール | 16.0.0(669cb4c) | 105 | 58,485 | 0.56 秒 |
| [tokio](https://github.com/tokio-rs/tokio) | 非同期実行基盤・ネットワーク | tokio-1.54.1(b53b2001) | 811 | 188,485 | 0.99 秒 |
| [alacritty](https://github.com/alacritty/alacritty) | GUI の端末エミュレータ | v0.17.0(94e7c88) | 88 | 33,696 | 0.18 秒 |
| [image](https://github.com/image-rs/image) | 画像の符号化・復号 | v0.25.10(76e57184) | 114 | 39,544 | 0.22 秒 |

条件は第1段階の完成判定と同じ(既定の設定、標準辞書の4層だけ、release ビルド、Apple Silicon の MacBook)。

| 区分 | ripgrep | tokio | alacritty | image | 合計 |
| --- | --- | --- | --- | --- | --- |
| 報告 | 6,322 | 26,047 | 5,097 | 7,315 | 44,781 |
| 正しい検出 | 2,114 | 9,630 | 1,085 | 2,562 | 15,391 |
| 辞書不足 | 4,070 | 15,913 | 3,847 | 4,631 | 28,461 |
| 規則の誤り | 58 | 331 | 58 | 90 | 537 |
| 分割誤り | 80 | 173 | 107 | 32 | 392 |
| 抽出誤り | 0 | 0 | 0 | 0 | 0 |
| **誤検出率** | **67%** | **63%** | **79%** | **65%** | **66%** |
| 辞書不足を除いた誤検出率 | 6% | 5% | 13% | 5% | — |

未知語:

| リポジトリ | 未知語 | 出現 |
| --- | --- | --- |
| ripgrep | 735 語 | 4,980 回 |
| tokio | 1,376 語 | 19,913 回 |
| alacritty | 794 語 | 4,194 回 |
| image | 910 語 | 5,424 回 |
| 4つを合わせて | 2,788 語 | 34,511 回 |

| 出るリポジトリの数 | 語数 | 出現 | 例 |
| --- | --- | --- | --- |
| 4つ全て | 67 | 4,712 | inner, try, panic, expected, block, reader, writer, after, before, decoder, now, zero |
| 3つ | 182 | 6,534 | |
| 2つ | 462 | 9,926 | |
| 1つだけ | 2,077 | 13,339 | |

規則の誤りの内訳(4つの合計):

| 原因 | 件数 | 主な出どころ |
| --- | --- | --- |
| 1文字の a が冠詞として許可され short にならない | 209 | tokio 131、image 65 |
| 文脈で意味が変わる禁止語(CLI・OS のフラグの flag / flags、端末の master、UI の bar、tokio の Handle) | 138 | 全て |
| Rust 標準の用語が避ける同義語に当たる(drop → remove、clone → copy、acquire → lock、release → dispose、seek、store) | 140 | tokio 129 |
| 副詞・前置詞で始まる getter・コンビネータ名(in_bounds、no_limits、and_then、for_each) | 37 | 全て |
| 連番パターンが意味のある数字に当たる(SelectTab1〜9、regression_106) | 12 | alacritty 9 |
| 原形化で lens → len が avoid に当たる | 1 | ripgrep |

語順の規則(`function-noun-verb-order`。計測時の名前は `word-order`)の報告は tokio の 6 件だけで、全て名詞＋動詞の語順だった(`task_remove`、`task_push_back`、`service_write` など)。うち `env_remove`・`env_clear` は標準ライブラリの `std::process::Command` に名前を合わせたもの。計測中に、`to` の後ろの動詞(`worker_to_notify`)を報告する誤りが見つかったので直した。

分類の方法:

- 第1段階の完成判定と同じ基準で、スクリプトで分けた。未知語は、目視で選んだ略語の一覧に当たれば正しい検出、それ以外は辞書不足とした。英単語かどうかは macOS の英語辞書で判定した。
- 値の名前の末尾が、同じリポジトリの型名の最後の語と同じなら(tokio の `notify: Notify`、`ready: Ready`)、その分野の名詞として使っているとみなし、辞書不足に数えた。ripgrep の値は再分類したので、改善1〜3の後の再計測の値と少し違う。
- 避ける同義語・1文字の名前・連番は、設計書の方針どおりなので正しい検出に数えた。

判定:

- 4つとも**誤検出率は 63〜79% で、基準の 20% に届かない。** 誤検出の 97% は辞書不足で、辞書不足を除くと 5〜13% になる。基準を満たせるかは、標準辞書を広げる改善4で決まる。
- 2つ以上のリポジトリに出る未知語は 711 語で、未知語の出現の 61% に当たる。標準辞書に足す語の候補になる。1つのリポジトリだけに出る 2,077 語(出現の 39%)は、ドメイン辞書に任せる候補になる。
- alacritty は、辞書不足を除いても 13% と高い。英単語を連結語として分解する誤り(viewport、inline、underline)が多いため。

## 未決事項と今後の拡張

最小構成では型情報と対語の整合を扱わない。次の6点を決めれば実装を始められる。

- [x] 正式名を確定する(yogolint に決定)(実装言語は Rust に決定済み)
- [ ] 型情報が要る検査(真偽値の変数に is / has を求める)を、言語サーバー連携で追加するか
- [ ] 対語の整合検査(同じクラスに open があって close が無いなど)を入れるか
- [ ] ドメイン辞書の初版を `yogolint words` の未知語一覧から作る手順を決める
- [ ] 日本語用語(ja欄)と識別子の対応表を出力し、仕様書の用語集と照合するか
- [ ] クイックフィックス(置換候補での名前変更)を言語サーバーに加えるか

## 出典

- [cspell(npm)](https://npmjs.com/package/cspell) — 分割方式、辞書の使い方
- [Spiral: splitters for identifiers](https://www.theoj.org/joss-papers/joss.00653/10.21105.joss.00653.pdf) — 連結語の分割
- [Vale Vocabularies](https://vale.sh/docs/keys/vocab) — accept / reject 形式
- [Approved Verbs for PowerShell Commands](https://learn.microsoft.com/powershell/scripting/developer/cmdlet/approved-verbs-for-windows-powershell-commands) — 動詞と同義語の扱い
- [SSW: Avoid generic names](https://www.ssw.com.au/rules/avoid-generic-names) — manager / helper / data / info
- [Clean Code: Meaningful Names](https://gpcoder.gitbook.io/clean-code/the-key-principles-of-clean-code/meaningful-names) — 雑音語
- [Dialpad inclusive language](https://www.github.com/dialpad/inclusive-language) — 配慮語の置換表
- [Inclusive Naming Initiative Tier 1](https://inclusivenaming.org/word-lists/tier-1/inclusive-naming-word-lists-v1.0) — 配慮語の理由
- [typescript-eslint naming-convention](https://typescript-eslint.io/rules/naming-convention/) — 前段の命名規則lint

* [web-tree-sitter README](https://unpkg.com/web-tree-sitter@0.25.6/README.md) — WASMとネイティブ束縛の速度差
* [ast-grep 開発ガイド](https://ast-grep.github.io/contributing/development.html) — Rust中核とNode束縛の先例
* [ast-grep VSCode](https://www.github.com/ast-grep/ast-grep-vscode) — LSPクライアントとしての拡張
* [vscode-terraform PR #905](https://github.com/hashicorp/vscode-terraform/pull/905) — プラットフォーム別VSIXでのバイナリ同梱
* [termlint(PyPI)](https://pypi.org/project/termlint/) — 同名の文章向け用語lint
* [Lint (software)](https://www.wikipedia.com/wiki/Linting) — lint の語源
