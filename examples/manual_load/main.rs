//! # vibrato-rkyv 辞書の手動読み込みサンプル
//!
//! このサンプルでは、ローカルの辞書ファイルを異なる `LoadMode` で読み込む方法を示します。
//! `Validate` モードでは安全性が保証され、`TrustCache` モードでは2回目以降の読み込みが高速化されます。
//!
//! ## 主な機能
//!
//! - `from_zstd`: 圧縮された辞書ファイルの自動解凍とキャッシング
//! - `from_zstd_with_options`: カスタムキャッシュディレクトリへの解凍
//! - `from_path` with `LoadMode::Validate`: 常に安全な検証付き読み込み
//! - `from_path` with `LoadMode::TrustCache`: 高速な信頼ベースの読み込み
//!
//! ## 使用例
//!
//! ```bash
//! cargo run --example manual_load
//! ```
//!
//! ## 前提条件
//!
//! このサンプルを実行するには、事前にコンパイル済みの辞書ファイル（例: `system.dic.zst`）が必要です。
//! 詳細は下記のドキュメントコメントを参照してください。

use std::fs;
use std::{error::Error, path::PathBuf};
use std::path::Path;

use vibrato_rkyv::{CacheStrategy, Dictionary, LoadMode, Tokenizer};

/// ローカル辞書ファイルの読み込み方法を示すメイン関数
///
/// この関数では、異なる `LoadMode` を使用してローカル辞書ファイルを読み込む方法を実演します。
/// `Validate` モードは安全性を保証し、`TrustCache` モードは2回目以降の読み込みを高速化します。
///
/// This example demonstrates how to load a local dictionary file using
/// different `LoadMode`s: `Validate` for guaranteed safety, and `TrustCache`
/// for speed on subsequent loads.
///
/// ## 前提条件（Prerequisites）
///
/// このサンプルを実行するには、事前にコンパイル済みの辞書ファイル（例: `system.dic`）が必要です。
/// 辞書ファイルを取得する方法はいくつかあります。
///
/// To run this example, you must have a pre-compiled dictionary file (e.g., `system.dic`)
/// available locally. There are several ways to get one:
///
/// ### 方法 1: リリースページからダウンロード（Option 1: Download from Releases）
///
/// プロジェクトの GitHub Releases ページから、コンパイル済みの辞書を直接ダウンロードできます。
///
/// You can download a pre-compiled dictionary directly from the project's
/// GitHub Releases page:
/// > https://github.com/stellanomia/vibrato-rkyv/releases
///
/// `.tar` ファイル（例: `mecab-ipadic.tar`）をダウンロードして展開すると、
/// 中に `.dic.zst` ファイルが見つかります。
///
/// Download a `.tar` file (e.g., `mecab-ipadic.tar`), extract it, and you will
/// find the `.dic.zst` file inside.
///
/// ### 方法 2: プログラムでダウンロード（Option 2: Download Programmatically）
///
/// `download_dictionary` API を使用してプリセット辞書を取得する小さなヘルパースクリプトを作成できます。
///
/// You can create a small helper script that uses the `download_dictionary` API
/// to fetch a preset dictionary.
///
/// ```no_run
/// use vibrato_rkyv::{Dictionary, dictionary::PresetDictionaryKind};
/// use std::path::Path;
///
/// fn prepare_dictionary() -> Result<(), Box<dyn Error>> {
///     let cache_dir = Path::new("./dictionary_cache");
///     // 圧縮された辞書（.zst）をダウンロード
///     // This downloads the compressed dictionary (.zst)
///     let zst_path = Dictionary::download_dictionary(PresetDictionaryKind::Ipadic, cache_dir)?;
///     // 解凍して検証し、.dic ファイルを作成
///     // This decompresses and validates it, creating the .dic file
///     let _ = Dictionary::from_zstd(zst_path)?;
///     println!("Dictionary is ready in ./dictionary_cache/decompressed/");
///     Ok(())
/// }
/// ```
///
/// ### 方法 3: ソースからコンパイル（上級者向け）（Option 3: Compile from Source - Advanced）
///
/// 完全な制御が必要な場合、このワークスペースの `compiler` ツールを使用して、
/// ソースの CSV ファイルから辞書をコンパイルできます。
///
/// For full control, you can compile a dictionary from source CSV files using
/// the `compiler` tool in this workspace:
///   `cargo run --release -p compiler -- build ... --sysdic-out system.dic`
///
/// 辞書ファイルを取得したら、このワークスペースのルートに配置するか、
/// 下記の `DICT_PATH` 定数を変更してください。
///
/// Once you have the dictionary file, place it in the root of this workspace,
/// or modify the `DICT_PATH` constant below.
fn main() -> Result<(), Box<dyn Error>> {
    // 圧縮辞書ファイルのパス
    const ZSTD_DICT_PATH: &str = "system.dic.zst";

    println!("--- Manual Dictionary Loading Example ---");

    // 辞書ファイルの存在確認
    if !Path::new(ZSTD_DICT_PATH).exists() {
        eprintln!("Error: Compressed dictionary file not found at '{}'", ZSTD_DICT_PATH);
        eprintln!("See comments in `vibrato/examples/manual_load.rs` for setup instructions.");
        return Err("Dictionary file missing".into());
    }

    // トークナイズするテキスト
    let text = "あなたは猫が好きですか？";

    // 方法 1: `from_zstd` による読み込み
    // 自動的に解凍を行い、ソースファイルの隣に `decompressed` サブディレクトリへキャッシュします
    // `from_zstd`:
    // It automatically handles decompression and caching to a `decompressed`
    // subdirectory next to the source file.
    println!("\n1. Loading with `from_zstd`");
    let _dict_zstd = Dictionary::from_zstd(ZSTD_DICT_PATH, CacheStrategy::GlobalCache)?;
    println!("Dictionary loaded from '{}'. Check for a 'decompressed' directory nearby.", ZSTD_DICT_PATH);

    // （形態素解析はすべて同じなので、最後に1回だけ実行します）
    // (Tokenization is the same for all, so we'll show it once at the end)

    // 次のステップのために、`from_zstd` が作成したキャッシュをクリーンアップします
    // Clean up the cache created by `from_zstd` for the next step.
    let default_cache_dir = Path::new(ZSTD_DICT_PATH).parent().unwrap().join("decompressed");
    if default_cache_dir.exists() {
        fs::remove_dir_all(&default_cache_dir)?;
    }

    // 方法 2: `from_zstd_with_options` による読み込み
    // ここでは、`from_path` のサンプル用に、予測可能な場所に辞書を解凍します
    // `from_zstd_with_options`:
    // Here we use it to decompress the dictionary into a predictable location
    // that we can then use for the `from_path` examples.
    println!("\n2. Setting up for `from_path` using `from_zstd_with_options`");
    let setup_cache_dir = PathBuf::from("./manual_load_cache");
    let _ = Dictionary::from_zstd_with_options(ZSTD_DICT_PATH, &setup_cache_dir, false)?;

    // これで、管理された場所に解凍された `.dic` ファイルが準備できました
    // Now, we have the decompressed `.dic` file ready in our controlled location.
    let dic_path = setup_cache_dir.join(Path::new(ZSTD_DICT_PATH).file_stem().unwrap());
    println!("Decompressed dictionary is ready at: {}", dic_path.display());

    // 方法 3: `from_path` による読み込み
    // `from_path:
    println!("\n3. Loading with `from_path`");
    println!("\n3a. Using LoadMode::Validate");

    // LoadMode::Validate: 常に安全
    // 毎回検証を行うため、完全に安全ですが処理時間がかかります
    // LoadMode::Validate: Always Safe
    let _dict_validate = Dictionary::from_path(&dic_path, LoadMode::Validate)?;
    println!("Dictionary loaded safely with validation.");


    println!("\n3b. Using LoadMode::TrustCache");
    println!("(First run with this mode creates a cache file: {}.sha256)", dic_path.display());

    // LoadMode::TrustCache: 2回目以降は高速
    // 初回実行時にチェックサムファイル（.sha256）を作成し、
    // 2回目以降はチェックサムの比較のみで検証をスキップするため高速です
    // LoadMode::TrustCache: Fast on Subsequent Loads
    let dict_trust_cache = Dictionary::from_path(&dic_path, LoadMode::TrustCache)?;
    println!("Dictionary loaded via TrustCache mode.");


    // 最後に読み込んだ辞書でトークナイザーを動作させます
    // Now let's see the tokenizer in action with the final loaded dictionary
    println!("\n--- Tokenization Example ---");
    let tokenizer_final = Tokenizer::new(dict_trust_cache);
    let mut worker_final = tokenizer_final.new_worker();
    worker_final.reset_sentence(text);
    worker_final.tokenize();
    for token in worker_final.token_iter() {
        println!("  {}\t{}", token.surface(), token.feature());
    }

    // クリーンアップ
    if setup_cache_dir.exists() {
        fs::remove_dir_all(&setup_cache_dir)?;
    }

    Ok(())
}
