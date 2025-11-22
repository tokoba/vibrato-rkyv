//! Rawコネクター実装
//!
//! このモジュールは、特徴ベースの接続コスト計算を行うRawコネクターを実装します。
//! 行列の代わりに、特徴テンプレートとスコアラーを使用して動的にコストを計算します。

pub mod scorer;


use bincode::{Decode, Encode};

use crate::legacy::dictionary::connector::raw_connector::scorer::{
    Scorer, U31x8,
};

/// Rawコネクター
///
/// 特徴ベースの接続コスト計算を行うコネクターです。
/// 行列を使用せず、特徴IDとスコアラーを使用して動的にコストを計算します。
#[derive(Decode, Encode)]
pub struct RawConnector {
    /// 右側特徴ID（SIMD最適化）
    right_feat_ids: Vec<U31x8>,
    /// 左側特徴ID（SIMD最適化）
    left_feat_ids: Vec<U31x8>,
    /// 特徴テンプレートのサイズ
    feat_template_size: usize,
    /// コスト計算用スコアラー
    scorer: Scorer,
}
