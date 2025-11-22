//! エラー型の定義
//!
//! このモジュールは、Vibratoライブラリで使用されるすべてのエラー型を定義します。

use std::error::Error;
use std::fmt::{self, Debug};

#[cfg(feature = "legacy")]
use crate::legacy;

/// Vibrato専用のResult型
///
/// エラー型としてデフォルトで[`VibratoError`]を使用します。
pub type Result<T, E = VibratoError> = std::result::Result<T, E>;

/// Vibratoのエラー型
///
/// このライブラリで発生する可能性のあるすべてのエラーを表現します。
/// 各バリアントは特定のエラー条件に対応しています。
#[derive(Debug, thiserror::Error)]
pub enum VibratoError {
    /// 無効な引数エラー
    ///
    /// [`InvalidArgumentError`]のエラーバリアント。
    #[error(transparent)]
    InvalidArgument(InvalidArgumentError),

    /// 無効なフォーマットエラー
    ///
    /// [`InvalidFormatError`]のエラーバリアント。
    #[error(transparent)]
    InvalidFormat(InvalidFormatError),

    /// 無効な状態エラー
    ///
    /// [`InvalidStateError`]のエラーバリアント。
    #[error(transparent)]
    InvalidState(InvalidStateError),

    /// 整数変換エラー
    ///
    /// [`TryFromIntError`](std::num::TryFromIntError)のエラーバリアント。
    #[error(transparent)]
    TryFromInt(std::num::TryFromIntError),

    /// 浮動小数点数パースエラー
    ///
    /// [`ParseFloatError`](std::num::ParseFloatError)のエラーバリアント。
    #[error(transparent)]
    ParseFloat(std::num::ParseFloatError),

    /// 整数パースエラー
    ///
    /// [`ParseIntError`](std::num::ParseIntError)のエラーバリアント。
    #[error(transparent)]
    ParseInt(std::num::ParseIntError),

    /// 標準I/Oエラー
    ///
    /// [`std::io::Error`]のエラーバリアント。
    #[error(transparent)]
    StdIo(std::io::Error),

    /// UTF-8エンコーディングエラー
    ///
    /// [`std::str::Utf8Error`]のエラーバリアント。
    #[error(transparent)]
    Utf8(std::str::Utf8Error),

    /// ディレクトリが指定されたエラー
    ///
    /// ファイルが期待される場所にディレクトリが指定された場合に発生します。
    #[error("The path '{0}' is a directory, but a file was expected.")]
    PathIsDirectory(std::path::PathBuf),

    /// バックグラウンドスレッドパニックエラー
    ///
    /// バックグラウンドスレッドがパニックした場合に発生します。
    #[error("Background thread panicked: {0}")]
    ThreadPanic(String),

    /// CRFライブラリのエラー
    ///
    /// [`RucrfError`](rucrf_rkyv::errors::RucrfError)のエラーバリアント。
    /// `train`フィーチャーが有効な場合のみ利用可能です。
    #[cfg(feature = "train")]
    #[error(transparent)]
    Crf(rucrf_rkyv::errors::RucrfError),

