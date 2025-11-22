//! 未知語処理モジュール
//!
//! このモジュールは、辞書に登録されていない未知語を処理するための
//! 機能を提供します。

use bincode::{Decode, Encode};

/// 未知語エントリ
///
/// この構造体は、未知語の品詞情報とコストを保持します。
/// 各文字カテゴリに対して、どのような品詞として扱うかを定義します。
#[derive(Default, Debug, Clone, Decode, Encode, PartialEq, Eq)]
pub struct UnkEntry {
    /// カテゴリID
    pub cate_id: u16,
    /// 左側接続ID
    pub left_id: u16,
    /// 右側接続ID
    pub right_id: u16,
    /// 単語コスト
    pub word_cost: i16,
    /// 特徴文字列
    pub feature: String,
}

/// 未知語ハンドラー
///
/// この構造体は、未知語の処理に必要な情報を管理します。
/// カテゴリIDごとに未知語エントリを保持し、効率的な検索を可能にします。
#[derive(Decode, Encode)]
pub struct UnkHandler {
    /// カテゴリIDでインデックス化されたオフセット配列
    offsets: Vec<usize>,
    /// 未知語エントリの配列
    entries: Vec<UnkEntry>,
}
