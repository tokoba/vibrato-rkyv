//! 接続IDマッパーモジュール
//!
//! このモジュールは、接続IDの変換を行うマッパーを提供します。

use bincode::{Decode, Encode};

/// 接続IDマッパー
///
/// この構造体は、左側および右側の接続IDを変換するためのマッピングテーブルを保持します。
/// 辞書の圧縮や最適化のために、接続IDの再マッピングが必要な場合に使用されます。
#[derive(Decode, Encode)]
pub struct ConnIdMapper {
    /// 左側接続IDのマッピングテーブル
    left: Vec<u16>,
    /// 右側接続IDのマッピングテーブル
    right: Vec<u16>,
}
