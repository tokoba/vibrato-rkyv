//! システム辞書のビルドモジュール
//!
//! このモジュールは、辞書ソースファイル(lex.csv, matrix.def等)から
//! バイナリ形式のシステム辞書を構築する機能を提供します。
//! matrix.defから構築する方法と、最適化されたbigram情報ファイルから構築する
//! 2つの方法をサポートしています。

use std::{fs::File, io};
use std::path::PathBuf;

use vibrato_rkyv::{dictionary::{DictionaryInner, SystemDictionaryBuilder}, errors::VibratoError};

use clap::Parser;

/// ビルドコマンドの引数
///
/// システム辞書をビルドするために必要な入力ファイルと出力先を指定します。
#[derive(Parser, Debug)]
#[clap(
    name = "build",
    about = "A program to build the system dictionary."
)]
pub struct Args {
    /// System lexicon file (lex.csv).
    #[clap(short = 'l', long)]
    lexicon_in: PathBuf,

    /// Matrix definition file (matrix.def).
    ///
    /// If this argument is not specified, the compiler considers `--bigram-right-in`,
    /// `--bigram-left-in`, and `--bigram-cost-in` arguments.
    #[clap(short = 'm', long)]
    matrix_in: Option<PathBuf>,

    /// Unknown word definition file (unk.def).
    #[clap(short = 'u', long)]
    unk_in: PathBuf,

    /// Character definition file (char.def).
    #[clap(short = 'c', long)]
    char_in: PathBuf,

    /// File to which the binary dictionary is output (in zstd).
    #[clap(short = 'o', long)]
    sysdic_out: PathBuf,

    /// Bi-gram information associated with right connection IDs (bigram.right).
    #[clap(long)]
    bigram_right_in: Option<PathBuf>,

    /// Bi-gram information associated with left connection IDs (bigram.left).
    #[clap(long)]
    bigram_left_in: Option<PathBuf>,

    /// Bi-gram cost file (bigram.cost).
    #[clap(long)]
    bigram_cost_in: Option<PathBuf>,

    /// Option to control trade-off between speed and memory.
    /// When setting it, the resulting model will be faster but larger.
    /// This option is enabled when bi-gram information is specified.
    #[clap(long)]
    dual_connector: bool,
}

/// ビルド処理中に発生する可能性のあるエラー
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// 不正な引数の組み合わせ
    ///
    /// `--matrix-in`または`--bigram-{right,left,cost}-in`のすべてが
    /// 指定されている必要があります。
    #[error(
        "Invalid argument combination: Either --matrix-in or all of \
        --bigram-{{right,left,cost}}-in must be specified."
    )]
    InvalidSourceArguments,

    /// 入出力エラー
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// 辞書構築エラー
    #[error("Dictionary building failed: {0}")]
    Vibrato(#[from] VibratoError),
}

/// コマンドライン引数からビルドソースを決定する
///
/// # 引数
///
/// * `args` - コマンドライン引数
///
/// # 戻り値
///
/// ビルドソースの種別と必要なファイルパス
///
/// # エラー
///
/// 不正な引数の組み合わせの場合、`BuildError::InvalidSourceArguments`を返します。
fn get_source_from_args(args: &Args) -> Result<BuildSource, BuildError> {
    if let Some(matrix_in) = &args.matrix_in {
        Ok(BuildSource::FromMatrix {
            lexicon: args.lexicon_in.clone(),
            matrix: matrix_in.clone(),
            char_def: args.char_in.clone(),
            unk_def: args.unk_in.clone(),
        })
    } else if let (Some(bigram_right_in), Some(bigram_left_in), Some(bigram_cost_in)) =
        (&args.bigram_right_in, &args.bigram_left_in, &args.bigram_cost_in)
    {
        Ok(BuildSource::FromBigram {
            lexicon: args.lexicon_in.clone(),
            bigram_right: bigram_right_in.clone(),
            bigram_left: bigram_left_in.clone(),
            bigram_cost: bigram_cost_in.clone(),
            char_def: args.char_in.clone(),
            unk_def: args.unk_in.clone(),
            dual_connector: args.dual_connector,
        })
    } else {
        Err(BuildError::InvalidSourceArguments)
    }
}

