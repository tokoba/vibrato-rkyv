//! 語彙辞書モジュール
//!
//! このモジュールは、単語の辞書データを管理します。
//! 単語のマッピング、パラメータ、特徴情報を含みます。

mod feature;
mod map;
mod param;


use bincode::{Decode, Encode};

use crate::legacy::dictionary::lexicon::feature::WordFeatures;
use crate::legacy::dictionary::lexicon::map::WordMap;
use crate::legacy::dictionary::lexicon::param::WordParams;
use crate::legacy::dictionary::LexType;


/// 単語の語彙辞書
///
/// この構造体は、単語の表層形から内部データへのマッピング、
/// 単語のパラメータ（コスト、品詞IDなど）、および特徴文字列を管理します。
#[derive(Decode, Encode)]
pub struct Lexicon {
    /// 単語マッピング（表層形からエントリへ）
    map: WordMap,
    /// 単語パラメータ（コスト、品詞IDなど）
    params: WordParams,
    /// 単語特徴（特徴文字列）
    features: WordFeatures,
    /// 辞書種別
    lex_type: LexType,
}
