//! 行列ベースのコネクター実装
//!
//! このモジュールは、接続コストを行列として保持し、
//! 高速なルックアップを提供する行列ベースのコネクターを実装します。

use bincode::{Decode, Encode};

/// 接続コストの行列
///
/// この構造体は、形態素間の接続コストを2次元行列として保持します。
/// 行列のインデックスは、右側の品詞IDと左側の品詞IDによって決定されます。
#[derive(Decode, Encode)]
pub struct MatrixConnector {
    /// 接続コストデータの平坦化された配列
    data: Vec<i16>,
    /// 右側の品詞数
    num_right: usize,
    /// 左側の品詞数
    num_left: usize,
}
