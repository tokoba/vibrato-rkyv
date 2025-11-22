//! ラティス（格子）構造の実装モジュール。
//!
//! このモジュールは、形態素解析におけるViterbiアルゴリズムのための
//! ラティス構造を提供します。ラティスはノードとパスから構成され、
//! 最適なトークン分割を見つけるために使用されます。
use crate::dictionary::connector::ConnectorCost;
use crate::dictionary::lexicon::WordParam;
use crate::dictionary::mapper::ConnIdCounter;
use crate::dictionary::word_idx::WordIdx;
use crate::dictionary::LexType;

use crate::common::{BOS_EOS_CONNECTION_ID, MAX_SENTENCE_LENGTH};

const MAX_COST: i32 = i32::MAX;
const INVALID_IDX: u16 = u16::MAX;

/// ラティス内のノード。
///
/// 各ノードは単語の候補を表し、位置情報、接続ID、最小コストなどを保持します。
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Node {
    /// 単語ID。
    pub word_id: u32,
    /// 辞書タイプ（システム辞書、ユーザー辞書など）。
    pub lex_type: LexType,
    /// ノードの開始位置（文字単位）。
    pub start_node: usize,
    /// 単語の開始位置（文字単位）。
    pub start_word: usize,
    /// 左側の接続ID。
    pub left_id: u16,
    /// 右側の接続ID。
    pub right_id: u16,
    /// 最小コストを持つ左側ノードのインデックス。
    pub min_idx: u16,
    /// BOSからこのノードまでの最小コスト。
    pub min_cost: i32,
    /// 左側から接続するパスの連結リストの先頭へのポインタ。
    /// パスがない場合はnull。
    pub lpath: *const Path,
}

/// ラティス内の2つのノード間の接続を表します。
///
/// パスは連結リストとして保存され、N-best探索に使用されます。
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Path {
    /// 左側のノードへのポインタ（BOSに近い方）。
    pub lnode: *const Node,
    /// 右側のノードから発生する次のパス（連結リスト）。
    pub lnext: *const Path,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            word_id: 0,
            lex_type: LexType::System,
            start_node: 0,
            start_word: 0,
            left_id: 0,
            right_id: 0,
            min_idx: 0,
            min_cost: i32::MAX,
            lpath: std::ptr::null(),
        }
    }
}

impl Node {
    /// 単語インデックスを取得します。
    #[inline(always)]
    pub fn word_idx(&self) -> WordIdx {
        WordIdx::new(self.lex_type, self.word_id)
    }

    /// このノードがBOSに接続されているかどうかを判定します。
    #[inline(always)]
    pub fn is_connected_to_bos(&self) -> bool {
        self.min_cost != MAX_COST
    }

    /// このノードがBOS（文頭）ノードかどうかを判定します。
    #[inline(always)]
    pub fn is_bos(&self) -> bool {
        self.start_node == MAX_SENTENCE_LENGTH
    }

    /// このノードがEOS（文末）ノードかどうかを判定します。
    #[inline(always)]
    pub fn is_eos(&self) -> bool {
        self.right_id == u16::MAX
    }
}

/// ラティスの種類を表す列挙型。
///
/// 1-best解用とN-best解用の2種類のラティスを区別します。
pub enum LatticeKind {
    /// 1-best解（最良の解のみ）用のラティス。
    For1Best(Lattice),
    /// N-best解（複数の候補解）用のラティス。
    ForNBest(LatticeNBest),
}

/// 1-best解用のラティス構造体。
///
/// Viterbiアルゴリズムを使用して最良のトークン分割を見つけるための
/// データ構造です。この実装はsudachi.rsにインスパイアされています。
#[derive(Default)]
pub struct Lattice {
    ends: Vec<Vec<Node>>,
    eos: Option<Node>,
    len_char: usize, // needed for avoiding to free ends
}

impl LatticeKind {
    /// 1-best解用にラティスを準備します。
    ///
    /// # 引数
    ///
    /// * `len_char` - 文の文字数
    ///
    /// # 戻り値
    ///
    /// 1-best用ラティスへの可変参照
    #[inline]
    pub fn prepare_for_1best(&mut self, len_char: usize) -> &mut Lattice {
        match self {
            LatticeKind::For1Best(l) => {
                l.reset(len_char);
                l
            }
            LatticeKind::ForNBest(_) => {
                *self = LatticeKind::For1Best(Lattice::default());
                self.prepare_for_1best(len_char)
            }
        }
    }

