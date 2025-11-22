//! 単語マッピングモジュール
//!
//! このモジュールは、単語の表層形から辞書エントリへのマッピングを提供します。
//! トライ構造とポスティングリストを使用して効率的な検索を実現します。

pub mod posting;
pub mod trie;

use bincode::{Decode, Encode};

use crate::legacy::dictionary::lexicon::map::posting::Postings;
use crate::legacy::dictionary::lexicon::map::trie::Trie;

/// 単語マッピング
///
/// この構造体は、単語の表層形から辞書エントリへのマッピングを管理します。
/// トライ構造を使用して効率的な前方一致検索を行い、
/// ポスティングリストで各単語に対応するエントリを取得します。
#[derive(Decode, Encode)]
pub struct WordMap {
    /// トライ構造（文字列検索用）
    trie: Trie,
    /// ポスティングリスト（エントリリスト）
    postings: Postings,
}
