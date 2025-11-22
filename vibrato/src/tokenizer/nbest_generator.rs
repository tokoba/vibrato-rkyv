//! N-best解生成モジュール。
//!
//! このモジュールは、A*探索アルゴリズムを使用してトークン化の
//! 上位N個の最良解を生成する機能を提供します。
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::rc::Rc;

use super::lattice::Node;
use crate::dictionary::connector::ConnectorCost;
use crate::dictionary::DictionaryInnerRef;
use crate::tokenizer::lattice::LatticeNBest;

// The following structs are designed to reconstruct paths from the A* search result.
// A path is stored as a linked list, which is pointed to by a QueueItem.
//
// QueueItem -> Path (node: EOS) -> Path (node: n-1) -> ... -> Path (node: BOS)

/// A*探索によって探索中の部分パス。
///
/// 文の終端から始端への連結リストを形成します。
#[derive(Debug)]
struct SearchPath {
    /// パスの現在位置にあるノード。
    node: *const Node,
    /// パス内の次のノードへのポインタ（BOS方向）。
    prev: Option<Rc<SearchPath>>,
    /// EOSからこのノードまでの総コスト（後方コスト）。
    backward_cost: i32,
}

/// A*探索のための優先度付きキュー内のアイテム。
#[derive(Debug)]
struct QueueItem {
    /// 現在の部分パスへのポインタ。
    path: Rc<SearchPath>,
    /// パスの優先度。f(x) = g(x) + h(x)として計算されます。
    ///  - g(x)はEOSからの後方コスト（backward_cost）。
    ///  - h(x)はBOSからの前方コスト（min_cost）で、ノードに保存されています。
    priority: i32,
}

impl PartialEq for QueueItem { fn eq(&self, other: &Self) -> bool { self.priority == other.priority } }
impl Eq for QueueItem {}
impl PartialOrd for QueueItem { fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) } }
impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> Ordering { other.priority.cmp(&self.priority) } // Invert to create a min-heap
}

/// N-bestトークン化結果のジェネレータ。
///
/// A*探索アルゴリズムを使用して、コストが低い順に
/// トークン化パスを生成するイテレータとして機能します。
pub struct NbestGenerator<'a> {
    queue: BinaryHeap<QueueItem>,
    connector: &'a dyn ConnectorCost,
    dictionary: DictionaryInnerRef<'a>,
}

impl<'a> NbestGenerator<'a> {
    /// 新しいN-bestジェネレータを作成します。
    ///
    /// # 引数
    ///
    /// * `lattice` - N-best用のラティス
    /// * `connector` - 接続コスト計算用のコネクタ
    /// * `dictionary` - 辞書への参照
    ///
    /// # 戻り値
    ///
    /// 新しいN-bestジェネレータインスタンス
    pub fn new(
        lattice: &'a LatticeNBest,
        connector: &'a dyn ConnectorCost,
        dictionary: DictionaryInnerRef<'a>,
    ) -> Self {
        let mut queue = BinaryHeap::new();
        if let Some(eos_node) = lattice.eos_node() {
            let initial_path = Rc::new(SearchPath {
                node: eos_node as *const Node,
                prev: None,
                backward_cost: 0,
            });
            queue.push(QueueItem {
                priority: eos_node.min_cost, // f(x) = g(x) + h(x) = 0 + h(BOS->EOS)
                path: initial_path,
            });
        }
        Self { queue, connector, dictionary }
    }
}

impl<'a> Iterator for NbestGenerator<'a> {
    /// イテレータが返す要素の型。
    ///
    /// ノードポインタのベクトルとパスの総コストのタプル。
    type Item = (Vec<*const Node>, i32);

    /// 次のN-bestパスを取得します。
    ///
    /// A*探索を使用して、次に低コストなパスを見つけて返します。
    ///
    /// # 戻り値
    ///
    /// パスが見つかった場合は`Some((ノードのベクトル, コスト))`、
    /// すべてのパスが探索済みの場合は`None`
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(item) = self.queue.pop() {
            let current_path = &item.path;
            let current_node = unsafe { &*current_path.node };

            // If we reached the BOS, a full path has been found.
            if current_node.is_bos() {
                let mut path_nodes = Vec::new();
                let mut p = Some(Rc::clone(current_path));
                while let Some(seg) = p {
                    let node = unsafe { &*seg.node };
                    if !node.is_bos() && !node.is_eos() {
                        path_nodes.push(seg.node);
                    }
                    p = seg.prev.clone();
                }
                return Some((path_nodes, item.priority));
            }

            let mut lpath_ptr = current_node.lpath;
            // Expand to previous nodes.
            while !lpath_ptr.is_null() {
                let lpath = unsafe { &*lpath_ptr };
                let prev_node_ptr = lpath.lnode;
                let prev_node = unsafe { &*prev_node_ptr };

                let conn_cost = self.connector.cost(prev_node.right_id, current_node.left_id);
                let word_cost = if current_node.is_bos() || current_node.is_eos() {
                    0
                } else {
                    self.dictionary.word_param(current_node.word_idx()).word_cost
                };
                let new_backward_cost = current_path.backward_cost + conn_cost + i32::from(word_cost);
                let new_priority = new_backward_cost + prev_node.min_cost; // f(x) = g(x) + h(x)

                let new_path = Rc::new(SearchPath {
                    node: prev_node_ptr,
                    prev: Some(Rc::clone(current_path)),
                    backward_cost: new_backward_cost,
                });
                self.queue.push(QueueItem { path: new_path, priority: new_priority });

                lpath_ptr = lpath.lnext;
            }
        }
        None
    }
}
