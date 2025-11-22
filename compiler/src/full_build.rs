//! フルビルドモジュール
//!
//! このモジュールは、コーパスから辞書を一括構築する機能を提供します。
//! モデルの訓練、辞書ファイルの生成、バイナリ辞書の構築の3つのステップを
//! 自動的に実行し、すべての中間ファイルと最終的な辞書を生成します。

use std::{fs::File, path::PathBuf};
use clap::Parser;

use crate::{build::{self, BuildError}, dictgen::{self, DictgenError, generate_dictionary_files}, train::{self, TrainError, TrainingParams}};

/// フルビルドコマンドの引数
///
/// コーパスと各種定義ファイルから辞書を一括構築するために必要な
/// すべての入力ファイルと設定を指定します。
#[derive(Parser, Debug)]
#[clap(
    name = "full-build",
    about = "Builds a dictionary and all intermediate artifacts from a corpus"
)]
pub struct Args {
    /// Corpus file to be trained (e.g., BCCWJ).
    #[clap(short = 't', long, value_name = "CORPUS_PATH")]
    pub corpus: PathBuf,

    /// Lexicon file (lex.csv) to be weighted. All costs must be 0.
    #[clap(short = 'l', long, value_name = "SEED_LEXICON_PATH")]
    pub seed_lexicon: PathBuf,

    /// Unknown word file (unk.def) to be weighted. All costs must be 0.
    #[clap(short = 'u', long, value_name = "SEED_UNK_PATH")]
    pub seed_unk: PathBuf,

    /// Character definition file (char.def).
    #[clap(short = 'c', long, value_name = "FILE_PATH")]
    pub char_def: PathBuf,

    /// Feature definition file (feature.def).
    #[clap(short = 'f', long, value_name = "FILE_PATH")]
    pub feature_def: PathBuf,

    /// Rewrite rule definition file (rewrite.def).
    #[clap(short = 'r', long, value_name = "FILE_PATH")]
    pub rewrite_def: PathBuf,

    /// User-defined lexicon file to include in the dictionary.
    #[clap(long, value_name = "USER_LEXICON_PATH")]
    pub user_lexicon_in: Option<PathBuf>,

    /// Regularization coefficient (L1).
    #[clap(long, default_value = "0.01")]
    pub lambda: f64,

    /// Maximum number of iterations for training.
    #[clap(long, default_value = "100")]
    pub max_iter: u64,

    /// Number of threads for training.
    #[clap(long, default_value = "1")]
    pub num_threads: usize,

    /// Enable the dual connector for a faster but larger dictionary.
    #[clap(long)]
    pub dual_connector: bool,

    /// Directory to which all artifacts will be output.
    #[clap(short = 'o', long, value_name = "OUTPUT_DIR")]
    pub out_dir: PathBuf,
}

/// フルビルド処理中に発生する可能性のあるエラー
///
/// 訓練、辞書生成、ビルドの各フェーズで発生したエラーをラップします。
#[derive(Debug, thiserror::Error)]
pub enum FullBuildError {
    /// モデル訓練中のエラー
    #[error(transparent)]
    Train(#[from] TrainError),
    /// 辞書生成中のエラー
    #[error(transparent)]
    Dictgen(#[from] DictgenError),
    /// 辞書ビルド中のエラー
    #[error(transparent)]
    Build(#[from] BuildError),
    /// 入出力エラー
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Vibratoライブラリのエラー
    #[error(transparent)]
    Vibrato(#[from] vibrato_rkyv::errors::VibratoError),
}

/// フルビルドコマンドを実行する
///
/// 以下の3つのステップを順次実行します:
/// 1. コーパスからモデルを訓練
/// 2. モデルから辞書ソースファイルを生成
/// 3. ソースファイルからバイナリ辞書を構築
///
/// # 引数
///
/// * `args` - フルビルドコマンドの引数
///
/// # 戻り値
///
/// 成功時は`Ok(())`。すべての成果物は`args.out_dir`に出力されます。
///
/// # エラー
///
/// 各フェーズの処理やファイルの入出力に失敗した場合、`FullBuildError`を返します。
pub fn run(args: Args) -> Result<(), FullBuildError> {
    std::fs::create_dir_all(&args.out_dir)?;

    println!("[1/3] Training model...");
    let params = TrainingParams {
        seed_lexicon: args.seed_lexicon,
        seed_unk: args.seed_unk,
        corpus: args.corpus,
        char_def: args.char_def,
        feature_def: args.feature_def,
        rewrite_def: args.rewrite_def,
        lambda: args.lambda,
        max_iter: args.max_iter,
        num_threads: args.num_threads,
    };
    let mut model = train::train_model(&params)?;

    let model_path = args.out_dir.join("model.bin.zst");
    let mut model_wtr = zstd::Encoder::new(File::create(&model_path)?, 19)?;
    model.write_model(&mut model_wtr)?;
    model_wtr.finish()?;

    println!("[2/3] Generating dictionary source files...");
    let mut sources = dictgen::create_dictionary_writers_from_paths(
        &args.out_dir.join("lex.csv"),
        &args.out_dir.join("matrix.def"),
        &args.out_dir.join("unk.def"),
        None,
        Some(&args.out_dir.join("bigram")), // Base name for .left, .right, .cost
    )?;

    if let Some(path) = &args.user_lexicon_in {
        model.read_user_lexicon(File::open(path)?)?;
    }

    generate_dictionary_files(&mut model, &mut sources)?;

    println!("[3/3] Building binary dictionary...");
    let build_source = build::BuildSource::FromBigram {
        lexicon: args.out_dir.join("lex.csv"),
        bigram_right: args.out_dir.join("bigram.right"),
        bigram_left: args.out_dir.join("bigram.left"),
        bigram_cost: args.out_dir.join("bigram.cost"),
        char_def: params.char_def,
        unk_def: args.out_dir.join("unk.def"),
        dual_connector: args.dual_connector,
    };

    let dict_inner = build::build_dictionary(&build_source)?;

    let sysdic_path = args.out_dir.join("system.dic.zst");
    let mut sysdic_wtr = zstd::Encoder::new(File::create(sysdic_path)?, 19)?;
    dict_inner.write(&mut sysdic_wtr)?;
    sysdic_wtr.finish()?;

    println!("Successfully built all artifacts in {}", args.out_dir.display());
    Ok(())
}