    /// N-best解用にラティスを準備します。
    ///
    /// # 引数
    ///
    /// * `len_char` - 文の文字数
    ///
    /// # 戻り値
    ///
    /// N-best用ラティスへの可変参照
    #[inline]
    pub fn prepare_for_nbest(&mut self, len_char: usize) -> &mut LatticeNBest {
        match self {
            LatticeKind::ForNBest(l) => {
                l.reset(len_char);
                l
            }
            LatticeKind::For1Best(_) => {
                *self = LatticeKind::ForNBest(LatticeNBest::default());
                self.prepare_for_nbest(len_char)
            }
        }
    }
}

impl Lattice {
    /// ラティスをリセットし、新しい文の処理を準備します。
    ///
    /// # 引数
    ///
    /// * `len_char` - 新しい文の文字数
    pub fn reset(&mut self, len_char: usize) {
        Self::reset_vec(&mut self.ends, len_char + 1);
        self.len_char = len_char;
        self.eos = None;
        self.insert_bos();
    }

    fn reset_vec<T>(data: &mut Vec<Vec<T>>, new_len: usize) {
        for v in data.iter_mut() {
            v.clear();
        }
        let cur_len = data.len();
        if cur_len <= new_len {
            data.reserve(new_len - cur_len);
            for _ in cur_len..new_len {
                data.push(Vec::with_capacity(16))
            }
        }
    }

    /// 設定された文の文字数を返します。
    ///
    /// # 戻り値
    ///
    /// 文字数
    #[inline(always)]
    pub const fn len_char(&self) -> usize {
        self.len_char
    }

    /// BOS（文頭）ノードを挿入します。
    fn insert_bos(&mut self) {
        self.ends[0].push(Node {
            word_id: u32::MAX,
            lex_type: LexType::default(),
            start_node: MAX_SENTENCE_LENGTH,
            start_word: MAX_SENTENCE_LENGTH,
            left_id: u16::MAX,
            right_id: BOS_EOS_CONNECTION_ID,
            min_idx: INVALID_IDX,
            min_cost: 0,
            lpath: std::ptr::null(),
        });
    }

    /// EOS（文末）ノードを挿入します。
    ///
    /// # 引数
    ///
    /// * `start_node` - EOSノードの開始位置
    /// * `connector` - 接続コスト計算用のコネクタ
    pub fn insert_eos<C>(&mut self, start_node: usize, connector: &C)
    where
        C: ConnectorCost,
    {
        let (min_idx, min_cost) =
            self.search_min_node(start_node, BOS_EOS_CONNECTION_ID, connector);
        self.eos = Some(Node {
            word_id: u32::MAX,
            lex_type: LexType::default(),
            start_node,
            start_word: self.len_char(),
            left_id: BOS_EOS_CONNECTION_ID,
            right_id: u16::MAX,
            min_idx,
            min_cost,
            lpath: std::ptr::null(),
        });
    }

    /// ラティスに新しいノードを挿入します。
    ///
    /// # 引数
    ///
    /// * `start_node` - ノードの開始位置
    /// * `start_word` - 単語の開始位置
    /// * `end_word` - 単語の終了位置
    /// * `word_idx` - 単語インデックス
    /// * `word_param` - 単語パラメータ（接続ID、コストなど）
    /// * `connector` - 接続コスト計算用のコネクタ
    pub fn insert_node<C>(
        &mut self,
        start_node: usize,
        start_word: usize,
        end_word: usize,
        word_idx: WordIdx,
        word_param: WordParam,
        connector: &C,
    ) where
        C: ConnectorCost,
    {
        debug_assert!(start_node <= start_word);
        debug_assert!(start_word < end_word);
        let (min_idx, min_cost) = self.search_min_node(start_node, word_param.left_id, connector);
        self.ends[end_word].push(Node {
            word_id: word_idx.word_id,
            lex_type: word_idx.lex_type,
            start_node,
            start_word,
            left_id: word_param.left_id,
            right_id: word_param.right_id,
            min_idx,
            min_cost: min_cost + i32::from(word_param.word_cost),
            lpath: std::ptr::null(),
        });
    }

    fn search_min_node<C>(&self, start_node: usize, left_id: u16, connector: &C) -> (u16, i32)
    where
        C: ConnectorCost,
    {
        debug_assert!(!self.ends[start_node].is_empty());

        let mut min_idx = INVALID_IDX;
        let mut min_cost = MAX_COST;
        for (i, left_node) in self.ends[start_node].iter().enumerate() {
            debug_assert!(left_node.is_connected_to_bos());
            let conn_cost = connector.cost(left_node.right_id, left_id);
            let new_cost = left_node.min_cost + conn_cost;
            // Depending on the order of tie-breaking, the result can be different from MeCab.
            // Using <= (not <) will produce results identical to MeCab in most case (empirically).
            if new_cost <= min_cost {
                min_idx = i as u16;
                min_cost = new_cost;
            }
        }

        debug_assert_ne!(min_idx, INVALID_IDX);
        (min_idx, min_cost)
    }