/// 辞書ビルドのソースファイル情報
///
/// ビルドに使用するファイルの種類と構成を表します。
pub enum BuildSource {
    /// matrix.defファイルから構築
    ///
    /// 従来の形式のmatrix.defファイルを使用します。
    FromMatrix {
        /// 語彙ファイル(lex.csv)のパス
        lexicon: PathBuf,
        /// 連接コスト定義ファイル(matrix.def)のパス
        matrix: PathBuf,
        /// 文字定義ファイル(char.def)のパス
        char_def: PathBuf,
        /// 未知語定義ファイル(unk.def)のパス
        unk_def: PathBuf,
    },
    /// 最適化されたbigram情報ファイルから構築
    ///
    /// モデル訓練で生成された最適化済みのbigramファイルを使用します。
    /// こちらの方が高速ですが、より大きな辞書になります。
    FromBigram {
        /// 語彙ファイル(lex.csv)のパス
        lexicon: PathBuf,
        /// 右接続ID情報ファイル(bigram.right)のパス
        bigram_right: PathBuf,
        /// 左接続ID情報ファイル(bigram.left)のパス
        bigram_left: PathBuf,
        /// バイグラムコストファイル(bigram.cost)のパス
        bigram_cost: PathBuf,
        /// 文字定義ファイル(char.def)のパス
        char_def: PathBuf,
        /// 未知語定義ファイル(unk.def)のパス
        unk_def: PathBuf,
        /// デュアルコネクタを使用するかどうか
        ///
        /// trueの場合、速度とメモリ使用量のトレードオフを速度優先にします。
        dual_connector: bool,
    },
}

/// ビルドコマンドを実行する
///
/// 指定されたソースファイルから辞書を構築し、zstd圧縮したバイナリ形式で出力します。
///
/// # 引数
///
/// * `args` - ビルドコマンドの引数
///
/// # 戻り値
///
/// 成功時は`Ok(())`
///
/// # エラー
///
/// ファイルの読み書きや辞書構築に失敗した場合、`BuildError`を返します。
pub fn run(args: Args) -> Result<(), BuildError> {
    let source = get_source_from_args(&args)?;

    println!("Compiling the system dictionary...");
    let dict = build_dictionary(&source)?;

    println!("Writing the system dictionary...");
    let file = File::create(&args.sysdic_out)?;
    let mut encoder = zstd::Encoder::new(file, 19)?;
    dict.write(&mut encoder)?;
    encoder.finish()?;

    println!("Successfully built the dictionary to {}", args.sysdic_out.display());
    Ok(())
}

/// 指定されたソースファイルから辞書を構築する
///
/// CLIに依存しないコアのビルドロジックです。
///
/// # 引数
///
/// * `source` - ビルドソース情報(ファイルパスと構築方法)
///
/// # 戻り値
///
/// 構築された辞書の内部表現
///
/// # エラー
///
/// ファイルの読み込みや辞書構築に失敗した場合、`BuildError`を返します。
pub fn build_dictionary(source: &BuildSource) -> Result<DictionaryInner, BuildError> {
    let dict = match source {
        BuildSource::FromMatrix { lexicon, matrix, char_def, unk_def } => {
            SystemDictionaryBuilder::from_readers(
                File::open(lexicon)?,
                File::open(matrix)?,
                File::open(char_def)?,
                File::open(unk_def)?,
            )?
        }
        BuildSource::FromBigram {
            lexicon,
            bigram_right,
            bigram_left,
            bigram_cost,
            char_def,
            unk_def,
            dual_connector,
        } => {
            SystemDictionaryBuilder::from_readers_with_bigram_info(
                File::open(lexicon)?,
                File::open(bigram_right)?,
                File::open(bigram_left)?,
                File::open(bigram_cost)?,
                File::open(char_def)?,
                File::open(unk_def)?,
                *dual_connector,
            )?
        }
    };
    Ok(dict)
}
