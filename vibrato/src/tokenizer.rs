//! Viterbiアルゴリズムに基づくトークナイザー。
//!
//! このモジュールは、日本語形態素解析のためのメイントークナイザーを提供します。
//! Viterbiアルゴリズムを使用して、入力文を最適な形態素列に分割します。
//!
//! # 主要な構造体
//!
//! - [`Tokenizer`]: 形態素解析を実行するメイントークナイザー構造体
//! - [`Worker`]: トークナイザーのワーカー。実際の解析処理を行う
//!
//! # 例
//!
//! ```no_run
//! use vibrato_rkyv::{Tokenizer, Dictionary, LoadMode};
//!
//! let dict = Dictionary::from_path("path/to/dict", LoadMode::Validate)?;
//! let tokenizer = Tokenizer::new(dict);
//! let mut worker = tokenizer.new_worker();
//!
//! worker.reset_sentence("自然言語処理");
//! worker.tokenize();
//!
//! for i in 0..worker.num_tokens() {
//!     let token = worker.token(i);
//!     println!("{}", token.surface());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
pub(crate) mod lattice;
mod nbest_generator;
pub mod worker;

use std::sync::Arc;

use crate::Dictionary;
use crate::dictionary::connector::{ArchivedConnectorWrapper, ConnectorCost, ConnectorWrapper};
use crate::dictionary::{ArchivedDictionaryInner, DictionaryInner, DictionaryInnerRef};
use crate::errors::{Result, VibratoError};
use crate::sentence::Sentence;
use crate::tokenizer::lattice::{Lattice, LatticeNBest};
use crate::tokenizer::worker::Worker;

/// 形態素解析を行うトークナイザー。
///
/// `Tokenizer`は、Viterbiアルゴリズムを使用して日本語テキストを形態素に分割します。
/// 辞書データを保持し、複数の[`Worker`]インスタンスを生成して並列処理を行うことができます。
///
/// # フィールド
///
/// - `dict`: 形態素解析に使用する辞書データへの参照
/// - `space_cateset`: MeCab互換モードでのスペース文字のカテゴリセット
/// - `max_grouping_len`: 未知語の最大グルーピング長
///
/// # 例
///
/// ```no_run
/// use vibrato_rkyv::{Dictionary, Tokenizer, LoadMode};
///
/// let dict = Dictionary::from_path("path/to/dict", LoadMode::Validate)?;
/// let tokenizer = Tokenizer::new(dict);
/// let mut worker = tokenizer.new_worker();
///
/// worker.reset_sentence("形態素解析");
/// worker.tokenize();
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone)]
pub struct Tokenizer {
    dict: Arc<Dictionary>,
    // For the MeCab compatibility
    space_cateset: Option<u32>,
    max_grouping_len: Option<usize>,
}

