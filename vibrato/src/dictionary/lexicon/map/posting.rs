//! ポスティングリスト
//!
//! このモジュールは、単語IDのポスティングリストを管理します。

use rkyv::rend::u32_le;
use rkyv::{Archive, Deserialize, Serialize};

use crate::errors::Result;
use crate::utils::FromU32;

/// ポスティングリスト
#[derive(Archive, Serialize, Deserialize)]
pub struct Postings {
    // Sets of ids are stored by interleaving their length and values.
    // Then, 8 bits would be sufficient to represent the length in most cases, and
    // serializing `data` into a byte sequence can reduce the memory usage.
    // However, the memory usage is slight compared to that of the connection matrix.
    // Thus, we implement `data` as `Vec<u32>` for simplicity.
    data: Vec<u32>,
}

impl Postings {
    /// 指定されたインデックスのIDイテレータを取得します。
    #[inline(always)]
    pub fn ids(&'_ self, i: usize) -> impl Iterator<Item = u32> + '_ {
        let len = usize::from_u32(self.data[i]);
        self.data[i + 1..i + 1 + len].iter().cloned()
    }
}

/// ポスティングリストを構築するビルダー
#[derive(Default)]
pub struct PostingsBuilder {
    data: Vec<u32>,
}

impl PostingsBuilder {
    /// 新しいビルダーを作成します。
    pub fn new() -> Self {
        Self::default()
    }

    /// IDリストを追加します。
    #[inline(always)]
    pub fn push(&mut self, ids: &[u32]) -> Result<usize> {
        let offset = self.data.len();
        self.data.push(ids.len().try_into()?);
        self.data.extend_from_slice(ids);
        Ok(offset)
    }

    /// ポスティングリストを構築します。
    #[allow(clippy::missing_const_for_fn)]
    pub fn build(self) -> Postings {
        Postings { data: self.data }
    }
}

impl ArchivedPostings {
    /// 指定されたインデックスのIDイテレータを取得します（アーカイブ版）。
    #[inline(always)]
    pub fn ids(&'_ self, i: usize) -> impl Iterator<Item = u32_le> + '_ {
        let len = usize::from_u32(self.data[i].to_native());
        self.data[i + 1..i + 1 + len].iter().cloned()
    }
}