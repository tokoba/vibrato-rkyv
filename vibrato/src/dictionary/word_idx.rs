//! 単語識別子
//!
//! このモジュールは、辞書内の単語を一意に識別するための
//! インデックス構造を提供します。

use rkyv::{Archive, Deserialize, Serialize};

use crate::dictionary::LexType;

/// 単語の識別子
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash, Archive, Serialize, Deserialize)]
pub struct WordIdx {
    /// この単語を含む辞書の種類
    pub lex_type: LexType,

    /// この単語のID
    pub word_id: u32,
}

impl Default for WordIdx {
    fn default() -> Self {
        Self::new(LexType::System, u32::MAX)
    }
}

impl WordIdx {
    /// 新しいインスタンスを作成します。
    #[inline(always)]
    pub(crate) const fn new(lex_type: LexType, word_id: u32) -> Self {
        Self { lex_type, word_id }
    }
}