impl Tokenizer {
    /// 新しいトークナイザーを作成します。
    ///
    /// 辞書はトークナイザーに所有権が移動します。複数のトークナイザー間で辞書を共有する
    /// 必要がある場合は、[`Tokenizer::from_shared_dictionary`]を使用してください。
    ///
    /// **注意:** `legacy`機能を有効にして`Dictionary::from_zstd`を使用している場合、
    /// この関数のムーブセマンティクスにより、トークナイザーがドロップされる際に、
    /// バックグラウンドのキャッシングスレッドが完了するまで現在のスレッドが
    /// ブロックされる可能性があります。
    ///
    /// # 引数
    ///
    /// * `dict` - 形態素解析に使用する辞書
    ///
    /// # 戻り値
    ///
    /// 新しい`Tokenizer`インスタンス
    ///
    /// # 例
    ///
    /// ```no_run
    /// use vibrato_rkyv::{Dictionary, Tokenizer, LoadMode};
    ///
    /// let dict = Dictionary::from_path("path/to/dict", LoadMode::Validate)?;
    /// let tokenizer = Tokenizer::new(dict);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new(dict: Dictionary) -> Self {
        Self {
            dict: Arc::new(dict),
            space_cateset: None,
            max_grouping_len: None,
        }
    }

    /// `DictionaryInner`から新しいトークナイザーを作成します。
    ///
    /// # 引数
    ///
    /// * `dict` - 内部辞書データ
    ///
    /// # 戻り値
    ///
    /// 新しい`Tokenizer`インスタンス
    pub fn from_inner(dict: DictionaryInner) -> Self {
        Self {
            dict: Arc::new(Dictionary::Owned { dict: Arc::new(dict), _caching_handle: None }),
            space_cateset: None,
            max_grouping_len: None,
        }
    }

    /// 共有された辞書から新しいトークナイザーを作成します。
    ///
    /// これは、複数のトークナイザーインスタンスが辞書データを再読み込みすることなく
    /// 同じ辞書データを共有する必要があるマルチスレッドシナリオで便利です。
    ///
    /// # 引数
    ///
    /// * `dict` - 共有される辞書への`Arc`参照
    ///
    /// # 戻り値
    ///
    /// 新しい`Tokenizer`インスタンス
    ///
    /// # 例
    ///
    /// ```no_run
    /// use std::sync::Arc;
    /// use vibrato_rkyv::{Dictionary, Tokenizer, LoadMode};
    ///
    /// let dict = Arc::new(Dictionary::from_path("path/to/dict", LoadMode::Validate)?);
    /// let tokenizer1 = Tokenizer::from_shared_dictionary(dict.clone());
    /// let tokenizer2 = Tokenizer::from_shared_dictionary(dict.clone());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_shared_dictionary(dict: Arc<Dictionary>) -> Self {
        Self {
            dict,
            space_cateset: None,
            max_grouping_len: None,
        }
    }

    /// トークンからスペースを無視するかどうかを設定します。
    ///
    /// このオプションはMeCabとの互換性のためのものです。
    /// MeCabと同じ結果を得たい場合は、これを有効にしてください。
    ///
    /// # 引数
    ///
    /// * `yes` - `true`の場合、スペース文字をトークンから除外します
    ///
    /// # 戻り値
    ///
    /// 設定が適用された`Tokenizer`インスタンス
    ///
    /// # エラー
    ///
    /// 入力辞書に`SPACE`カテゴリが定義されていない場合、[`VibratoError`]が返されます。
    ///
    /// # 例
    ///
    /// ```no_run
    /// use vibrato_rkyv::{Dictionary, Tokenizer, LoadMode};
    ///
    /// let dict = Dictionary::from_path("path/to/dict", LoadMode::Validate)?;
    /// let tokenizer = Tokenizer::new(dict).ignore_space(true)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn ignore_space(mut self, yes: bool) -> Result<Self> {
        if yes {
            let cate_id = match &*self.dict {
                Dictionary::Archived(archived_dict) => archived_dict.char_prop().cate_id("SPACE"),
                Dictionary::Owned { dict, ..} => dict.char_prop().cate_id("SPACE"),
            }.ok_or_else(|| {
                VibratoError::invalid_argument(
                    "dict",
                    "SPACE is not defined in the input dictionary (i.e., char.def).",
                )
            })?;

            self.space_cateset = Some(1 << cate_id);
        } else {
            self.space_cateset = None;
        }
        Ok(self)
    }


    /// 未知語の最大グルーピング長を指定します。
    ///
    /// デフォルトでは、長さは無限です。
    ///
    /// このオプションはMeCabとの互換性のためのものです。
    /// MeCabと同じ結果を得たい場合は、引数に`24`を指定してください。
    ///
    /// # 引数
    ///
    /// * `max_grouping_len` - 未知語の最大グルーピング長。
    ///   デフォルト値は0で、無限の長さを示します。
    ///
    /// # 戻り値
    ///
    /// 設定が適用された`Tokenizer`インスタンス
    ///
    /// # 例
    ///
    /// ```no_run
    /// use vibrato_rkyv::{Dictionary, Tokenizer, LoadMode};
    ///
    /// let dict = Dictionary::from_path("path/to/dict", LoadMode::Validate)?;
    /// // MeCab互換モード
    /// let tokenizer = Tokenizer::new(dict).max_grouping_len(24);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub const fn max_grouping_len(mut self, max_grouping_len: usize) -> Self {
        if max_grouping_len != 0 {
            self.max_grouping_len = Some(max_grouping_len);
        } else {
            self.max_grouping_len = None;
        }
        self
    }

    /// 辞書への参照を取得します。
    ///
    /// # 戻り値
    ///
    /// 辞書内部データへの参照
    pub(crate) fn dictionary<'a>(&'a self) -> DictionaryInnerRef<'a> {
        match &*self.dict {
            Dictionary::Archived(archived_dict) => DictionaryInnerRef::Archived(archived_dict),
            Dictionary::Owned { dict, .. } => DictionaryInnerRef::Owned(dict),
        }
    }

    /// 新しいワーカーを作成します。
    ///
    /// ワーカーは実際の形態素解析処理を実行するために使用されます。
    /// 各ワーカーは独立したラティス構造を保持するため、複数のワーカーを
    /// 並列に使用して同時に複数の文を解析できます。
    ///
    /// # 戻り値
    ///
    /// 新しい[`Worker`]インスタンス
    ///
    /// # 例
    ///
    /// ```no_run
    /// use vibrato_rkyv::{Dictionary, Tokenizer, LoadMode};
    ///
    /// let dict = Dictionary::from_path("path/to/dict", LoadMode::Validate)?;
    /// let tokenizer = Tokenizer::new(dict);
    /// let mut worker = tokenizer.new_worker();
    ///
    /// worker.reset_sentence("形態素解析");
    /// worker.tokenize();
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn new_worker(&self) -> Worker {
        Worker::new(self.clone())
    }

    /// ラティス構造を構築します。
    ///
    /// 入力文に対してViterbiアルゴリズム用のラティスを構築します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - 構築するラティス構造
    pub(crate) fn build_lattice(&self, sent: &Sentence, lattice: &mut Lattice) {
        match &*self.dict {
            Dictionary::Archived(archived_dict) => match archived_dict.connector() {
                ArchivedConnectorWrapper::Matrix(c) => self.build_lattice_inner(sent, lattice, c),
                ArchivedConnectorWrapper::Raw(c) => self.build_lattice_inner(sent, lattice, c),
                ArchivedConnectorWrapper::Dual(c) => self.build_lattice_inner(sent, lattice, c),
            },
            Dictionary::Owned{ dict, .. } => match dict.connector() {
                ConnectorWrapper::Matrix(c) => self.build_lattice_inner(sent, lattice, c),
                ConnectorWrapper::Raw(c) => self.build_lattice_inner(sent, lattice, c),
                ConnectorWrapper::Dual(c) => self.build_lattice_inner(sent, lattice, c),
            },
        }
    }

    /// N-best解析用のラティス構造を構築します。
    ///
    /// 入力文に対してN-best解析用のラティスを構築します。
    /// 通常のラティスとは異なり、複数の解析結果を保持できます。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - 構築するN-best用ラティス構造
    pub(crate) fn build_lattice_nbest(&self, sent: &Sentence, lattice: &mut LatticeNBest) {
        match &*self.dict {
            Dictionary::Archived(archived_dict) => match archived_dict.connector() {
                ArchivedConnectorWrapper::Matrix(c) => self.build_lattice_inner_nbest(sent, lattice, c),
                ArchivedConnectorWrapper::Raw(c) => self.build_lattice_inner_nbest(sent, lattice, c),
                ArchivedConnectorWrapper::Dual(c) => self.build_lattice_inner_nbest(sent, lattice, c),
            },
            Dictionary::Owned{ dict, .. } => match dict.connector() {
                ConnectorWrapper::Matrix(c) => self.build_lattice_inner_nbest(sent, lattice, c),
                ConnectorWrapper::Raw(c) => self.build_lattice_inner_nbest(sent, lattice, c),
                ConnectorWrapper::Dual(c) => self.build_lattice_inner_nbest(sent, lattice, c),
            },
        }
    }

    /// ラティス構造の内部構築処理。
    ///
    /// コネクタの型に応じてラティスを構築します。
    /// MeCab互換モードの場合、スペース文字の処理も行います。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - 構築するラティス構造
    /// * `connector` - 接続コスト計算用のコネクタ
    fn build_lattice_inner<C>(&self, sent: &Sentence, lattice: &mut Lattice, connector: &C)
    where
        C: ConnectorCost,
    {
        lattice.reset(sent.len_char());

        // These variables indicate the starting character positions of words currently stored
        // in the lattice. If ignore_space() is unset, these always have the same values, and
        // start_node is practically non-functional. If ignore_space() is set, start_node and
        // start_word indicate the starting positions containing and ignoring a space character,
        // respectively. Suppose handle sentence "mens second" at position 4. start_node indicates
        // position 4, and start_word indicates position 5.
        let mut start_node = 0;
        let mut start_word = 0;

        while start_word < sent.len_char() {
            if !lattice.has_previous_node(start_node) {
                start_word += 1;
                start_node = start_word;
                continue;
            }

            // on mecab compatible mode
            if let Some(space_cateset) = self.space_cateset {
                let is_space = (sent.char_info(start_node).cate_idset() & space_cateset) != 0;
                start_word += if !is_space {
                    0
                } else {
                    // Skips space characters.
                    sent.groupable(start_node)
                };
            }

            // Does the input end with spaces?
            if start_word == sent.len_char() {
                break;
            }

            self.add_lattice_edges(sent, lattice, start_node, start_word, connector);

            start_word += 1;
            start_node = start_word;
        }

        lattice.insert_eos(start_node, connector);
    }

    /// N-best解析用ラティス構造の内部構築処理。
    ///
    /// コネクタの型に応じてN-best用ラティスを構築します。
    /// MeCab互換モードの場合、スペース文字の処理も行います。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - 構築するN-best用ラティス構造
    /// * `connector` - 接続コスト計算用のコネクタ
    fn build_lattice_inner_nbest<C>(&self, sent: &Sentence, lattice: &mut LatticeNBest, connector: &C)
    where
        C: ConnectorCost,
    {
        lattice.reset(sent.len_char());

        // These variables indicate the starting character positions of words currently stored
        // in the lattice. If ignore_space() is unset, these always have the same values, and
        // start_node is practically non-functional. If ignore_space() is set, start_node and
        // start_word indicate the starting positions containing and ignoring a space character,
        // respectively. Suppose handle sentence "mens second" at position 4. start_node indicates
        // position 4, and start_word indicates position 5.
        let mut start_node = 0;
        let mut start_word = 0;

        while start_word < sent.len_char() {
            if !lattice.has_previous_node(start_node) {
                start_word += 1;
                start_node = start_word;
                continue;
            }

            // on mecab compatible mode
            if let Some(space_cateset) = self.space_cateset {
                let is_space = (sent.char_info(start_node).cate_idset() & space_cateset) != 0;
                start_word += if !is_space {
                    0
                } else {
                    // Skips space characters.
                    sent.groupable(start_node)
                };
            }

            // Does the input end with spaces?
            if start_word == sent.len_char() {
                break;
            }

            self.add_lattice_edges_nbest(sent, lattice, start_node, start_word, connector);

            start_word += 1;
            start_node = start_word;
        }

        lattice.insert_eos(start_node, connector);
    }
}

