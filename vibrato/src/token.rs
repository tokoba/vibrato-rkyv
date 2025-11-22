//! トークンの結果コンテナ
//!
//! このモジュールは、形態素解析の結果として得られるトークンを表現する型を提供します。
//! トークンは辞書内の単語への参照を保持し、表層形、品詞情報、位置情報などへの
//! アクセスを提供します。

use std::ops::Range;

use crate::dictionary::DictionaryInnerRef;
use crate::dictionary::{word_idx::WordIdx, LexType};
use crate::tokenizer::lattice::Node;
use crate::tokenizer::worker::Worker;

/// 形態素解析の結果トークン
///
/// このトークンは[`Worker`]への軽量な参照であり、実際のデータは
/// Workerが保持しています。トークンはWorkerが生存している間のみ有効です。
///
/// トークンからは以下の情報にアクセスできます：
/// - 表層形（元のテキスト中の文字列）
/// - 品詞などの素性情報
/// - 文字位置およびバイト位置
/// - 単語コストおよび累積コスト
pub struct Token<'w> {
    worker: &'w Worker,
    index: usize,
}

impl<'w> Token<'w> {
    #[inline(always)]
    pub(crate) const fn new(worker: &'w Worker, index: usize) -> Self {
        Self { worker, index }
    }

    /// トークンの文字単位の位置範囲を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの開始位置から終了位置までの文字単位の範囲を返します。
    ///
    /// Gets the position range of the token in characters.
    #[inline(always)]
    pub fn range_char(&self) -> Range<usize> {
        let (end_word, node) = &self.worker.top_nodes[self.index];
        node.start_word..*end_word
    }

    /// トークンのバイト単位の位置範囲を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの開始位置から終了位置までのバイト単位の範囲を返します。
    ///
    /// Gets the position range of the token in bytes.
    #[inline(always)]
    pub fn range_byte(&self) -> Range<usize> {
        let sent = &self.worker.sent;
        let (end_word, node) = &self.worker.top_nodes[self.index];
        sent.byte_position(node.start_word)..sent.byte_position(*end_word)
    }

    /// トークンの表層形（元のテキスト中の文字列）を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの表層形の文字列参照を返します。
    ///
    /// Gets the surface string of the token.
    #[inline(always)]
    pub fn surface(&self) -> &'w str {
        let sent = &self.worker.sent;
        &sent.raw()[self.range_byte()]
    }

    /// トークンの単語インデックスを取得します。
    ///
    /// # 戻り値
    ///
    /// 辞書内の単語を一意に識別する[`WordIdx`]を返します。
    ///
    /// Gets the word index of the token.
    #[inline(always)]
    pub fn word_idx(&self) -> WordIdx {
        let (_, node) = &self.worker.top_nodes[self.index];
        node.word_idx()
    }

    /// トークンの素性（品詞などの情報）を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの素性情報を表す文字列参照を返します。
    /// 素性の形式は辞書によって異なります。
    ///
    /// Gets the feature string of the token.
    #[inline(always)]
    pub fn feature(&self) -> &str {
        match self.worker.tokenizer.dictionary() {
            DictionaryInnerRef::Archived(dict) => dict
                .word_feature(self.word_idx()),
            DictionaryInnerRef::Owned(dict) => dict
                .word_feature(self.word_idx()),
        }
    }

    /// トークンが由来する辞書のタイプを取得します。
    ///
    /// # 戻り値
    ///
    /// システム辞書、ユーザー辞書、未知語のいずれかを示す[`LexType`]を返します。
    ///
    /// Gets the lexicon type where the token is from.
    #[inline(always)]
    pub fn lex_type(&self) -> LexType {
        self.word_idx().lex_type
    }

    /// トークンノードの左文脈IDを取得します。
    ///
    /// # 戻り値
    ///
    /// 接続コスト計算に使用される左文脈IDを返します。
    ///
    /// Gets the left id of the token's node.
    #[inline(always)]
    pub fn left_id(&self) -> u16 {
        let (_, node) = &self.worker.top_nodes[self.index];
        node.left_id
    }

    /// トークンノードの右文脈IDを取得します。
    ///
    /// # 戻り値
    ///
    /// 接続コスト計算に使用される右文脈IDを返します。
    ///
    /// Gets the right id of the token's node.
    #[inline(always)]
    pub fn right_id(&self) -> u16 {
        let (_, node) = &self.worker.top_nodes[self.index];
        node.right_id
    }

    /// トークンノードの単語コストを取得します。
    ///
    /// # 戻り値
    ///
    /// 単語の生起コストを返します。値が低いほど出現しやすい単語です。
    ///
    /// Gets the word cost of the token's node.
    #[inline(always)]
    pub fn word_cost(&self) -> i16 {
        let (_, node) = &self.worker.top_nodes[self.index];
        match self.worker.tokenizer.dictionary() {
            DictionaryInnerRef::Archived(dict) => dict
                .word_param(node.word_idx()).word_cost,
            DictionaryInnerRef::Owned(dict) => dict
                .word_param(node.word_idx()).word_cost,
        }
    }

    /// 文頭からこのトークンノードまでの累積コストを取得します。
    ///
    /// # 戻り値
    ///
    /// BOS（文頭）からこのトークンまでのパス全体の累積コストを返します。
    ///
    /// Gets the total cost from BOS to the token's node.
    #[inline(always)]
    pub fn total_cost(&self) -> i32 {
        let (_, node) = &self.worker.top_nodes[self.index];
        node.min_cost
    }

    /// このトークンビューを所有型の[`TokenBuf`]に変換します。
    ///
    /// # 戻り値
    ///
    /// このトークンのすべての情報を含む所有型の[`TokenBuf`]を返します。
    /// スレッド間でトークン情報を送信したり、長期保存する際に有用です。
    pub fn to_buf(&self) -> TokenBuf {
        TokenBuf {
            surface: self.surface().to_string(),
            feature: self.feature().to_string(),
            range_char: self.range_char(),
            range_byte: self.range_byte(),
            word_id: self.word_idx(),
            lex_type: self.lex_type(),
            left_id: self.left_id(),
            right_id: self.right_id(),
            word_cost: self.word_cost(),
            total_cost: self.total_cost(),
        }
    }
}

