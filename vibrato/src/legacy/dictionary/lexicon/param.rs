//! 単語パラメータモジュール
//!
//! このモジュールは、単語のコストと接続IDのパラメータを管理します。

use bincode::{Decode, Encode};


/// 単語パラメータ
///
/// この構造体は、個々の単語エントリに関連付けられたパラメータを保持します。
/// 左右の接続IDと単語コストを含みます。
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Decode, Encode)]
pub struct WordParam {
    /// 左側接続ID（前の単語との接続用）
    pub left_id: u16,
    /// 右側接続ID（次の単語との接続用）
    pub right_id: u16,
    /// 単語コスト
    pub word_cost: i16,
}

/// 単語パラメータの集合
///
/// この構造体は、辞書内のすべての単語パラメータを配列として保持します。
#[derive(Decode, Encode)]
pub struct WordParams {
    /// パラメータの配列
    params: Vec<WordParam>,
}
