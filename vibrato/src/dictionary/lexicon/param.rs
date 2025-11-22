//! 単語のパラメータ情報
//!
//! このモジュールは、単語の接続IDとコストなどのパラメータを管理します。

use rkyv::{Archive, Deserialize, Serialize};

use crate::dictionary::mapper::ConnIdMapper;

/// 単語のパラメータ（接続IDとコスト）
#[derive(Debug, Default, Clone, Copy, Eq, PartialEq, Archive, Serialize, Deserialize)]
pub struct WordParam {
    pub left_id: u16,
    pub right_id: u16,
    pub word_cost: i16,
}

impl WordParam {
    /// 新しい単語パラメータを作成します。
    #[inline(always)]
    pub const fn new(left_id: u16, right_id: u16, word_cost: i16) -> Self {
        Self {
            left_id,
            right_id,
            word_cost,
        }
    }
}

impl ArchivedWordParam {
    /// ネイティブ形式に変換します。
    pub fn to_native(&self) -> WordParam {
        WordParam {
            left_id: self.left_id.to_native(),
            right_id: self.right_id.to_native(),
            word_cost: self.word_cost.to_native(),
        }
    }
}

/// 単語パラメータのコレクション
#[derive(Archive, Serialize, Deserialize)]
pub struct WordParams {
    params: Vec<WordParam>,
}

impl WordParams {
    /// パラメータのイテレータから新しいインスタンスを作成します。
    pub fn new<I>(params: I) -> Self
    where
        I: IntoIterator<Item = WordParam>,
    {
        Self {
            params: params.into_iter().collect(),
        }
    }

    /// 単語IDからパラメータを取得します。
    #[inline(always)]
    pub fn get(&self, word_id: usize) -> WordParam {
        self.params[word_id]
    }

    /// パラメータの数を取得します。
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.params.len()
    }

    /// 接続IDをマッピングします。
    pub fn map_connection_ids(&mut self, mapper: &ConnIdMapper) {
        for p in &mut self.params {
            p.left_id = mapper.left(p.left_id);
            p.right_id = mapper.right(p.right_id);
        }
    }
}

impl ArchivedWordParams {
    /// 単語IDからパラメータを取得します（アーカイブ版）。
    #[inline(always)]
    pub fn get(&self, word_id: usize) -> WordParam {
        self.params[word_id].to_native()
    }
}