impl std::fmt::Debug for Token<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Token")
            .field("surface", &self.surface())
            .field("range_char", &self.range_char())
            .field("range_byte", &self.range_byte())
            .field("feature", &self.feature())
            .field("lex_type", &self.lex_type())
            .field("word_id", &self.word_idx())
            .field("left_id", &self.left_id())
            .field("right_id", &self.right_id())
            .field("word_cost", &self.word_cost())
            .field("total_cost", &self.total_cost())
            .finish()
    }
}

/// N-best解析パス内のトークンへの軽量ビュー
///
/// [`Token`]と同様に、このトークンは[`Worker`]を借用する軽量なビューです。
/// [`NbestTokenIter`]によって生成されます。N-best解析では複数の候補パスが
/// 生成されますが、このトークンはその中の一つのパス内のトークンを表現します。
///
/// A lightweight view of a token within an N-best path.
///
/// Similar to `Token`, this struct is a lightweight view that borrows the `Worker`.
/// It is created by `NbestTokenIter`.
pub struct NbestToken<'w> {
    worker: &'w Worker,
    path_idx: usize,
    token_idx: usize,
}

impl<'w> NbestToken<'w> {
    /// Gets a raw pointer to the underlying `Node` for this token.
    #[inline(always)]
    fn node_ptr(&self) -> *const Node {
        // This relies on bounds checks performed in NbestTokenIter::new
        // and NbestTokenIter::next, so it should be safe within the iterator context.
        self.worker.nbest_paths[self.path_idx].0[self.token_idx]
    }