macro_rules! add_lattice_edges_logic {
    (
        // self is required to access max_grouping_len
        $self:expr,
        $sent:expr,
        $lattice:expr,
        $start_node:expr,
        $start_word:expr,
        $connector:expr,
        $dict:expr,
    ) => {{
        let mut has_matched = false;
        let suffix = &$sent.chars()[$start_word..];

        if let Some(user_lexicon) = $dict.user_lexicon().as_ref() {
            for m in user_lexicon.common_prefix_iterator(suffix) {
                debug_assert!($start_word + m.end_char <= $sent.len_char());
                $lattice.insert_node(
                    $start_node,
                    $start_word,
                    $start_word + m.end_char,
                    m.word_idx,
                    m.word_param,
                    $connector,
                );
                has_matched = true;
            }
        }

        for m in $dict.system_lexicon().common_prefix_iterator(suffix) {
            debug_assert!($start_word + m.end_char <= $sent.len_char());
            $lattice.insert_node(
                $start_node,
                $start_word,
                $start_word + m.end_char,
                m.word_idx,
                m.word_param,
                $connector,
            );
            has_matched = true;
        }

        $dict.unk_handler().gen_unk_words(
            $sent,
            $start_word,
            has_matched,
            $self.max_grouping_len,
            |w| {
                $lattice.insert_node(
                    $start_node,
                    w.start_char(),
                    w.end_char(),
                    w.word_idx(),
                    w.word_param(),
                    $connector,
                );
            },
        );
    }};
}

