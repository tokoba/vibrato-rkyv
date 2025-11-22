//! 辞書構築のためのビルダー
//!
//! このモジュールは、MeCab形式の辞書ファイルから [`DictionaryInner`] を構築するための
//! ビルダーを提供します。

use std::io::Read;

use crate::dictionary::connector::{DualConnector, MatrixConnector, RawConnector};
use crate::dictionary::{
    CharProperty, ConnectorWrapper, DictionaryInner, LexType, Lexicon, UnkHandler,
};
use crate::errors::{Result, VibratoError};

use super::lexicon::RawWordEntry;

/// システム辞書エントリから [`DictionaryInner`] を構築するビルダー
pub struct SystemDictionaryBuilder {}

impl SystemDictionaryBuilder {
    /// パースされたコンポーネントから `DictionaryInner` を構築します。
    ///
    /// # 引数
    ///
    /// * `system_word_entries` - システム辞書の単語エントリ
    /// * `connector` - 接続コスト計算器
    /// * `char_prop` - 文字プロパティ
    /// * `unk_handler` - 未知語ハンドラー
    ///
    /// # 戻り値
    ///
    /// 成功時は `Ok(DictionaryInner)` を返します。
    ///
    /// # エラー
    ///
    /// 辞書の検証に失敗した場合にエラーを返します。
    pub(crate) fn build(
        system_word_entries: &[RawWordEntry],
        connector: ConnectorWrapper,
        char_prop: CharProperty,
        unk_handler: UnkHandler,
    ) -> Result<DictionaryInner> {
        let system_lexicon = Lexicon::from_entries(system_word_entries, LexType::System)?;

        if !system_lexicon.verify(&connector) {
            return Err(VibratoError::invalid_argument(
                "system_lexicon_rdr",
                "system_lexicon_rdr includes invalid connection ids.",
            ));
        }
        if !unk_handler.verify(&connector) {
            return Err(VibratoError::invalid_argument(
                "unk_handler_rdr",
                "unk_handler_rdr includes invalid connection ids.",
            ));
        }

        Ok(DictionaryInner {
            system_lexicon,
            user_lexicon: None,
            connector,
            mapper: None,
            char_prop,
            unk_handler,
        })
    }

    /// MeCab形式のシステムエントリから新しい [`DictionaryInner`] を作成します。
    ///
    /// メモリ使用量を削減したい場合は [`from_readers_with_bigram_info()`](Self::from_readers_with_bigram_info)
    /// の使用を検討してください。
    ///
    /// # 引数
    ///
    ///  - `system_lexicon_rdr`: 辞書ファイル `*.csv` のリーダー
    ///  - `connector_rdr`: 接続行列ファイル `matrix.def` のリーダー
    ///  - `char_prop_rdr`: 文字定義ファイル `char.def` のリーダー
    ///  - `unk_handler_rdr`: 未知語定義ファイル `unk.def` のリーダー
    ///
    /// # エラー
    ///
    /// 入力フォーマットが不正な場合に [`VibratoError`] を返します。
    pub fn from_readers<S, C, P, U>(
        mut system_lexicon_rdr: S,
        connector_rdr: C,
        char_prop_rdr: P,
        unk_handler_rdr: U,
    ) -> Result<DictionaryInner>
    where
        S: Read,
        C: Read,
        P: Read,
        U: Read,
    {
        let mut system_lexicon_buf = vec![];
        system_lexicon_rdr.read_to_end(&mut system_lexicon_buf)?;
        let system_word_entries = Lexicon::parse_csv(&system_lexicon_buf, "lex.csv")?;
        let connector = MatrixConnector::from_reader(connector_rdr)?;
        let char_prop = CharProperty::from_reader(char_prop_rdr)?;
        let unk_handler = UnkHandler::from_reader(unk_handler_rdr, &char_prop)?;

        Self::build(
            &system_word_entries,
            ConnectorWrapper::Matrix(connector),
            char_prop,
            unk_handler,
        )
    }

    /// システムエントリからメモリ効率の良い新しい [`DictionaryInner`] を作成します。
    ///
    /// この関数は接続コスト行列をコンパクト形式で実装します。
    /// [`from_readers()`](Self::from_readers) で生成された辞書と比較して、
    /// この関数で生成された辞書はメモリ使用量を節約できますが、
    /// 解析速度は遅くなる可能性があります。
    ///
    /// # 引数
    ///
    ///  - `system_lexicon_rdr`: 辞書ファイル `*.csv` のリーダー
    ///  - `bigram_right_rdr`: 右IDに関連付けられたバイグラム情報ファイル `bigram.right` のリーダー
    ///  - `bigram_left_rdr`: 左IDに関連付けられたバイグラム情報ファイル `bigram.left` のリーダー
    ///  - `bigram_cost_rdr`: バイグラムコストファイル `bigram.cost` のリーダー
    ///  - `char_prop_rdr`: 文字定義ファイル `char.def` のリーダー
    ///  - `unk_handler_rdr`: 未知語定義ファイル `unk.def` のリーダー
    ///  - `dual_connector`: `true` の場合、辞書は速度低下を制御します
    ///
    /// # エラー
    ///
    /// 入力フォーマットが不正な場合に [`VibratoError`] を返します。
    pub fn from_readers_with_bigram_info<S, R, L, C, P, U>(
        mut system_lexicon_rdr: S,
        bigram_right_rdr: R,
        bigram_left_rdr: L,
        bigram_cost_rdr: C,
        char_prop_rdr: P,
        unk_handler_rdr: U,
        dual_connector: bool,
    ) -> Result<DictionaryInner>
    where
        S: Read,
        R: Read,
        L: Read,
        C: Read,
        P: Read,
        U: Read,
    {
        let mut system_lexicon_buf = vec![];
        system_lexicon_rdr.read_to_end(&mut system_lexicon_buf)?;
        let system_word_entries = Lexicon::parse_csv(&system_lexicon_buf, "lex.csv")?;
        let connector = if dual_connector {
            ConnectorWrapper::Dual(DualConnector::from_readers(
                bigram_right_rdr,
                bigram_left_rdr,
                bigram_cost_rdr,
            )?)
        } else {
            ConnectorWrapper::Raw(RawConnector::from_readers(
                bigram_right_rdr,
                bigram_left_rdr,
                bigram_cost_rdr,
            )?)
        };
        let char_prop = CharProperty::from_reader(char_prop_rdr)?;
        let unk_handler = UnkHandler::from_reader(unk_handler_rdr, &char_prop)?;

        Self::build(&system_word_entries, connector, char_prop, unk_handler)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oor_lex() {
        let lexicon_csv = "自然,1,1,0";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 1 0";
        let unk_def = "DEFAULT,0,0,100,*";

        let result = SystemDictionaryBuilder::from_readers(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_oor_unk() {
        let lexicon_csv = "自然,0,0,0";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 1 0";
        let unk_def = "DEFAULT,1,1,100,*";

        let result = SystemDictionaryBuilder::from_readers(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        assert!(result.is_err());
    }
}