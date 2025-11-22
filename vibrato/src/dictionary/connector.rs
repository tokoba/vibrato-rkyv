//! 接続コスト計算のためのコネクター
//!
//! このモジュールは、形態素間の接続コストを計算するための
//! 様々なコネクター実装を提供します。

mod dual_connector;
mod matrix_connector;
mod raw_connector;

use rkyv::{Archive, Deserialize, Serialize};

pub use crate::dictionary::connector::dual_connector::DualConnector;
pub use crate::dictionary::connector::matrix_connector::MatrixConnector;
pub use crate::dictionary::connector::raw_connector::RawConnector;
use crate::dictionary::mapper::ConnIdMapper;

/// コネクターのビュー機能を提供するトレイト
pub trait ConnectorView {
    /// 左接続IDの最大数を返します。
    fn num_left(&self) -> usize;

    /// 右接続IDの最大数を返します。
    fn num_right(&self) -> usize;
}

/// 接続ID のマッピング機能を提供するトレイト
pub trait Connector: ConnectorView {
    /// 接続IDをマッピングします。
    ///
    /// # 注意
    ///
    /// `Dictionary` のメンバー間で接続IDマッピングの一貫性を保つため、
    /// この関数は公開しないでください。一貫性は `Dictionary` で管理されます。
    fn map_connection_ids(&mut self, mapper: &ConnIdMapper);
}

/// 接続コスト計算機能を提供するトレイト
pub trait ConnectorCost: ConnectorView {
    /// 接続行列の値を取得します。
    ///
    /// # 引数
    ///
    /// * `right_id` - 右接続ID
    /// * `left_id` - 左接続ID
    ///
    /// # 戻り値
    ///
    /// 接続コスト
    fn cost(&self, right_id: u16, left_id: u16) -> i32;
}

/// コネクターのラッパー列挙型
#[derive(Archive, Serialize, Deserialize)]
pub enum ConnectorWrapper {
    Matrix(MatrixConnector),
    Raw(RawConnector),
    Dual(DualConnector),
}

impl ConnectorView for ConnectorWrapper {
    fn num_left(&self) -> usize {
        match self {
            Self::Matrix(c) => c.num_left(),
            Self::Raw(c) => c.num_left(),
            Self::Dual(c) => c.num_left(),
        }
    }
    fn num_right(&self) -> usize {
        match self {
            Self::Matrix(c) => c.num_right(),
            Self::Raw(c) => c.num_right(),
            Self::Dual(c) => c.num_right(),
        }
    }
}

impl Connector for ConnectorWrapper {
    fn map_connection_ids(&mut self, mapper: &ConnIdMapper) {
        match self {
            Self::Matrix(c) => c.map_connection_ids(mapper),
            Self::Raw(c) => c.map_connection_ids(mapper),
            Self::Dual(c) => c.map_connection_ids(mapper),
        }
    }
}

impl ConnectorView for ArchivedConnectorWrapper {
    fn num_left(&self) -> usize {
        match self {
            Self::Matrix(c) => c.num_left(),
            Self::Raw(c) => c.num_left(),
            Self::Dual(c) => c.num_left(),
        }
    }
    fn num_right(&self) -> usize {
        match self {
            Self::Matrix(c) => c.num_right(),
            Self::Raw(c) => c.num_right(),
            Self::Dual(c) => c.num_right(),
        }
    }
}

impl ConnectorCost for ConnectorWrapper {
    fn cost(&self, right_id: u16, left_id: u16) -> i32 {
        match self {
            Self::Matrix(c) => c.cost(right_id, left_id),
            Self::Raw(c) => c.cost(right_id, left_id),
            Self::Dual(c) => c.cost(right_id, left_id),
        }
    }
}

impl ConnectorCost for ArchivedConnectorWrapper {
    fn cost(&self, right_id: u16, left_id: u16) -> i32 {
        match self {
            Self::Matrix(c) => c.cost(right_id, left_id),
            Self::Raw(c) => c.cost(right_id, left_id),
            Self::Dual(c) => c.cost(right_id, left_id),
        }
    }
}