//! トークン化処理のためのルーチンを提供するモジュール。
//!
//! このモジュールは、形態素解析のための主要なワーカー構造体を提供します。
//! ワーカーは内部データ構造を保持し、再利用することで不要なメモリアロケーションを避けます。
use crate::dictionary::{ConnectorKindRef, DictionaryInnerRef};
use crate::dictionary::connector::ConnectorView;
use crate::dictionary::mapper::{ConnIdCounter, ConnIdProbs};
use crate::sentence::Sentence;
use crate::token::{NbestTokenIter, Token, TokenIter};
use crate::tokenizer::lattice::{Lattice, LatticeKind, Node};
use crate::tokenizer::Tokenizer;
use crate::tokenizer::nbest_generator::NbestGenerator;

/// トークン化処理のためのルーチンを提供する構造体。
///
/// トークン化に使用される内部データ構造を保持し、それらを再利用することで
/// 不要なメモリ再割り当てを回避します。
///
/// # 例
///
/// ```ignore
/// let mut worker = Worker::new(tokenizer);
/// worker.reset_sentence("日本語の文章");
/// worker.tokenize();
/// for token in worker.token_iter() {
///     println!("{}", token.surface());
/// }
/// ```
pub struct Worker {
    pub(crate) tokenizer: Tokenizer,
    pub(crate) sent: Sentence,
    pub(crate) lattice: LatticeKind,
    pub(crate) top_nodes: Vec<(usize, Node)>,
    pub(crate) counter: Option<ConnIdCounter>,
    pub(crate) nbest_paths: Vec<(Vec<*const Node>, i32)>,
}

impl Worker {
    /// 新しいインスタンスを作成します。
    ///
    /// # 引数
    ///
    /// * `tokenizer` - 使用するトークナイザー
    pub(crate) fn new(tokenizer: Tokenizer) -> Self {
        Self {
            tokenizer,
            sent: Sentence::new(),
            lattice: LatticeKind::For1Best(Lattice::default()),
            top_nodes: vec![],
            counter: None,
            nbest_paths: Vec::with_capacity(0),
        }
    }

    /// トークン化する入力文をリセットします。
    ///
    /// 新しい文を設定し、以前の状態をクリアします。
    ///
    /// # 引数
    ///
    /// * `input` - トークン化する入力文字列
    pub fn reset_sentence<S>(&mut self, input: S)
    where
        S: AsRef<str>,
    {
        self.sent.clear();
        self.top_nodes.clear();
        let input = input.as_ref();
        if !input.is_empty() {
            self.sent.set_sentence(input);
            match self.tokenizer.dictionary() {
                DictionaryInnerRef::Archived(dict) => {
                    self.sent.compile_archived(dict.char_prop());
                },
                DictionaryInnerRef::Owned(dict) => {
                    self.sent.compile(dict.char_prop());
                },
            }
        }
    }

    /// 設定された入力文をトークン化します。
    ///
    /// トークン化結果は内部状態に保存され、`token_iter()`や`token()`メソッドで
    /// アクセスできます。空の文が設定されている場合は何も行いません。
    pub fn tokenize(&mut self) {
        if self.sent.chars().is_empty() {
            return;
        }
        let lattice_1best = self.lattice.prepare_for_1best(self.sent.len_char());

        self.tokenizer.build_lattice(&self.sent, lattice_1best);
        lattice_1best.append_top_nodes(&mut self.top_nodes);
    }

    /// 文をトークン化し、上位N個の最良結果を内部に保存します。
    ///
    /// この関数を呼び出した後、結果は`num_nbest_paths()`, `path_cost(path_idx)`,
    /// `nbest_token_iter(path_idx)`を通じてアクセスできます。
    ///
    /// # 引数
    ///
    /// * `n` - 取得する候補パスの最大数
    pub fn tokenize_nbest(&mut self, n: usize) {
        self.nbest_paths.clear();
        if self.sent.chars().is_empty() {
            return;
        }
        let lattice_nbest = self.lattice.prepare_for_nbest(self.sent.len_char());

        self.tokenizer.build_lattice_nbest(&self.sent, lattice_nbest);

        let dict_ref = self.tokenizer.dictionary();
        let connector_ref = dict_ref.connector();

        let generator = match connector_ref {
            ConnectorKindRef::Archived(connector) => NbestGenerator::new(lattice_nbest, connector, dict_ref),
            ConnectorKindRef::Owned(connector) => NbestGenerator::new(lattice_nbest, connector, dict_ref),
        };
        self.nbest_paths = generator.take(n).collect();
    }

