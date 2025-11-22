//! トライ構造による高速文字列検索
//!
//! このモジュールは、トライ構造を使用した高速な文字列検索機能を提供します。

use rkyv::{Archive, Deserialize, Serialize};

use crate::errors::{Result, VibratoError};

/// ダブル配列トライ
#[derive(Archive, Serialize, Deserialize)]
pub struct Trie {
    da: crawdad_rkyv::Trie,
}

impl Trie {
    /// レコードからトライを構築します。
    pub fn from_records<K>(records: &[(K, u32)]) -> Result<Self>
    where
        K: AsRef<str>,
    {
        Ok(Self {
            da: crawdad_rkyv::Trie::from_records(records.iter().map(|(k, v)| (k, *v)))
                .map_err(|e| VibratoError::invalid_argument("records", e.to_string()))?,
        })
    }

    #[inline(always)]
    pub fn common_prefix_iterator<'a>(
        &'a self,
        input: &'a [char],
    ) -> impl Iterator<Item = TrieMatch> + 'a {
        self.da
            .common_prefix_search(input.iter().cloned())
            .map(move |(value, end_char)| TrieMatch::new(value, end_char))
    }
}

/// トライマッチング結果
#[derive(Debug, Eq, PartialEq, Clone)]
pub struct TrieMatch {
    pub value: u32,
    pub end_char: usize,
}

impl TrieMatch {
    /// 新しいマッチング結果を作成します。
    #[inline(always)]
    pub const fn new(value: u32, end_char: usize) -> Self {
        Self { value, end_char }
    }
}

impl ArchivedTrie {
    /// 共通接頭辞検索のイテレータを取得します（アーカイブ版）。
    #[inline(always)]
    pub fn common_prefix_iterator<'a>(
        &'a self,
        input: &'a [char],
    ) -> impl Iterator<Item = TrieMatch> + 'a {
        self.da
            .common_prefix_search(input.iter().cloned())
            .map(move |(value, end_char)| TrieMatch::new(value, end_char))
    }
}