impl Tokenizer {
    /// ラティスにエッジを追加します。
    ///
    /// 辞書の型（アーカイブ版または所有版）に応じて適切な内部メソッドを呼び出します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - エッジを追加するラティス
    /// * `start_node` - ノードの開始位置（スペースを含む）
    /// * `start_word` - 単語の開始位置（スペースを除く）
    /// * `connector` - 接続コスト計算用のコネクタ
    fn add_lattice_edges<C>(
        &self,
        sent: &Sentence,
        lattice: &mut Lattice,
        start_node: usize,
        start_word: usize,
        connector: &C,
    ) where
        C: ConnectorCost,
    {
        match self.dictionary() {
            DictionaryInnerRef::Archived(dict) => {
                self.add_lattice_edges_archived(sent, lattice, start_node, start_word, connector, dict)
            }
            DictionaryInnerRef::Owned(dict) => {
                self.add_lattice_edges_owned(sent, lattice, start_node, start_word, connector, dict)
            }
        }
    }

    /// N-best用ラティスにエッジを追加します。
    ///
    /// 辞書の型（アーカイブ版または所有版）に応じて適切な内部メソッドを呼び出します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - エッジを追加するN-best用ラティス
    /// * `start_node` - ノードの開始位置（スペースを含む）
    /// * `start_word` - 単語の開始位置（スペースを除く）
    /// * `connector` - 接続コスト計算用のコネクタ
    fn add_lattice_edges_nbest<C>(
        &self,
        sent: &Sentence,
        lattice: &mut LatticeNBest,
        start_node: usize,
        start_word: usize,
        connector: &C,
    ) where
        C: ConnectorCost,
    {
        match self.dictionary() {
            DictionaryInnerRef::Archived(dict) => {
                self.add_lattice_edges_archived_nbest(sent, lattice, start_node, start_word, connector, dict)
            }
            DictionaryInnerRef::Owned(dict) => {
                self.add_lattice_edges_owned_nbest(sent, lattice, start_node, start_word, connector, dict)
            }
        }
    }

