//! 接続コスト計算モジュール
//!
//! このモジュールは、形態素間の接続コストを計算するための
//! 各種コネクター実装を提供します。

mod dual_connector;
mod matrix_connector;
mod raw_connector;

use bincode::{Decode, Encode};

pub use crate::legacy::dictionary::connector::dual_connector::DualConnector;
pub use crate::legacy::dictionary::connector::matrix_connector::MatrixConnector;
pub use crate::legacy::dictionary::connector::raw_connector::RawConnector;

/// コネクターのラッパー列挙型
///
/// この列挙型は、異なる種類の接続コスト計算方法を統一的に扱うための
/// ラッパーです。Matrix、Raw、Dualの3つのバリアントがあります。
#[derive(Decode, Encode)]
pub enum ConnectorWrapper {
    /// 行列ベースのコネクター
    Matrix(MatrixConnector),
    /// 生（Raw）コネクター
    Raw(RawConnector),
    /// デュアルコネクター（MatrixとRawの組み合わせ）
    Dual(DualConnector),
}
