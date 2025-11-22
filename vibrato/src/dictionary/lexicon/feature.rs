//! 単語の素性情報
//!
//! このモジュールは、単語に関連付けられた素性（品詞情報など）を管理します。

use rkyv::{Archive, Deserialize, Serialize};

/// 単語の素性情報を管理する構造体
#[derive(Default, Archive, Serialize, Deserialize)]
pub struct WordFeatures {
    features: Vec<String>,
}

impl WordFeatures {
    /// 素性のイテレータから新しいインスタンスを作成します。
    pub fn new<I, S>(features: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            features: features
                .into_iter()
                .map(|s| s.as_ref().to_string())
                .collect(),
        }
    }

    #[inline(always)]
    pub fn get(&self, word_id: usize) -> &str {
        &self.features[word_id]
    }
}

impl ArchivedWordFeatures {
    /// 単語IDから素性を取得します（アーカイブ版）。
    #[inline(always)]
    pub fn get(&self, word_id: usize) -> &str {
        &self.features[word_id]
    }
}
