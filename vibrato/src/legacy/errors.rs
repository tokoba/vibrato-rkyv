//! エラー定義
//!
//! このモジュールは、Vibratoレガシー形式で発生する可能性のあるエラー型を定義します。

use std::error::Error;
use std::fmt;

/// Vibrato用の特化型Result型
///
/// この型は、VibratoErrorをデフォルトのエラー型として使用する
/// 標準ライブラリのResult型のエイリアスです。
pub type Result<T, E = VibratoError> = std::result::Result<T, E>;

/// Vibratoのエラー型
///
/// この列挙型は、Vibratoレガシー形式の処理中に発生する可能性のある
/// すべてのエラーを表します。各バリアントは、異なる種類のエラーに対応しています。
#[derive(Debug)]
pub enum VibratoError {
    /// 無効な引数エラーのバリアント
    ///
    /// 関数やメソッドに渡された引数が無効な場合に使用されます。
    /// 詳細は[`InvalidArgumentError`]を参照してください。
    InvalidArgument(InvalidArgumentError),

    /// 無効な形式エラーのバリアント
    ///
    /// 入力データの形式が期待と異なる場合に使用されます。
    /// 詳細は[`InvalidFormatError`]を参照してください。
    InvalidFormat(InvalidFormatError),

    /// 整数変換エラーのバリアント
    ///
    /// [`TryFromIntError`](std::num::TryFromIntError)のラッパーです。
    TryFromInt(std::num::TryFromIntError),

    /// 浮動小数点数解析エラーのバリアント
    ///
    /// [`ParseFloatError`](std::num::ParseFloatError)のラッパーです。
    ParseFloat(std::num::ParseFloatError),

    /// 整数解析エラーのバリアント
    ///
    /// [`ParseIntError`](std::num::ParseIntError)のラッパーです。
    ParseInt(std::num::ParseIntError),

    /// bincodeデコードエラーのバリアント
    ///
    /// [`DecodeError`](bincode::error::DecodeError)のラッパーです。
    BincodeDecode(bincode::error::DecodeError),

    /// bincodeエンコードエラーのバリアント
    ///
    /// [`EncodeError`](bincode::error::EncodeError)のラッパーです。
    BincodeEncode(bincode::error::EncodeError),

    /// 標準I/Oエラーのバリアント
    ///
    /// [`std::io::Error`]のラッパーです。
    StdIo(std::io::Error),

    /// UTF-8エラーのバリアント
    ///
    /// [`std::str::Utf8Error`]のラッパーです。
    Utf8(std::str::Utf8Error),

    /// CRFエラーのバリアント
    ///
    /// [`RucrfError`](rucrf::errors::RucrfError)のラッパーです。
    /// このバリアントは`train`フィーチャーが有効な場合にのみ利用可能です。
    #[cfg(feature = "train")]
    Crf(rucrf::errors::RucrfError),
}

impl VibratoError {
    /// 無効な引数エラーを作成します。
    ///
    /// # 引数
    ///
    /// * `arg` - 無効な引数の名前
    /// * `msg` - エラーメッセージ
    ///
    /// # 戻り値
    ///
    /// 作成された`VibratoError::InvalidArgument`バリアント
    pub(crate) fn invalid_argument<S>(arg: &'static str, msg: S) -> Self
    where
        S: Into<String>,
    {
        Self::InvalidArgument(InvalidArgumentError {
            arg,
            msg: msg.into(),
        })
    }
}

impl fmt::Display for VibratoError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::InvalidArgument(e) => e.fmt(f),
            Self::InvalidFormat(e) => e.fmt(f),
            Self::TryFromInt(e) => e.fmt(f),
            Self::ParseFloat(e) => e.fmt(f),
            Self::ParseInt(e) => e.fmt(f),
            Self::BincodeDecode(e) => e.fmt(f),
            Self::BincodeEncode(e) => e.fmt(f),
            Self::StdIo(e) => e.fmt(f),
            Self::Utf8(e) => e.fmt(f),

            #[cfg(feature = "train")]
            Self::Crf(e) => e.fmt(f),
        }
    }
}

impl Error for VibratoError {}

/// 引数が無効な場合に使用されるエラー
///
/// このエラーは、関数やメソッドに渡された引数が無効な値や形式を持つ場合に発生します。
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

/// 入力形式が無効な場合に使用されるエラー
///
/// このエラーは、入力データの形式が期待される形式と異なる場合に発生します。
#[derive(Debug)]
pub struct InvalidFormatError {
    /// 形式の名前
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

impl From<bincode::error::DecodeError> for VibratoError {
    fn from(error: bincode::error::DecodeError) -> Self {
        Self::BincodeDecode(error)
    }
}

impl From<bincode::error::EncodeError> for VibratoError {
    fn from(error: bincode::error::EncodeError) -> Self {
        Self::BincodeEncode(error)
    }
}

impl From<std::io::Error> for VibratoError {
    fn from(error: std::io::Error) -> Self {
        Self::StdIo(error)
    }
}

impl From<std::str::Utf8Error> for VibratoError {
    fn from(error: std::str::Utf8Error) -> Self {
        Self::Utf8(error)
    }
}

#[cfg(feature = "train")]
impl From<rucrf::errors::RucrfError> for VibratoError {
    fn from(error: rucrf::errors::RucrfError) -> Self {
        Self::Crf(error)
    }
}