    /// Gets a safe reference to the underlying `Node`.
    #[inline(always)]
    fn node(&self) -> &'w Node {
        unsafe { &*self.node_ptr() }
    }

    /// Gets the end position (in characters) of this token.
    #[inline(always)]
    fn end_word(&self) -> usize {
        let path = &self.worker.nbest_paths[self.path_idx].0;
        if self.token_idx + 1 < path.len() {
            // If there is a next token, its start position is our end position.
            unsafe { (*path[self.token_idx + 1]).start_word }
        } else {
            // If this is the last token in the path, the sentence end is our end.
            self.worker.sent.len_char()
        }
    }

    /// トークンの表層形（元のテキスト中の文字列）を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの表層形の文字列参照を返します。
    ///
    /// Gets the surface string of the token.
    #[inline(always)]
    pub fn surface(&self) -> &'w str {
        &self.worker.sent.raw()[self.range_byte()]
    }

    /// トークンの素性（品詞などの情報）を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの素性情報を表す文字列参照を返します。
    /// 素性の形式は辞書によって異なります。
    ///
    /// Gets the feature string of the token.
    #[inline(always)]
    pub fn feature(&self) -> &'w str {
        match self.worker.tokenizer.dictionary() {
            DictionaryInnerRef::Archived(dict) => dict
                .word_feature(self.word_idx()),
            DictionaryInnerRef::Owned(dict) => dict
                .word_feature(self.word_idx()),
        }
    }

    /// トークンの文字単位の位置範囲を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの開始位置から終了位置までの文字単位の範囲を返します。
    ///
    /// Gets the position range of the token in characters.
    #[inline(always)]
    pub fn range_char(&self) -> Range<usize> {
        self.node().start_word..self.end_word()
    }

    /// トークンのバイト単位の位置範囲を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの開始位置から終了位置までのバイト単位の範囲を返します。
    ///
    /// Gets the position range of the token in bytes.
    #[inline(always)]
    pub fn range_byte(&self) -> Range<usize> {
        let sent = &self.worker.sent;
        sent.byte_position(self.node().start_word)..sent.byte_position(self.end_word())
    }

    /// トークンの単語インデックスを取得します。
    ///
    /// # 戻り値
    ///
    /// 辞書内の単語を一意に識別する[`WordIdx`]を返します。
    ///
    /// Gets the word index of the token.
    #[inline(always)]
    pub fn word_idx(&self) -> WordIdx {
        self.node().word_idx()
    }

    /// トークンが由来する辞書のタイプを取得します。
    ///
    /// # 戻り値
    ///
    /// システム辞書、ユーザー辞書、未知語のいずれかを示す[`LexType`]を返します。
    ///
    /// Gets the lexicon type where the token is from.
    #[inline(always)]
    pub fn lex_type(&self) -> LexType {
        self.word_idx().lex_type
    }

    /// トークンノードの左文脈IDを取得します。
    ///
    /// # 戻り値
    ///
    /// 接続コスト計算に使用される左文脈IDを返します。
    ///
    /// Gets the left connection ID of the token's node.
    #[inline(always)]
    pub fn left_id(&self) -> u16 {
        self.node().left_id
    }

    /// トークンノードの右文脈IDを取得します。
    ///
    /// # 戻り値
    ///
    /// 接続コスト計算に使用される右文脈IDを返します。
    ///
    /// Gets the right connection ID of the token's node.
    #[inline(always)]
    pub fn right_id(&self) -> u16 {
        self.node().right_id
    }

    /// トークンノードの単語コストを取得します。
    ///
    /// # 戻り値
    ///
    /// 単語の生起コストを返します。値が低いほど出現しやすい単語です。
    ///
    /// Gets the word cost of the token's node.
    #[inline(always)]
    pub fn word_cost(&self) -> i16 {
        let dict = self.worker.tokenizer.dictionary();
        dict.word_param(self.word_idx()).word_cost
    }

    /// 文頭からこのトークンノードまでの累積コストを取得します。
    ///
    /// # 戻り値
    ///
    /// BOS（文頭）からこのトークンまでのパス全体の累積コストを返します。
    /// この値は前向きビタビパスで計算されます。
    ///
    /// Gets the total cost from the beginning of the sentence (BOS)
    /// to this token's node, calculated during the forward Viterbi pass.
    #[inline(always)]
    pub fn total_cost(&self) -> i32 {
        self.node().min_cost
    }

    /// このトークンビューを所有型の[`TokenBuf`]に変換します。
    ///
    /// # 戻り値
    ///
    /// このトークンのすべての情報を含む所有型の[`TokenBuf`]を返します。
    /// スレッド間でトークン情報を送信したり、長期保存する際に有用です。
    ///
    /// Converts this token view into an owned `TokenBuf`.
    pub fn to_buf(&self) -> TokenBuf {
        TokenBuf {
            surface: self.surface().to_string(),
            feature: self.feature().to_string(),
            word_id: self.word_idx(),
            lex_type: self.lex_type(),
            range_char: self.range_char(),
            range_byte: self.range_byte(),
            left_id: self.left_id(),
            right_id: self.right_id(),
            word_cost: self.word_cost(),
            total_cost: self.total_cost(),
        }
    }
}

impl std::fmt::Debug for NbestToken<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NbestToken")
            .field("surface", &self.surface())
            .field("range_char", &self.range_char())
            .field("range_byte", &self.range_byte())
            .field("feature", &self.feature())
            .field("lex_type", &self.lex_type())
            .field("word_id", &self.word_idx())
            .field("left_id", &self.left_id())
            .field("right_id", &self.right_id())
            .field("word_cost", &self.word_cost())
            .field("total_cost", &self.total_cost())
            .finish()
    }
}

/// トークンのイテレータ
///
/// 形態素解析の結果得られたトークン列を順次取得するためのイテレータです。
/// 前方および後方からの走査をサポートしています（[`DoubleEndedIterator`]を実装）。
///
/// Iterator of tokens.
pub struct TokenIter<'w> {
    worker: &'w Worker,
    front: usize,
    back: usize,
}

impl<'w> TokenIter<'w> {
    #[inline(always)]
    pub(crate) fn new(worker: &'w Worker) -> Self {
        let num_tokens = worker.num_tokens();
        Self {
            worker,
            front: 0,
            back: num_tokens,
        }
    }
}