    /// アーカイブ版辞書を使用してラティスにエッジを追加します。
    ///
    /// ユーザー辞書とシステム辞書から単語を検索し、
    /// 未知語ハンドラを使用して未知語も処理します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - エッジを追加するラティス
    /// * `start_node` - ノードの開始位置（スペースを含む）
    /// * `start_word` - 単語の開始位置（スペースを除く）
    /// * `connector` - 接続コスト計算用のコネクタ
    /// * `dict` - アーカイブ版辞書
    fn add_lattice_edges_archived<C>(
        &self,
        sent: &Sentence,
        lattice: &mut Lattice,
        start_node: usize,
        start_word: usize,
        connector: &C,
        dict: &ArchivedDictionaryInner,
    ) where
        C: ConnectorCost,
    {
        add_lattice_edges_logic!(
            self,
            sent,
            lattice,
            start_node,
            start_word,
            connector,
            dict,
        )
    }

    /// 所有版辞書を使用してラティスにエッジを追加します。
    ///
    /// ユーザー辞書とシステム辞書から単語を検索し、
    /// 未知語ハンドラを使用して未知語も処理します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - エッジを追加するラティス
    /// * `start_node` - ノードの開始位置（スペースを含む）
    /// * `start_word` - 単語の開始位置（スペースを除く）
    /// * `connector` - 接続コスト計算用のコネクタ
    /// * `dict` - 所有版辞書
    fn add_lattice_edges_owned<C>(
        &self,
        sent: &Sentence,
        lattice: &mut Lattice,
        start_node: usize,
        start_word: usize,
        connector: &C,
        dict: &DictionaryInner,
    ) where
        C: ConnectorCost,
    {
        add_lattice_edges_logic!(
            self,
            sent,
            lattice,
            start_node,
            start_word,
            connector,
            dict,
        )
    }