    /// トークン化結果のトークン数を取得します。
    ///
    /// # 戻り値
    ///
    /// トークンの総数
    #[inline(always)]
    pub fn num_tokens(&self) -> usize {
        self.top_nodes.len()
    }

    /// `i`番目のトークンを取得します。
    ///
    /// # 引数
    ///
    /// * `i` - トークンのインデックス（0から始まる）
    ///
    /// # 戻り値
    ///
    /// 指定されたインデックスのトークン
    #[inline(always)]
    pub fn token<'w>(&'w self, i: usize) -> Token<'w> {
        let index = self.num_tokens() - i - 1;
        Token::new(self, index)
    }

    /// トークン化結果のイテレータを作成します。
    ///
    /// # 戻り値
    ///
    /// トークンのイテレータ
    #[inline(always)]
    pub fn token_iter<'w>(&'w self) -> TokenIter<'w> {
        TokenIter::new(self)
    }

    /// `path_idx`で指定されたN-bestパスのトークンイテレータを返します。
    ///
    /// # 引数
    ///
    /// * `path_idx` - パスのインデックス（0から始まる）
    ///
    /// # 戻り値
    ///
    /// パスが存在する場合は`Some(イテレータ)`、存在しない場合は`None`
    pub fn nbest_token_iter(&self, path_idx: usize) -> Option<NbestTokenIter<'_>> {
        if path_idx < self.nbest_paths.len() {
            Some(NbestTokenIter::new(self, path_idx))
        } else {
            None
        }
    }

    /// 接続IDの出現確率を計算するためのカウンタを初期化します。
    ///
    /// この関数は、接続IDの統計情報を収集する前に呼び出す必要があります。
    pub fn init_connid_counter(&mut self) {
        let (num_left, num_right) = match self.tokenizer.dictionary() {
            DictionaryInnerRef::Archived(dict) =>
                (dict.connector().num_left(), dict.connector().num_right()),
            DictionaryInnerRef::Owned(dict) =>
                (dict.connector().num_left(), dict.connector().num_right()),
        };
        self.counter = Some(ConnIdCounter::new(
            num_left,
            num_right,
        ));
    }

    /// 最後のトークン化における接続IDの頻度を更新します。
    ///
    /// # パニック
    ///
    /// [`Self::init_connid_counter()`]が一度も呼び出されていない場合、パニックします。
    pub fn update_connid_counts(&mut self) {
        match &self.lattice {
            LatticeKind::For1Best(lattice) => lattice.add_connid_counts(self.counter.as_mut().unwrap()),
            LatticeKind::ForNBest(lattice_nbest) => lattice_nbest.add_connid_counts(self.counter.as_mut().unwrap()),
        }
    }

    /// 接続IDの出現確率を計算し、左IDと右IDの確率を返します。
    ///
    /// # 戻り値
    ///
    /// 左IDと右IDの確率のタプル
    ///
    /// # パニック
    ///
    /// [`Self::init_connid_counter()`]が一度も呼び出されていない場合、パニックします。
    pub fn compute_connid_probs(&self) -> (ConnIdProbs, ConnIdProbs) {
        self.counter.as_ref().unwrap().compute_probs()
    }

    /// 見つかったN-bestパスの数を返します。
    ///
    /// # 戻り値
    ///
    /// パスの総数
    pub fn num_nbest_paths(&self) -> usize {
        self.nbest_paths.len()
    }

    /// `path_idx`で指定されたパスの総コストを返します。
    ///
    /// # 引数
    ///
    /// * `path_idx` - パスのインデックス
    ///
    /// # 戻り値
    ///
    /// パスが存在する場合は`Some(コスト)`、存在しない場合は`None`
    pub fn path_cost(&self, path_idx: usize) -> Option<i32> {
        self.nbest_paths.get(path_idx).map(|(_, cost)| *cost)
    }
}
