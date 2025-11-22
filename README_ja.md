# 🎤 vibrato-rkyv: VIterbi-Based acceleRAted TOkenizer with rkyv

**注意:** これは元の[daac-tools/vibrato](https://github.com/daac-tools/vibrato)をフォークし、`rkyv`シリアライゼーションフレームワークを使用して辞書読み込みを大幅に高速化したバージョンです。

[![Crates.io](https://img.shields.io/crates/v/vibrato-rkyv)](https://crates.io/crates/vibrato-rkyv)
[![Documentation](https://docs.rs/vibrato-rkyv/badge.svg)](https://docs.rs/vibrato-rkyv)
[![Build Status](https://github.com/stellanomia/vibrato-rkyv/actions/workflows/rust.yml/badge.svg)](https://github.com/stellanomia/vibrato-rkyv/actions)

Vibratoは、Viterbiアルゴリズムに基づくトークン化（形態素解析）の高速実装です。

## `rkyv`による大幅な辞書読み込み速度の向上

`vibrato-rkyv`は、[`rkyv`](https://rkyv.org/)ゼロコピー逆シリアライゼーションフレームワークを利用して、辞書読み込みの大幅な高速化を実現しています。辞書ファイルをメモリマッピングすることで、ほぼ瞬時に使用可能になります。

以下のベンチマーク結果は、非圧縮ファイルと`zstd`圧縮ファイルの両方からの読み込みを比較し、パフォーマンスの違いを示しています。

CPU: Intel Core i7-14700
OS: WSL2 (Ubuntu 24.04)
辞書: UniDic-cwj v3.1.1（非圧縮辞書バイナリ 約700MB）
ソース: ベンチマークコードは[benches](./vibrato/benches)ディレクトリで利用可能です。

### 非圧縮ファイル（`.dic`）から

以下の表は、事前に解凍された`.dic`ファイルからの辞書読み込みのパフォーマンスを比較しています。`from_path_unchecked`で最速のスピードが達成され、`LoadMode::TrustCache`を使った`from_path`は、安全でほぼ瞬時に読み込める代替手段を提供します。

| 条件 | オリジナルの`vibrato`（ストリームから読み込み） | `vibrato-rkyv`（メモリマップ） | 高速化倍率 |
| :--- | :--- | :--- | :--- |
| コールドスタート（セーフキャッシュ）¹ | ~42秒 | **~1.1ミリ秒** | ~38,000倍 |
| ウォームスタート（未チェック）² | ~34秒 | **~2.9マイクロ秒** | ~11,700,000倍 |
| ウォームスタート（セーフキャッシュ）³ | ~34秒 | **~4.1マイクロ秒** | ~8,300,000倍 |

これは、キャッシュの安全性機構（メタデータハッシュとファイルチェック）が、アンセーフバージョンと比較してわずか~1.2マイクロ秒のオーバーヘッドしか追加しないことを示しています。

¹ **コールドスタート（セーフキャッシュ）**: ファイルはOSページキャッシュにありませんが、アプリケーションキャッシュ（proofファイル）は有効です。これはディスクI/Oのコストを測定します。
² **ウォームスタート（未チェック）**: `from_path_unchecked`を使用した最速のシナリオ。ファイルはOSページキャッシュにあり、バイトチェックはバイパスされます。
³ **ウォームスタート（セーフキャッシュ）**: `LoadMode::TrustCache`を使用した典型的な高速再読み込みシナリオ。ファイルはOSページキャッシュにあり、最小限の検証が実行されます。

### Zstd圧縮ファイル（`.dic.zst`）から

| 条件 | オリジナルの`vibrato`（ストリームから読み込み） | `vibrato-rkyv`（キャッシュ使用） | 高速化倍率 |
| :--- | :--- | :--- | :--- |
| 初回実行（コールド） | ~4.6秒 | ~1.3秒 | ~3.5倍 |
| 2回目以降の実行（キャッシュヒット） | ~4.5秒 | ~6.5マイクロ秒 | ~700,000倍 |

<small>*`vibrato-rkyv`は、初回実行時に辞書を自動的に解凍してキャッシュし、その後の読み込みではメモリマップされたキャッシュを使用します。*</small>

このパフォーマンスを活用するには、`Dictionary::from_path`または`Dictionary::from_zstd`メソッドを使用してください：

```rust
use vibrato_rkyv::{Dictionary, LoadMode};

// 非圧縮辞書に推奨：
// メモリマッピングによるほぼ瞬時の読み込み。
let dict_mmap = Dictionary::from_path("path/to/system.dic")?;

// zstd圧縮辞書に推奨：
// 初回実行時に解凍してキャッシュし、その後はメモリマッピングを使用。
let dict_zstd = Dictionary::from_zstd("path/to/system.dic.zst", LoadMode::TrustCache)?;
```

## 相違点

以下は、オリジナル実装との主な相違点をまとめたものです。

### オリジナルの`vibrato`との違い

オリジナルの`daac-tools/vibrato`から移行する場合、以下の主な変更点に注意してください：

- **レガシー辞書サポート（legacyフィーチャー使用時）:** `vibrato-rkyv`は、ネイティブの`rkyv`ベース辞書フォーマットでのパフォーマンスを追求して設計されています。しかし、柔軟性を提供し、ユーザーが幅広い辞書アセットを活用できるようにするため、`legacy`フィーチャーを有効にすると、オリジナルの`vibrato`で使用されていた`bincode`ベースのフォーマットもサポートします。
これにより、独自コーパス（例：BCCWJ）で学習された辞書など、`bincode`フォーマットでのみ利用可能な価値ある既存の辞書を使用できます。
ライブラリは異なるフォーマットを扱います：
  - `Dictionary::from_path()`: 非圧縮の`rkyv`と`bincode`フォーマットの辞書を透過的に読み込みます。ファイルの内容に基づいてフォーマットを自動検出します。
  - `Dictionary::from_zstd()`: Zstandard圧縮辞書を与えられると、フォーマットを認識した洗練されたキャッシング機能を提供します：
    - 辞書が`rkyv`フォーマットの場合、解凍されてキャッシュされ、その後の読み込みではほぼ瞬時のメモリマップアクセスが可能になります。
    - 辞書が`bincode`フォーマットの場合、即座に使用できるようメモリに直接読み込まれます。バックグラウンドで`rkyv`フォーマットへの変換プロセスが開始され、別個のキャッシュが作成されます。これにより、初回読み込みは動作可能であり、すべての将来の読み込みは高速な`rkyv`キャッシュから恩恵を受けます。

ほとんどのユースケースでは、手動での変換が不要になります。辞書を変換したいユーザーには、compilerのtransmuteコマンドも利用可能です（下記の[ツールチェーン](#追加の改善)を参照）。

- **ユーザー辞書は事前コンパイルが必要:** `--user-dic`ランタイムオプションは削除されました。ユーザー辞書は、システム辞書に事前にコンパイルする必要があります。この設計選択は、`rkyv`のゼロコピー、不変モデルをサポートしています。
  ただし、これは辞書が純粋に静的であることを意味するものではありません。辞書を読み込んだ*後*に変更することはできませんが、メモリ内で動的に辞書を構築し（例：`SystemDictionaryBuilder`を使用）、`Dictionary::from_inner()`を使用してそこから`Tokenizer`を作成することができます。これは、トークン化が開始される前に辞書の内容が実行時に生成されるシナリオに有用です。

- **新しい推奨読み込みAPI:** 最大のパフォーマンスのために、非圧縮ファイルには`Dictionary::from_path()`を、`zstd`圧縮ファイルには`Dictionary::from_zstd()`を使用してください。これらのメソッドは、ほぼ瞬時の読み込みのためにメモリマッピングとキャッシングを活用します。`Dictionary::read()`は汎用リーダー用にまだ利用可能ですが、効率は劣ります。

```rust
use vibrato_rkyv::{dictionary::LoadMode, Dictionary};

// 推奨：メモリマッピングによるゼロコピー読み込み。
let dict = Dictionary::from_path("path/to/system.dic", LoadMode::TrustCache)?;
```

### 追加の改善

高速読み込みのための`rkyv`への中核的な変更を超えて、`vibrato-rkyv`はオリジナル実装に対していくつかの重要な拡張を含んでいます：

* **統合・強化されたツールチェーン（`compiler`）**
  `train`、`dictgen`、`compile`実行可能ファイルは、より強力な単一の`compiler`ツールに統合されました。これにより、明確なサブコマンド構造（`train`、`dictgen`、`build`）で辞書作成ワークフローが簡素化されます。また、以下が追加されています：
  * `full-build`: トレーニング-生成-ビルドプロセス全体を一度に実行する便利なコマンド。
  * `transmute`: オリジナルの`vibrato`からレガシー`bincode`フォーマット辞書を新しい`rkyv`フォーマットに変換するユーティリティ。

* **柔軟な`Tokenizer`**
  `Tokenizer` APIは、長年の設計上の制限（[上流のissue #99](https://github.com/daac-tools/vibrato/issues/99)）を解決し、より柔軟性を高めるために再設計されました。
  * 安価に`Clone`可能になりました（内部的に`Arc<Dictionary>`を使用）。
  * `Tokenizer::from_inner(DictionaryInner)`のような新しいコンストラクタにより、動的に構築された辞書インスタンスから直接トークナイザーを作成でき、テストやオンザフライで辞書を生成するアプリケーションの柔軟性が向上しました。

* **所有トークン型（`TokenBuf`）**
  既存の借用型`Token<'a>`と並んで、新しい所有トークン型`TokenBuf`が導入されました。Rustの標準ライブラリの馴染みのある`Path`/`PathBuf`パターンに従っています。これにより、ライフタイムの複雑さなしに、トークン化結果を簡単に保存、変更、またはスレッド間で送信できます。

* **組み込み辞書ダウンローダーとマネージャー**
  開始がこれまで以上に簡単になりました。単一の関数呼び出しで、事前コンパイルされたプリセット辞書（IPADIC、UNIDICなど）をダウンロードしてセットアップできます。
  * `Dictionary::from_preset_with_download()`: ダウンロード、チェックサム検証、キャッシングを自動的に処理します。
  * `Dictionary::from_zstd()`: `zstd`圧縮辞書を、初回実行時にローカルキャッシュに解凍して管理します。また、レガシー`bincode`フォーマット辞書を自動的に検出して変換し（legacyフィーチャーが有効な場合）、将来の高速読み込みのためにモダンフォーマットでキャッシュします。

* N-best トークン化（実験的）
上流の機能リクエスト（[上流のissue #151](https://github.com/daac-tools/vibrato/issues/151)）に応えて、コスト順にソートされた複数のトークン化候補を取得する実験的機能が追加されました。実装はA*探索アルゴリズムを採用しており、下流のNLPタスクにおける曖昧性の処理に役立ちます。

## 特徴

### 高速なトークン化

Vibratoは、高速トークナイザー[MeCab](https://taku910.github.io/mecab/)のRust再実装ですが、その実装はさらに高速なトークン化のために簡素化され最適化されています。特に大きなマトリックスを持つ言語リソース（例：459MiBのマトリックスを持つ[`unidic-cwj-3.1.1`](https://clrd.ninjal.ac.jp/unidic/back_number.html#unidic_cwj)）では、キャッシュ効率の良いIDマッピングのおかげで、Vibratoはより高速に動作します。

例えば、以下の図は、MeCabとその再実装によるトークン化時間の実験結果を示しています。詳細な実験設定やその他の結果は、[Wiki](https://github.com/daac-tools/vibrato/wiki/Speed-Comparison)で利用可能です。

![](./figures/comparison.svg)

### MeCab互換性

Vibratoは、空白を無視するなど、MeCabと同一のトークン化結果を出力するオプションをサポートしています。

### トレーニングパラメータ

Vibratoは、コーパスから辞書内のパラメータ（コスト）のトレーニングもサポートしています。詳細な説明は[こちら](./docs/train.md)をご覧ください。

## 基本的な使用方法

このソフトウェアはRustで実装されています。まず、[公式の手順](https://www.rust-lang.org/tools/install)に従って`rustc`と`cargo`をインストールしてください。


### Rustライブラリとして（推奨）

最も簡単な始め方は、`vibrato-rkyv`をライブラリとして使用し、事前コンパイルされたプリセット辞書をダウンロードすることです。

**1. `vibrato-rkyv`を依存関係に追加**

`Cargo.toml`に以下を追加してください。辞書ダウンロード機能はデフォルトで有効です。

```toml
[dependencies]
vibrato-rkyv = "x.y.z"
```

**2. 辞書をダウンロードしてテキストをトークン化**

`Dictionary::from_preset_with_download()`関数がすべてを処理します：ダウンロード、チェックサムの検証、および将来の実行のための指定ディレクトリへの辞書のキャッシュ。

```rust
use std::path::PathBuf;
use vibrato_rkyv::{dictionary::PresetDictionaryKind, Dictionary, Tokenizer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 辞書をキャッシュするディレクトリを指定。
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from(".cache"))
        .join("vibrato-rkyv");

    // プリセット辞書（例：IPADIC）をダウンロードして読み込み。
    // 辞書は指定ディレクトリにキャッシュされるため、以降の実行は瞬時です。
    let dict = Dictionary::from_preset_with_download(
        PresetDictionaryKind::Ipadic,
        &cache_dir,
    )?;

    // 読み込んだ辞書でトークナイザーを作成。
    let tokenizer = Tokenizer::new(dict);

    // ワーカーはトークン化のための内部状態を保持し、再利用できます。
    let mut worker = tokenizer.new_worker();

    worker.set_text("本とカレーの街神保町へようこそ。");
    worker.tokenize();

    // トークンを反復処理。
    for token in worker.token_iter() {
        println!("{}\t{}", token.surface(), token.feature());
    }

    Ok(())
}
```

### コマンドラインツールとして

**1. 辞書の準備**

`vibrato-rkyv`互換の辞書ファイル（`.dic`）が必要です。`compiler`ツールを使用して、ソースCSVファイルから辞書をビルドしてください。

```bash
# 辞書のコンパイル例
$ cargo run --release -p compiler -- build \
    --lexicon-in path/to/lex.csv \
    --matrix-in path/to/matrix.def \
    --char-in path/to/char.def \
    --unk-in path/to/unk.def \
    --sysdic-out system.dic
```

**2. 文のトークン化**

テキストを`tokenize`コマンドにパイプし、`-i`で辞書パスを指定してください。

```bash
$ echo '本とカレーの街神保町へようこそ。' | cargo run --release -p tokenize -- -i path/to/system.dic
```

結果はMeCabフォーマットで出力されます。トークンをスペース区切りで出力するには、`-O wakati`オプションを使用してください。

```
本	名詞,一般,*,*,*,*,本,ホン,ホン
と	助詞,並立助詞,*,*,*,*,と,ト,ト
カレー	名詞,固有名詞,地域,一般,*,*,カレー,カレー,カレー
の	助詞,連体化,*,*,*,*,の,ノ,ノ
街	名詞,一般,*,*,*,*,街,マチ,マチ
神保	名詞,固有名詞,地域,一般,*,*,神保,ジンボウ,ジンボー
町	名詞,接尾,地域,*,*,*,町,マチ,マチ
へ	助詞,格助詞,一般,*,*,*,へ,ヘ,エ
ようこそ	感動詞,*,*,*,*,*,ようこそ,ヨウコソ,ヨーコソ
。	記号,句点,*,*,*,*,。,。,。
EOS
```

## 高度な使用方法

### MeCab互換オプション

VibratoはMeCabアルゴリズムの再実装ですが、デフォルトのトークン化結果は異なる場合があります。例えば、Vibratoはデフォルトで空白をトークンとして扱いますが、MeCabは無視します。

MeCabと同一の結果を得るには、`-S`（空白を無視）と`-M`（最大未知語長）フラグを使用してください。

```bash
# MeCab互換の出力を得る
$ echo 'mens second bag' | cargo run --release -p tokenize -- -i path/to/system.dic -S -M 24
mens	名詞,固有名詞,組織,*,*,*,*
second	名詞,一般,*,*,*,*,*
bag	名詞,固有名詞,組織,*,*,*,*
EOS
```
*注意：コスト計算のタイブレークにより、まれに結果が異なる場合があります。*

### ユーザー辞書の使用

**重要:** `vibrato-rkyv`では、ユーザー辞書をランタイムオプションとして指定できなくなりました。事前にシステム辞書にコンパイルする必要があります。

**オプション：`compiler full-build`コマンドを使用**

新しい辞書をトレーニングする場合、`full-build`コマンドがユーザー辞書を含める推奨方法です。トレーニング、ソースファイルの生成（ユーザー語彙を含む）、最終バイナリのビルドといったパイプライン全体を処理します。`--user-lexicon-in`オプションを使用してください。

```bash
$ cargo run --release -p compiler -- full-build \
    -t path/to/corpus.txt \
    -l path/to/seed_lex.csv \
    --user-lexicon-in path/to/my_user_dic.csv \
    ... # その他の必須引数
    -o ./my_dictionary
```

## ライセンス

以下のいずれかの下でライセンスされています

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) または http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) または http://opensource.org/licenses/MIT)

お好きな方をお選びください。

## 参考文献

Vibratoの技術的詳細は、以下のリソースで利用可能です：

- 神田峻介, 赤部晃一, 後藤啓介, 小田悠介.
  [最小コスト法に基づく形態素解析におけるCPUキャッシュの効率化](https://www.anlp.jp/proceedings/annual_meeting/2023/pdf_dir/C2-4.pdf),
  言語処理学会第29回年次大会 (NLP2023).
- 赤部晃一, 神田峻介, 小田悠介.
  [CRFに基づく形態素解析器のスコア計算の分割によるモデルサイズと解析速度の調整](https://www.anlp.jp/proceedings/annual_meeting/2023/pdf_dir/C2-1.pdf),
  言語処理学会第29回年次大会 (NLP2023).
- [MeCab互換な形態素解析器Vibratoの高速化技法](https://tech.legalforce.co.jp/entry/2022/09/20/133132),
  LegalOn Technologies Engineering Blog (2022-09-20).