    /// ダウンロードエラー
    ///
    /// [`DownloadError`]のエラーバリアント。
    /// `download`フィーチャーが有効な場合のみ利用可能です。
    #[cfg(feature = "download")]
    #[error(transparent)]
    Download(#[from] DownloadError),

    /// レガシーフォーマットのエラー
    ///
    /// [`VibratoError`](vibrato::errors::VibratoError)のエラーバリアント。
    /// `legacy`フィーチャーが有効な場合のみ利用可能です。
    #[cfg(feature = "legacy")]
    #[error(transparent)]
    Legacy(#[from] legacy::errors::VibratoError),

    /// I/Oエラー
    ///
    /// [`std::io::Error`](std::io::Error)のエラーバリアント。
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    /// rkyvシリアライゼーションエラー
    ///
    /// [`rkyv::rancor::Error`](rkyv::rancor::Error)のエラーバリアント。
    #[error(transparent)]
    RkyvError(#[from] rkyv::rancor::Error),

    /// 一時ファイルの永続化エラー
    ///
    /// [`tempfile::PathPersistError`](tempfile::PathPersistError)のエラーバリアント。
    #[error(transparent)]
    PathPersist(#[from] tempfile::PersistError),
}

impl VibratoError {
    /// 無効な引数エラーを生成します
    ///
    /// # 引数
    ///
    /// * `arg` - 引数の名前
    /// * `msg` - エラーメッセージ
    pub(crate) fn invalid_argument<S>(arg: &'static str, msg: S) -> Self
    where
        S: Into<String>,
    {
        Self::InvalidArgument(InvalidArgumentError {
            arg,
            msg: msg.into(),
        })
    }

    /// 無効なフォーマットエラーを生成します
    ///
    /// # 引数
    ///
    /// * `arg` - フォーマット名
    /// * `msg` - エラーメッセージ
    pub(crate) fn invalid_format<S>(arg: &'static str, msg: S) -> Self
    where
        S: Into<String>,
    {
        Self::InvalidFormat(InvalidFormatError {
            arg,
            msg: msg.into(),
        })
    }

    /// 無効な状態エラーを生成します
    ///
    /// # 引数
    ///
    /// * `msg` - エラーメッセージ
    /// * `cause` - エラーの原因
    pub(crate) fn invalid_state<S, M>(msg: S, cause: M) -> Self
    where
        S: Into<String>,
        M: Into<String>,
    {
        Self::InvalidState(InvalidStateError {
            msg: msg.into(),
            cause: cause.into(),
        })
    }
}

/// 引数が無効な場合に使用されるエラー
#[derive(Debug)]
pub struct InvalidArgumentError {
    /// 引数の名前
    pub(crate) arg: &'static str,

    /// エラーメッセージ
    pub(crate) msg: String,
}

impl fmt::Display for InvalidArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InvalidArgumentError: {}: {}", self.arg, self.msg)
    }
}

impl Error for InvalidArgumentError {}

/// 入力フォーマットが無効な場合に使用されるエラー
#[derive(Debug)]
pub struct InvalidFormatError {
    /// フォーマットの名前
    pub(crate) arg: &'static str,

    /// エラーメッセージ
    pub(crate) msg: String,
}

impl fmt::Display for InvalidFormatError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InvalidFormatError: {}: {}", self.arg, self.msg)
    }
}

impl Error for InvalidFormatError {}

/// 状態が無効な場合に使用されるエラー
#[derive(Debug)]
pub struct InvalidStateError {
    /// エラーメッセージ
    pub(crate) msg: String,

    /// エラーの根本原因
    pub(crate) cause: String,
}

impl fmt::Display for InvalidStateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "InvalidStateError: {}: {}", self.msg, self.cause)
    }
}

impl Error for InvalidStateError {}

/// ダウンロード関連のエラー
///
/// `download`フィーチャーが有効な場合のみ利用可能です。
/// 辞書ファイルのダウンロード中に発生する可能性のあるエラーを表現します。
#[cfg(feature = "download")]
#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    /// ネットワークリクエストの失敗
    #[error("Network request failed")]
    Request(#[from] reqwest::Error),

    /// I/Oエラー
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// ダウンロードファイルのチェックサム不一致
    ///
    /// ダウンロードされたファイルが破損している可能性があります。
    #[error("Downloaded file checksum mismatch. It may be corrupted.")]
    HashMismatch,

    /// 展開されたファイルが存在しない
    #[error("The extracted file does not exist.")]
    ExtractedFileNotFound,

    /// 展開された辞書のチェックサム不一致
    ///
    /// 展開されたファイルが破損している可能性があります。
    #[error("Extracted dictionary checksum mismatch. The extracted file may be corrupted.")]
    ExtractedHashMismatch,

    /// HTTPステータスエラー
    #[error("HTTP error: {0}")]
    HttpStatus(reqwest::StatusCode),

    /// パスの永続化エラー
    #[error(transparent)]
    PathPersist(#[from] tempfile::PersistError),
}

impl From<std::num::TryFromIntError> for VibratoError {
    fn from(error: std::num::TryFromIntError) -> Self {
        Self::TryFromInt(error)
    }
}

impl From<std::num::ParseFloatError> for VibratoError {
    fn from(error: std::num::ParseFloatError) -> Self {
        Self::ParseFloat(error)
    }
}

impl From<std::num::ParseIntError> for VibratoError {
    fn from(error: std::num::ParseIntError) -> Self {
        Self::ParseInt(error)
    }
}

impl From<std::str::Utf8Error> for VibratoError {
    fn from(error: std::str::Utf8Error) -> Self {
        Self::Utf8(error)
    }
}

#[cfg(feature = "train")]
impl From<rucrf_rkyv::errors::RucrfError> for VibratoError {
    fn from(error: rucrf_rkyv::errors::RucrfError) -> Self {
        Self::Crf(error)
    }
}
