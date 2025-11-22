//! Vibrato 辞書コンパイラのメインエントリーポイント
//!
//! このモジュールは、形態素解析用の辞書をビルドするための様々なサブコマンドを提供します。
//! コーパスから辞書を構築、モデルの訓練、辞書ファイルの生成、レガシー辞書の変換など、
//! 辞書構築に関する全ての操作を統合したCLIツールです。

mod build;
mod dictgen;
mod full_build;
mod train;
mod transmute_legacy;

use clap::Parser;
use thiserror::Error;

use crate::{build::BuildError, dictgen::DictgenError, full_build::FullBuildError, train::TrainError, transmute_legacy::TransmuteLegacyError};


/// コマンドライン引数の構造体
///
/// `clap`を使用してコマンドライン引数をパースします。
#[derive(Parser, Debug)]
#[clap(name = "compile", version)]
struct Cli {
    /// 実行するサブコマンド
    #[clap(subcommand)]
    command: Command,
}

/// 利用可能なサブコマンド
///
/// 各サブコマンドは辞書構築プロセスの異なるフェーズに対応します。
#[derive(Parser, Debug)]
enum Command {
    /// コーパスから辞書をワンステップで構築します
    ///
    /// 訓練、辞書生成、ビルドを一度に実行し、すべての中間ファイルと最終的な辞書を生成します。
    FullBuild(full_build::Args),

    /// コーパスからモデルを訓練します
    ///
    /// 教師データとなるコーパスから統計モデルを学習し、重みパラメータを推定します。
    Train(train::Args),

    /// モデルから辞書ファイルを生成します
    ///
    /// 訓練されたモデルから、形態素解析に必要な辞書ファイル群を出力します。
    Dictgen(dictgen::Args),

    /// ソースファイルからバイナリ辞書を構築します
    ///
    /// 辞書ソースファイル(lex.csv, matrix.def等)からバイナリ形式の辞書を生成します。
    Build(build::Args),

    /// レガシーのVibrato辞書をbincode形式からrkyv形式に変換します
    ///
    /// 古い形式の辞書ファイルを新しいrkyv形式に変換します。
    Transmute(transmute_legacy::Args),
}

/// コンパイラの実行中に発生する可能性のあるエラー
///
/// 各サブコマンドで発生したエラーをラップします。
#[derive(Debug, Error)]
pub enum CompileError {
    /// フルビルド実行中のエラー
    #[error(transparent)]
    FullBuildError(#[from] FullBuildError),
    /// モデル訓練中のエラー
    #[error(transparent)]
    TrainError(#[from] TrainError),
    /// 辞書生成中のエラー
    #[error(transparent)]
    DictgenError(#[from] DictgenError),
    /// 辞書ビルド中のエラー
    #[error(transparent)]
    BuildError(#[from] BuildError),
    /// レガシー辞書変換中のエラー
    #[error(transparent)]
    TransmuteLegacy(#[from] TransmuteLegacyError),
}

/// メイン関数
///
/// コマンドライン引数をパースし、指定されたサブコマンドを実行します。
///
/// # 戻り値
///
/// 実行が成功した場合は`Ok(())`、失敗した場合は対応する`CompileError`を返します。
///
/// # エラー
///
/// 各サブコマンドの実行中にエラーが発生した場合、そのエラーが返されます。
fn main() -> Result<(), CompileError> {
    let cli = Cli::parse();
    match cli.command {
        Command::FullBuild(args) => Ok(full_build::run(args)?),
        Command::Train(args) => Ok(train::run(args)?),
        Command::Dictgen(args) => Ok(dictgen::run(args)?),
        Command::Build(args) => Ok(build::run(args)?),
        Command::Transmute(args) => Ok(transmute_legacy::run(args)?),
    }
}