    /// 指定位置に少なくとも1つのノードが存在するかチェックします。
    ///
    /// # 引数
    ///
    /// * `i` - チェックする位置
    ///
    /// # 戻り値
    ///
    /// ノードが存在する場合は`true`、存在しない場合は`false`
    #[inline(always)]
    pub fn has_previous_node(&self, i: usize) -> bool {
        self.ends.get(i).map(|d| !d.is_empty()).unwrap_or(false)
    }

    /// 最良パスのノードをベクトルに追加します。
    ///
    /// EOSから後方にたどり、最良パスを構成するすべてのノードを追加します。
    ///
    /// # 引数
    ///
    /// * `top_nodes` - ノードを追加するベクトル
    pub fn append_top_nodes(&self, top_nodes: &mut Vec<(usize, Node)>) {
        let eos = self.eos.as_ref().unwrap();
        let mut end_node = eos.start_node;
        let mut min_idx = eos.min_idx;
        while end_node != 0 {
            let node = &self.ends[end_node][usize::from(min_idx)];
            top_nodes.push((end_node, *node));
            (end_node, min_idx) = (node.start_node, node.min_idx);
        }
    }

    /// 接続IDの出現回数をカウンタに追加します。
    ///
    /// # 引数
    ///
    /// * `counter` - 接続IDカウンタ
    pub fn add_connid_counts(&self, counter: &mut ConnIdCounter) {
        for end_char in 1..=self.len_char() {
            for r_node in &self.ends[end_char] {
                let start_node = r_node.start_node;
                for l_node in &self.ends[start_node] {
                    counter.add(r_node.left_id, l_node.right_id, 1);
                }
            }
        }
        let r_node = self.eos.as_ref().unwrap();
        for l_node in &self.ends[self.len_char()] {
            counter.add(r_node.left_id, l_node.right_id, 1);
        }
    }
}

/// N-best解用のラティス構造体。
///
/// 複数の候補パスを保持するために、各ノード間のすべての接続を保存します。
/// この実装はsudachi.rsにインスパイアされています。
#[derive(Default)]
pub struct LatticeNBest {
    arena: bumpalo::Bump,
    ends: Vec<Vec<*mut Node>>,
    eos: *mut Node,
    len_char: usize, // needed for avoiding to free ends
}

impl LatticeNBest {
    /// ラティスをリセットし、新しい文の処理を準備します。
    ///
    /// アリーナアロケータもリセットされます。
    ///
    /// # 引数
    ///
    /// * `len_char` - 新しい文の文字数
    pub fn reset(&mut self, len_char: usize) {
        self.arena.reset();

        let new_len = len_char + 1;

        for v in self.ends.iter_mut() {
            v.clear();
        }

        let cur_len = self.ends.len();
        if cur_len < new_len {
            self.ends.reserve(new_len - cur_len);
            for _ in cur_len..new_len {
                self.ends.push(Vec::with_capacity(16));
            }
        }

        self.eos = std::ptr::null_mut();
        self.len_char = len_char;
        self.insert_bos();
    }

    /// EOSノードを取得します。
    ///
    /// # 戻り値
    ///
    /// EOSノードが存在する場合は`Some(&Node)`、存在しない場合は`None`
    #[inline(always)]
    pub fn eos_node(&self) -> Option<&Node> {
        unsafe { self.eos.as_ref() }
    }

    /// 設定された文の文字数を返します。
    ///
    /// # 戻り値
    ///
    /// 文字数
    #[inline(always)]
    pub const fn len_char(&self) -> usize {
        self.len_char
    }

    /// BOS（文頭）ノードを挿入します。
    fn insert_bos(&mut self) {
        let bos_node = self.arena.alloc(Node {
            word_id: u32::MAX,
            lex_type: LexType::default(),
            start_node: MAX_SENTENCE_LENGTH,
            start_word: MAX_SENTENCE_LENGTH,
            left_id: u16::MAX,
            right_id: BOS_EOS_CONNECTION_ID,
            min_idx: INVALID_IDX,
            min_cost: 0,
            lpath: std::ptr::null(),
        });
        self.ends[0].push(bos_node);
    }

