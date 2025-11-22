//! 単語特徴モジュール
//!
//! このモジュールは、単語に関連付けられた特徴文字列を管理します。

use bincode::{Decode, Encode};

/// 単語特徴
///
/// この構造体は、各単語エントリに対応する特徴文字列を保持します。
/// 特徴文字列には、品詞、活用形、読みなどの情報が含まれます。
#[derive(Default, Decode, Encode)]
pub struct WordFeatures {
    /// 特徴文字列の配列
    features: Vec<String>,
}
