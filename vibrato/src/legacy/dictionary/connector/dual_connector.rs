//! デュアルコネクター実装
//!
//! このモジュールは、行列ベースとRawコネクターを組み合わせた
//! デュアルコネクターの実装を提供します。

use bincode::{Decode, Encode};

use crate::legacy::dictionary::connector::raw_connector::scorer::{
    Scorer, U31x8,
};
use crate::legacy::dictionary::connector::MatrixConnector;

/// デュアルコネクター
///
/// 行列ベースのコネクターとRawコネクターの両方を組み合わせた
/// ハイブリッド型のコネクターです。
#[derive(Decode, Encode)]
pub struct DualConnector {
    /// 行列ベースのコネクター
    matrix_connector: MatrixConnector,
    /// 右側接続IDマッピング
    right_conn_id_map: Vec<u16>,
    /// 左側接続IDマッピング
    left_conn_id_map: Vec<u16>,
    /// 右側特徴ID（SIMD最適化）
    right_feat_ids: Vec<U31x8>,
    /// 左側特徴ID（SIMD最適化）
    left_feat_ids: Vec<U31x8>,
    /// Rawスコアラー
    raw_scorer: Scorer,
}