    /// EOS（文末）ノードを挿入し、すべての可能な接続を保存します。
    ///
    /// # 引数
    ///
    /// * `start_node` - EOSノードの開始位置
    /// * `connector` - 接続コスト計算用のコネクタ
    pub fn insert_eos<C: ConnectorCost>(&mut self, start_node: usize, connector: &C) {
        let eos_node = self.arena.alloc(Node {
            word_id: u32::MAX,
            lex_type: LexType::default(),
            start_node,
            start_word: self.len_char(),
            left_id: BOS_EOS_CONNECTION_ID,
            right_id: u16::MAX,
            ..Default::default()
        });

        let mut min_cost = MAX_COST;
        eos_node.lpath = std::ptr::null();

        for (i, &lnode_ptr) in self.ends[start_node].iter().enumerate() {
            let lnode = unsafe { &*lnode_ptr };
            let conn_cost = connector.cost(lnode.right_id, BOS_EOS_CONNECTION_ID);
            let new_cost = lnode.min_cost + conn_cost;

            if new_cost <= min_cost {
                min_cost = new_cost;
                eos_node.min_idx = i as u16;
            }
            let new_path = self.arena.alloc(Path { lnode: lnode_ptr, lnext: eos_node.lpath });
            eos_node.lpath = new_path;
        }
        eos_node.min_cost = min_cost;
        self.eos = eos_node;
    }

    /// ラティスに新しいノードを挿入し、すべての可能な接続パスを保存します。
    ///
    /// # 引数
    ///
    /// * `start_node_pos` - ノードの開始位置
    /// * `start_word` - 単語の開始位置
    /// * `end_word` - 単語の終了位置
    /// * `word_idx` - 単語インデックス
    /// * `word_param` - 単語パラメータ（接続ID、コストなど）
    /// * `connector` - 接続コスト計算用のコネクタ
    pub fn insert_node<C>(
        &mut self,
        start_node_pos: usize,
        start_word: usize,
        end_word: usize,
        word_idx: WordIdx,
        word_param: WordParam,
        connector: &C,
    ) where
        C: ConnectorCost,
    {
        debug_assert!(start_node_pos <= start_word);
        debug_assert!(start_word < end_word);

        let rnode_ptr = self.arena.alloc(Node {
            word_id: word_idx.word_id,
            lex_type: word_idx.lex_type,
            start_node: start_node_pos,
            start_word,
            left_id: word_param.left_id,
            right_id: word_param.right_id,
            ..Default::default()
        });
        let rnode = &mut *rnode_ptr;

        let mut min_cost = MAX_COST;
        let mut min_idx = INVALID_IDX;

        rnode.lpath = std::ptr::null();

        for (i, &lnode_ptr) in self.ends[start_node_pos].iter().enumerate() {
            let lnode = unsafe { &*lnode_ptr };
            if !lnode.is_connected_to_bos() {
                continue;
            }

            let conn_cost = connector.cost(lnode.right_id, rnode.left_id);
            let new_cost = lnode.min_cost.saturating_add(conn_cost);
            // Depending on the order of tie-breaking, the result can be different from MeCab.
            // Using <= (not <) will produce results identical to MeCab in most case (empirically).
            if new_cost <= min_cost {
                min_cost = new_cost;
                min_idx = i as u16;
            }

            let new_path = self.arena.alloc(Path {
                lnode: lnode_ptr,
                lnext: rnode.lpath,
            });
            rnode.lpath = new_path;
        }

        if min_idx != INVALID_IDX {
            rnode.min_idx = min_idx;
            rnode.min_cost = min_cost.saturating_add(i32::from(word_param.word_cost));
            self.ends[end_word].push(rnode_ptr);
        }
    }

    /// 指定位置に少なくとも1つのノードが存在するかチェックします。
    ///
    /// # 引数
    ///
    /// * `i` - チェックする位置
    ///
    /// # 戻り値
    ///
    /// ノードが存在する場合は`true`、存在しない場合は`false`
    #[inline(always)]
    pub fn has_previous_node(&self, i: usize) -> bool {
        self.ends.get(i).map(|d| !d.is_empty()).unwrap_or(false)
    }

    /// 接続IDの出現回数をカウンタに追加します。
    ///
    /// # 引数
    ///
    /// * `counter` - 接続IDカウンタ
    pub fn add_connid_counts(&self, counter: &mut ConnIdCounter) {
        for end_char in 1..=self.len_char() {
            for &r_node_ptr in &self.ends[end_char] {
                let r_node = unsafe { &*r_node_ptr };
                let start_node = r_node.start_node;

                for &l_node_ptr in &self.ends[start_node] {
                    let l_node = unsafe { &*l_node_ptr };
                    counter.add(r_node.left_id, l_node.right_id, 1);
                }
            }
        }

        if !self.eos.is_null() {
            let r_node = unsafe { &*self.eos };
            if let Some(last_nodes) = self.ends.get(r_node.start_node) {
                for &l_node_ptr in last_nodes {
                    let l_node = unsafe { &*l_node_ptr };
                    counter.add(r_node.left_id, l_node.right_id, 1);
                }
            }
        }
    }
}

impl std::fmt::Debug for Lattice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Lattice {{ eos: {:?}, ends: [", &self.eos)?;
        for (i, e) in self.ends[..=self.len_char()].iter().enumerate() {
            writeln!(f, "{i} => {e:?}")?;
        }
        writeln!(f, "]}}")
    }
}