    /// アーカイブ版辞書を使用してN-best用ラティスにエッジを追加します。
    ///
    /// ユーザー辞書とシステム辞書から単語を検索し、
    /// 未知語ハンドラを使用して未知語も処理します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - エッジを追加するN-best用ラティス
    /// * `start_node` - ノードの開始位置（スペースを含む）
    /// * `start_word` - 単語の開始位置（スペースを除く）
    /// * `connector` - 接続コスト計算用のコネクタ
    /// * `dict` - アーカイブ版辞書
    fn add_lattice_edges_archived_nbest<C>(
        &self,
        sent: &Sentence,
        lattice: &mut LatticeNBest,
        start_node: usize,
        start_word: usize,
        connector: &C,
        dict: &ArchivedDictionaryInner,
    ) where
        C: ConnectorCost,
    {
        add_lattice_edges_logic!(
            self,
            sent,
            lattice,
            start_node,
            start_word,
            connector,
            dict,
        )
    }

    /// 所有版辞書を使用してN-best用ラティスにエッジを追加します。
    ///
    /// ユーザー辞書とシステム辞書から単語を検索し、
    /// 未知語ハンドラを使用して未知語も処理します。
    ///
    /// # 引数
    ///
    /// * `sent` - 入力文
    /// * `lattice` - エッジを追加するN-best用ラティス
    /// * `start_node` - ノードの開始位置（スペースを含む）
    /// * `start_word` - 単語の開始位置（スペースを除く）
    /// * `connector` - 接続コスト計算用のコネクタ
    /// * `dict` - 所有版辞書
    fn add_lattice_edges_owned_nbest<C>(
        &self,
        sent: &Sentence,
        lattice: &mut LatticeNBest,
        start_node: usize,
        start_word: usize,
        connector: &C,
        dict: &DictionaryInner,
    ) where
        C: ConnectorCost,
    {
        add_lattice_edges_logic!(
            self,
            sent,
            lattice,
            start_node,
            start_word,
            connector,
            dict,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dictionary::SystemDictionaryBuilder;

    #[track_caller]
    fn build_test_dictionary(
        lexicon_csv: &[u8],
        matrix_def: &[u8],
        char_def: &[u8],
        unk_def: &[u8],
    ) -> Dictionary {
        let dict_inner =
            SystemDictionaryBuilder::from_readers(
                lexicon_csv,
                matrix_def,
                char_def,
                unk_def
            ).unwrap();

        Dictionary::from_inner(dict_inner)
    }

    #[test]
    fn test_tokenize_1() {
        let lexicon_csv = "自然,0,0,1,sizen
言語,0,0,4,gengo
処理,0,0,3,shori
自然言語,0,0,6,sizengengo
言語処理,0,0,5,gengoshori";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 1 0";
        let unk_def = "DEFAULT,0,0,100,*";

        let dict = build_test_dictionary(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        let tokenizer = Tokenizer::new(dict);
        let mut worker = tokenizer.new_worker();
        worker.reset_sentence("自然言語処理");
        worker.tokenize();
        assert_eq!(worker.num_tokens(), 2);

        {
            let t = worker.token(0);
            assert_eq!(t.surface(), "自然");
            assert_eq!(t.range_char(), 0..2);
            assert_eq!(t.range_byte(), 0..6);
            assert_eq!(t.feature(), "sizen");
            assert_eq!(t.total_cost(), 1);
        }
        {
            let t = worker.token(1);
            assert_eq!(t.surface(), "言語処理");
            assert_eq!(t.range_char(), 2..6);
            assert_eq!(t.range_byte(), 6..18);
            assert_eq!(t.feature(), "gengoshori");
            assert_eq!(t.total_cost(), 6);
        }
    }

    #[test]
    fn test_tokenize_2() {
        let lexicon_csv = "自然,0,0,1,sizen
言語,0,0,4,gengo
処理,0,0,3,shori
自然言語,0,0,6,sizengengo
言語処理,0,0,5,gengoshori";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 1 0";
        let unk_def = "DEFAULT,0,0,100,*";

        let dict = build_test_dictionary(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        let tokenizer = Tokenizer::new(dict);
        let mut worker = tokenizer.new_worker();
        worker.reset_sentence("自然日本語処理");
        worker.tokenize();
        assert_eq!(worker.num_tokens(), 2);

        {
            let t = worker.token(0);
            assert_eq!(t.surface(), "自然");
            assert_eq!(t.range_char(), 0..2);
            assert_eq!(t.range_byte(), 0..6);
            assert_eq!(t.feature(), "sizen");
            assert_eq!(t.total_cost(), 1);
        }
        {
            let t = worker.token(1);
            assert_eq!(t.surface(), "日本語処理");
            assert_eq!(t.range_char(), 2..7);
            assert_eq!(t.range_byte(), 6..21);
            assert_eq!(t.feature(), "*");
            assert_eq!(t.total_cost(), 101);
        }
    }

    #[test]
    fn test_tokenize_3() {
        let lexicon_csv = "自然,0,0,1,sizen
言語,0,0,4,gengo
処理,0,0,3,shori
自然言語,0,0,6,sizengengo
言語処理,0,0,5,gengoshori";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 0 3";
        let unk_def = "DEFAULT,0,0,100,*";

        let dict = build_test_dictionary(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        let tokenizer = Tokenizer::new(dict);
        let mut worker = tokenizer.new_worker();
        worker.reset_sentence("不自然言語処理");
        worker.tokenize();
        assert_eq!(worker.num_tokens(), 2);

        {
            let t = worker.token(0);
            assert_eq!(t.surface(), "不自然");
            assert_eq!(t.range_char(), 0..3);
            assert_eq!(t.range_byte(), 0..9);
            assert_eq!(t.feature(), "*");
            assert_eq!(t.total_cost(), 100);
        }
        {
            let t = worker.token(1);
            assert_eq!(t.surface(), "言語処理");
            assert_eq!(t.range_char(), 3..7);
            assert_eq!(t.range_byte(), 9..21);
            assert_eq!(t.feature(), "gengoshori");
            assert_eq!(t.total_cost(), 105);
        }
    }

    #[test]
    fn test_tokenize_empty() {
        let lexicon_csv = "自然,0,0,1,sizen
言語,0,0,4,gengo
処理,0,0,3,shori
自然言語,0,0,6,sizengengo
言語処理,0,0,5,gengoshori";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 0 3";
        let unk_def = "DEFAULT,0,0,100,*";

        let dict = build_test_dictionary(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        let tokenizer = Tokenizer::new(dict);
        let mut worker = tokenizer.new_worker();
        worker.reset_sentence("");
        worker.tokenize();
        assert_eq!(worker.num_tokens(), 0);
    }

    #[test]
    fn test_tokenize_nbest() {
        let lexicon_csv = "自然,0,0,1,sizen
言語,0,0,4,gengo
処理,0,0,3,shori
自然言語,0,0,6,sizengengo
言語処理,0,0,5,gengoshori";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 1 0";
        let unk_def = "DEFAULT,0,0,100,*";

        let dict = build_test_dictionary(
            lexicon_csv.as_bytes(),
            matrix_def.as_bytes(),
            char_def.as_bytes(),
            unk_def.as_bytes(),
        );

        let tokenizer = Tokenizer::new(dict);
        let mut worker = tokenizer.new_worker();

        worker.reset_sentence("自然言語処理");
        worker.tokenize_nbest(5);

        assert_eq!(worker.num_nbest_paths(), 3, "Should find 3 possible paths");

        // 自然 | 言語処理
        // Cost = C(自然) + C(言語処理) = 1 + 5 = 6
        {
            let path_idx = 0;
            assert_eq!(worker.path_cost(path_idx), Some(6));
            let mut tokens = worker.nbest_token_iter(path_idx).unwrap();

            let token1 = tokens.next().unwrap();
            assert_eq!(token1.surface(), "自然");
            assert_eq!(token1.feature(), "sizen");

            let token2 = tokens.next().unwrap();
            assert_eq!(token2.surface(), "言語処理");
            assert_eq!(token2.feature(), "gengoshori");

            assert!(tokens.next().is_none(), "Path 1 should have only 2 tokens");
        }

        // 自然 | 言語 | 処理
        // Cost = C(自然) + C(言語) + C(処理) = 1 + 4 + 3 = 8
        {
            let path_idx = 1;
            assert_eq!(worker.path_cost(path_idx), Some(8));
            let mut tokens = worker.nbest_token_iter(path_idx).unwrap();

            let token1 = tokens.next().unwrap();
            assert_eq!(token1.surface(), "自然");

            let token2 = tokens.next().unwrap();
            assert_eq!(token2.surface(), "言語");

            let token3 = tokens.next().unwrap();
            assert_eq!(token3.surface(), "処理");

            assert!(tokens.next().is_none(), "Path 2 should have 3 tokens");
        }

        // 自然言語 | 処理
        // Cost = C(自然言語) + C(処理) = 6 + 3 = 9
        {
            let path_idx = 2;
            assert_eq!(worker.path_cost(path_idx), Some(9));
            let mut tokens = worker.nbest_token_iter(path_idx).unwrap();

            let token1 = tokens.next().unwrap();
            assert_eq!(token1.surface(), "自然言語");
            assert_eq!(token1.feature(), "sizengengo");

            let token2 = tokens.next().unwrap();
            assert_eq!(token2.surface(), "処理");
            assert_eq!(token2.feature(), "shori");

            assert!(tokens.next().is_none(), "Path 3 should have only 2 tokens");
        }

        // Empty string
        worker.reset_sentence("");
        worker.tokenize_nbest(5);
        assert_eq!(worker.num_nbest_paths(), 0, "N-best for empty string should be empty");

        // No ambiguity
        worker.reset_sentence("言語");
        worker.tokenize_nbest(5);
        assert_eq!(worker.num_nbest_paths(), 1, "Should find only 1 path for unambiguous input");
        assert_eq!(worker.path_cost(0), Some(4));
        let mut tokens = worker.nbest_token_iter(0).unwrap();
        assert_eq!(tokens.next().unwrap().surface(), "言語");
        assert!(tokens.next().is_none());
    }
}
