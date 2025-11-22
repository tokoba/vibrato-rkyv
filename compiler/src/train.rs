//! モデル訓練モジュール
//!
//! このモジュールは、コーパスから形態素解析モデルを訓練する機能を提供します。
//! 教師データとなるコーパスと各種定義ファイルを読み込み、L1正則化を用いた
//! 確率的勾配降下法により重みパラメータを学習します。

use std::fs::File;
use std::io;
use std::path::PathBuf;

use clap::Parser;
use thiserror::Error;

use vibrato_rkyv::errors::VibratoError;
use vibrato_rkyv::trainer::{Corpus, Model, Trainer, TrainerConfig};

/// 訓練コマンドの引数
///
/// モデルを訓練するために必要な入力ファイルと訓練パラメータを指定します。
#[derive(Parser, Debug)]
#[clap(name = "train", about = "Model trainer")]
pub struct Args {
    /// Lexicon file (lex.csv) to be weighted.
    ///
    /// All connection IDs and weights must be set to 0.
    #[clap(short = 'l', long)]
    seed_lexicon: PathBuf,

    /// Unknown word file (unk.def) to be weighted.
    ///
    /// All connection IDs and weights must be set to 0.
    #[clap(short = 'u', long)]
    seed_unk: PathBuf,

    /// Corpus file to be trained. The format is the same as the output of the tokenize command of
    /// Vibrato.
    #[clap(short = 't', long)]
    corpus: PathBuf,

    /// Character definition file (char.def).
    #[clap(short = 'c', long)]
    char_def: PathBuf,

    /// Feature definition file (feature.def).
    #[clap(short = 'f', long)]
    feature_def: PathBuf,

    /// Rewrite rule definition file (rewrite.def).
    #[clap(short = 'r', long)]
    rewrite_def: PathBuf,

    /// A file to which the model is output. The file is compressed by zstd.
    #[clap(short = 'o', long)]
    model_out: PathBuf,

    /// Regularization coefficient. The larger the value, the stronger the L1-regularization.
    #[clap(long, default_value = "0.01")]
    lambda: f64,

    /// Maximum number of iterations.
    #[clap(long, default_value = "100")]
    max_iter: u64,

    /// Number of threads.
    #[clap(long, default_value = "1")]
    num_threads: usize,
}

/// 訓練処理中に発生する可能性のあるエラー
#[derive(Debug, Error)]
pub enum TrainError {
    /// 入出力エラー
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// 訓練処理エラー
    #[error("Training process failed: {0}")]
    Vibrato(#[from] VibratoError),
}

/// モデル訓練のパラメータ
///
/// 訓練に必要なファイルパスと訓練設定をまとめた構造体です。
#[derive(Debug, Clone)]
pub struct TrainingParams {
    /// シード語彙ファイル(lex.csv)のパス
    pub seed_lexicon: PathBuf,
    /// シード未知語ファイル(unk.def)のパス
    pub seed_unk: PathBuf,
    /// 訓練用コーパスファイルのパス
    pub corpus: PathBuf,
    /// 文字定義ファイル(char.def)のパス
    pub char_def: PathBuf,
    /// 素性定義ファイル(feature.def)のパス
    pub feature_def: PathBuf,
    /// 書き換え規則定義ファイル(rewrite.def)のパス
    pub rewrite_def: PathBuf,
    /// L1正則化係数
    ///
    /// 値が大きいほど正則化が強くなり、スパース性が高まります。
    pub lambda: f64,
    /// 最大イテレーション数
    pub max_iter: u64,
    /// 並列処理に使用するスレッド数
    pub num_threads: usize,
}

/// 訓練コマンドを実行する
///
/// コーパスと定義ファイルからモデルを訓練し、zstd圧縮して保存します。
///
/// # 引数
///
/// * `args` - 訓練コマンドの引数
///
/// # 戻り値
///
/// 成功時は`Ok(())`
///
/// # エラー
///
/// ファイルの読み書きや訓練処理に失敗した場合、`TrainError`を返します。
pub fn run(args: Args) -> Result<(), TrainError> {
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

    println!("Starting model training...");
    let model = train_model(&params)?;

    println!("Writing model to {}...", args.model_out.display());
    let file = File::create(&args.model_out)?;
    let mut encoder = zstd::stream::Encoder::new(file, 19)?;
    model.write_model(&mut encoder)?;
    encoder.finish()?;

    println!("Successfully trained and wrote the model.");
    Ok(())
}

/// 指定されたパラメータでモデルを訓練する
///
/// CLIに依存しないコアの訓練ロジックです。
///
/// # 引数
///
/// * `params` - 訓練パラメータ
///
/// # 戻り値
///
/// 訓練されたモデル
///
/// # エラー
///
/// ファイルの読み込みや訓練処理に失敗した場合、`TrainError`を返します。
pub fn train_model(params: &TrainingParams) -> Result<Model, TrainError> {
    let lexicon_rdr = File::open(&params.seed_lexicon)?;
    let char_prop_rdr = File::open(&params.char_def)?;
    let unk_handler_rdr = File::open(&params.seed_unk)?;
    let feature_templates_rdr = File::open(&params.feature_def)?;
    let rewrite_rules_rdr = File::open(&params.rewrite_def)?;

    let config = TrainerConfig::from_readers(
        lexicon_rdr,
        char_prop_rdr,
        unk_handler_rdr,
        feature_templates_rdr,
        rewrite_rules_rdr,
    )?;

    let trainer = Trainer::new(config)?
        .regularization_cost(params.lambda)
        .max_iter(params.max_iter)
        .num_threads(params.num_threads);

    let corpus_rdr = File::open(&params.corpus)?;
    let corpus = Corpus::from_reader(corpus_rdr)?;

    let model = trainer.train(corpus)?;
    Ok(model)
}