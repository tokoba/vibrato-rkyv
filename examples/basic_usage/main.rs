//! # vibrato-rkyv 基本的な使い方のサンプル
//!
//! このサンプルでは、プリセット辞書の自動ダウンロードとキャッシング機能を使用した
//! vibrato-rkyv の基本的な使い方を示します。
//!
//! ## 主な機能
//!
//! - プリセット辞書の自動ダウンロード
//! - キャッシュディレクトリへの辞書の保存
//! - 基本的な形態素解析
//! - N-best 解析による複数の解析候補の取得
//!
//! ## 使用例
//!
//! ```bash
//! cargo run --example basic_usage
//! ```
//!
//! 初回実行時は辞書のダウンロードが行われるため時間がかかりますが、
//! 2回目以降はキャッシュから高速に読み込まれます。

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use vibrato_rkyv::{Dictionary, Tokenizer};
use vibrato_rkyv::dictionary::PresetDictionaryKind;

/// vibrato-rkyv の基本的な使い方を示すメイン関数
///
/// この関数では以下の処理を実行します：
/// 1. キャッシュディレクトリの準備
/// 2. プリセット辞書のダウンロードと読み込み
/// 3. トークナイザーの作成とワーカーの初期化
/// 4. 通常の形態素解析の実行
/// 5. N-best 解析による複数の解析候補の取得
fn main() -> Result<(), Box<dyn Error>> {
    // キャッシュディレクトリの準備
    // システム標準のキャッシュ場所にサブディレクトリを作成します
    // This example uses a subdirectory in the system's standard cache location.
    let cache_dir = dirs::cache_dir().unwrap_or_else(|| PathBuf::from(".cache")).join("vibrato-rkyv-assets");
    fs::create_dir_all(&cache_dir)?;

    println!("Cache directory: {}", cache_dir.display());

    // プリセット辞書のダウンロードと読み込み
    // `from_preset_with_download` はダウンロード、チェックサム検証、キャッシングを自動で行います
    // 初回実行時は辞書をダウンロードしますが、2回目以降はキャッシュから即座に読み込みます
    //
    // 利用可能なプリセット（フィーチャーフラグなし）:
    // - Ipadic: MeCab IPADIC v2.7.0
    // - Unidic: UniDic-cwj v3.1.1
    //
    // `legacy` フィーチャーフラグを有効にすると、他の辞書も利用可能です
    // `from_preset_with_download` handles downloading, checksum verification,
    // and caching. The first run will download the dictionary, but subsequent
    // runs will load the cache instantly.
    //
    // Available presets without any feature flags:
    // - Ipadic: MeCab IPADIC v2.7.0
    // - Unidic: UniDic-cwj v3.1.1
    //
    // Other dictionaries are available with the `legacy` feature flag.
    println!("Loading the IPADIC preset dictionary. This may take a moment on the first run...");
    let preset = PresetDictionaryKind::Ipadic;
    let dict = Dictionary::from_preset_with_download(
        preset,
        cache_dir.join(preset.name()),
    )?;
    println!("Dictionary loaded successfully.");

    // トークナイザーの作成
    // 辞書からトークナイザーを作成します
    //
    // 注意: `legacy` フィーチャー有効時に Dictionary::from_zstd を使用した場合、
    // この関数のムーブセマンティクスにより、トークナイザーがドロップされた際に
    // バックグラウンドのキャッシングスレッドの完了を待つため、
    // 現在のスレッドがブロックされる可能性があります
    // The tokenizer is created from the dictionary.
    //
    // Note: When using Dictionary::from_zstd with the legacy feature,
    // this function's move semantics may cause the current thread
    // to block and wait for a background caching thread to finish when the tokenizer is dropped.
    let tokenizer = Tokenizer::new(dict);

    // ワーカーの作成
    // トークナイザーから形態素解析を実行するワーカーを作成します
    let mut worker = tokenizer.new_worker();

    // 基本的な形態素解析の実行
    // テキストを設定し、単一の最適な解析結果を取得します
    let text = "あなたは猫が好きですか？";
    println!("\nTokenizing the text: \"{}\"", text);
    worker.reset_sentence(text);
    worker.tokenize();

    // 形態素解析結果の表示
    // 各トークンの表層形（surface）と素性（feature）を出力します
    println!("\nTokenization Result:");
    for token in worker.token_iter() {
        println!("{}\t{}", token.surface(), token.feature());
    }

    // N-best 解析の実行
    // 複数の解析候補を取得します（この例では上位3つ）
    // 曖昧性のある文では、異なる解析結果が得られます
    let text = "かえるがかえるをかえる";
    println!("\nTokenizing the text: \"{}\"", text);

    worker.reset_sentence(text);
    worker.tokenize_nbest(3);

    // N-best 解析の最上位解析結果の表示
    // token_iter() は最もコストの低い（最適な）解析結果を返します
    println!("\nTokenization N-best Result:");
    for token in worker.token_iter() {
        println!("{}\t{}", token.surface(), token.feature());
    }

    // すべての解析候補の表示
    // 各候補パスのコストと、そのパスに含まれるトークンを表示します
    println!("Found {} paths:", worker.num_nbest_paths());
    for i in 0..worker.num_nbest_paths() {
        println!("Path {}:", i + 1);
        let cost = worker.path_cost(i).unwrap();
        println!("  cost {}:", cost);

        worker
            .nbest_token_iter(i)
            .unwrap()
            .for_each(|t| {
                println!("{}: {}", t.surface(), t.feature());
            });
    }

    Ok(())
}