impl<'w> Iterator for TokenIter<'w> {
    type Item = Token<'w>;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.front < self.back {
            let t = self.worker.token(self.front);
            self.front += 1;
            Some(t)
        } else {
            None
        }
    }
}

impl<'w> DoubleEndedIterator for TokenIter<'w> {
    #[inline(always)]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front < self.back {
            self.back -= 1;
            let t = self.worker.token(self.back);
            Some(t)
        } else {
            None
        }
    }
}

/// 特定のN-best解析パス内のトークンをイテレートするイテレータ
///
/// N-best解析で得られた複数の候補パスのうち、特定のパス（`path_idx`で指定）に
/// 含まれるトークンを順次取得するためのイテレータです。
///
/// An iterator over tokens in a specific N-best path.
pub struct NbestTokenIter<'w> {
    worker: &'w Worker,
    path_idx: usize,
    current_token_idx: usize,
}

impl<'w> NbestTokenIter<'w> {
    pub(crate) fn new(worker: &'w Worker, path_idx: usize) -> Self {
        Self { worker, path_idx, current_token_idx: 0 }
    }
}

impl<'w> Iterator for NbestTokenIter<'w> {
    type Item = NbestToken<'w>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_token_idx < self.worker.nbest_paths[self.path_idx].0.len() {
            let token = NbestToken {
                worker: self.worker,
                path_idx: self.path_idx,
                token_idx: self.current_token_idx,
            };
            self.current_token_idx += 1;
            Some(token)
        } else {
            None
        }
    }
}

/// 所有型の自己完結したトークン
///
/// このトークンは[`Token`]の所有型版です。形態素解析の結果を長期保存したり、
/// スレッド間で送信する際に有用です。すべてのトークン情報を自身で保持するため、
/// [`Worker`]への参照が不要です。
///
/// An owned, self-contained token.
///
/// This struct is the owned counterpart to [`Token`].
/// It is useful for storing tokenization results or
/// sending them across threads.
#[derive(Debug, Clone)]
pub struct TokenBuf {
    /// トークンの表層形（元のテキスト中の文字列）
    ///
    /// The surface string of the token.
    pub surface: String,

    /// トークンの素性情報（品詞など）
    ///
    /// The feature string of the token.
    pub feature: String,

    /// トークンの文字単位の位置範囲
    ///
    /// The position range of the token in characters.
    pub range_char: Range<usize>,

    /// トークンのバイト単位の位置範囲
    ///
    /// The position range of the token in bytes.
    pub range_byte: Range<usize>,

    /// トークンが由来する辞書のタイプ
    ///
    /// The lexicon type where the token is from.
    pub lex_type: LexType,

    /// トークンの単語インデックス
    ///
    /// The word index of the token.
    pub word_id: WordIdx,

    /// トークンノードの左文脈ID
    ///
    /// The left connection ID of the token's node.
    pub left_id: u16,

    /// トークンノードの右文脈ID
    ///
    /// The right connection ID of the token's node.
    pub right_id: u16,

    /// トークンノードの単語コスト
    ///
    /// The word cost of the token's node.
    pub word_cost: i16,

    /// 文頭からこのトークンノードまでの累積コスト
    ///
    /// The total cost from BOS to the token's node.
    pub total_cost: i32,
}

impl<'w> From<Token<'w>> for TokenBuf {
    fn from(token: Token<'w>) -> Self {
        token.to_buf()
    }
}

#[cfg(test)]
mod tests {
    use crate::dictionary::*;
    use crate::tokenizer::*;

    #[test]
    fn test_iter() {
        let lexicon_csv = "自然,0,0,1,sizen
言語,0,0,4,gengo
処理,0,0,3,shori
自然言語,0,0,6,sizengengo
言語処理,0,0,5,gengoshori";
        let matrix_def = "1 1\n0 0 0";
        let char_def = "DEFAULT 0 1 0";
        let unk_def = "DEFAULT,0,0,100,*";

        let dict_inner =
            SystemDictionaryBuilder::from_readers(
                lexicon_csv.as_bytes(),
                matrix_def.as_bytes(),
                char_def.as_bytes(),
                unk_def.as_bytes(),
            ).unwrap();

        let mut buffer = Vec::new();
        dict_inner.write(&mut buffer).unwrap();

        let dict = Dictionary::read(buffer.as_slice()).unwrap();

        let tokenizer = Tokenizer::new(dict);
        let mut worker = tokenizer.new_worker();
        worker.reset_sentence("自然言語処理");
        worker.tokenize();
        assert_eq!(worker.num_tokens(), 2);

        let mut it = worker.token_iter();
        for i in 0..worker.num_tokens() {
            let lhs = worker.token(i);
            let rhs = it.next().unwrap();
            assert_eq!(lhs.surface(), rhs.surface());
        }
        assert!(it.next().is_none());
    }
}
