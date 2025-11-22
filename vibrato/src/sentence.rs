//! 入力テキストの内部表現を提供するモジュール
//!
//! このモジュールは、形態素解析のために入力テキストを効率的に処理するための
//! 内部データ構造を提供します。入力文字列を文字単位に分割し、各文字の属性情報や
//! バイト位置のマッピング、文字のグループ化可能性などを計算・保持します。

use crate::dictionary::character::{ArchivedCharProperty, CharInfo, CharProperty};

/// 入力テキストの内部表現を保持する構造体
///
/// この構造体は、形態素解析のために入力テキストを処理し、以下の情報を保持します:
/// - 元の入力文字列
/// - 文字配列
/// - 文字位置からバイト位置へのマッピング
/// - 各文字の属性情報
/// - 各文字のグループ化可能性
///
/// # フィールド
///
/// * `input` - 元の入力文字列
/// * `chars` - 入力文字列を文字単位に分割した配列
/// * `c2b` - 文字位置からバイト位置へのマッピング配列
/// * `cinfos` - 各文字の属性情報を保持する配列
/// * `groupable` - 各文字位置からグループ化可能な文字数を保持する配列
#[derive(Default, Clone, Debug)]
pub struct Sentence {
    input: String,
    chars: Vec<char>,
    c2b: Vec<usize>,
    cinfos: Vec<CharInfo>,
    groupable: Vec<usize>,
}

impl Sentence {
    /// 新しい空の `Sentence` インスタンスを生成します
    ///
    /// # 戻り値
    ///
    /// 空の `Sentence` インスタンス
    pub fn new() -> Self {
        Self::default()
    }

    /// 内部状態をクリアします
    ///
    /// すべての内部フィールド（入力文字列、文字配列、マッピング情報など）を
    /// 空の状態にリセットします。
    #[inline(always)]
    pub fn clear(&mut self) {
        self.input.clear();
        self.chars.clear();
        self.c2b.clear();
        self.cinfos.clear();
        self.groupable.clear();
    }

    /// 入力文字列を設定します
    ///
    /// 既存の内部状態をクリアした後、新しい入力文字列を設定します。
    /// この時点では文字列の解析は行われません。解析を行うには [`compile`]
    /// または [`compile_archived`] を呼び出す必要があります。
    ///
    /// # 引数
    ///
    /// * `input` - 設定する入力文字列
    ///
    /// [`compile`]: Self::compile
    /// [`compile_archived`]: Self::compile_archived
    pub fn set_sentence<S>(&mut self, input: S)
    where
        S: AsRef<str>,
    {
        self.clear();
        self.input.push_str(input.as_ref());
    }

    /// 入力文字列を解析し、内部データ構造を構築します
    ///
    /// 設定された入力文字列に対して以下の処理を実行します:
    /// 1. 文字配列とバイト位置マッピングの計算
    /// 2. 各文字の属性情報の計算
    /// 3. 文字のグループ化可能性の計算
    ///
    /// # 引数
    ///
    /// * `char_prop` - 文字属性定義を保持する `CharProperty` への参照
    pub fn compile(&mut self, char_prop: &CharProperty) {
        self.compute_basic();
        self.compute_categories(char_prop);
        self.compute_groupable();
    }

    /// アーカイブされた文字属性を使用して入力文字列を解析します
    ///
    /// [`compile`] と同じ処理を行いますが、アーカイブされた文字属性定義を
    /// 使用します。これにより、デシリアライズのオーバーヘッドなしに
    /// 文字属性情報にアクセスできます。
    ///
    /// # 引数
    ///
    /// * `char_prop` - アーカイブされた文字属性定義への参照
    ///
    /// [`compile`]: Self::compile
    pub fn compile_archived(&mut self, char_prop: &ArchivedCharProperty) {
        self.compute_basic();
        self.compute_categories_archived(char_prop);
        self.compute_groupable();
    }

    /// 基本的な文字情報を計算します（内部メソッド）
    ///
    /// 入力文字列を文字単位に分割し、文字配列と文字位置からバイト位置への
    /// マッピング配列を構築します。
    fn compute_basic(&mut self) {
        for (bi, ch) in self.input.char_indices() {
            self.chars.push(ch);
            self.c2b.push(bi);
        }
        self.c2b.push(self.input.len());
    }

    /// 各文字の属性情報を計算します（内部メソッド）
    ///
    /// 文字属性定義を使用して、各文字の属性情報（カテゴリなど）を取得し、
    /// 内部配列に保存します。
    ///
    /// # 引数
    ///
    /// * `char_prop` - 文字属性定義を保持する `CharProperty` への参照
    fn compute_categories(&mut self, char_prop: &CharProperty) {
        debug_assert!(!self.chars.is_empty());

        self.cinfos.reserve(self.chars.len());
        for &c in &self.chars {
            self.cinfos.push(char_prop.char_info(c));
        }
    }

    /// アーカイブされた文字属性を使用して各文字の属性情報を計算します（内部メソッド）
    ///
    /// [`compute_categories`] と同じ処理を行いますが、アーカイブされた
    /// 文字属性定義を使用します。
    ///
    /// # 引数
    ///
    /// * `char_prop` - アーカイブされた文字属性定義への参照
    ///
    /// [`compute_categories`]: Self::compute_categories
    fn compute_categories_archived(&mut self, char_prop: &ArchivedCharProperty) {
        debug_assert!(!self.chars.is_empty());

        self.cinfos.reserve(self.chars.len());
        for &c in &self.chars {
            self.cinfos.push(char_prop.char_info(c));
        }
    }

    /// 各文字位置からグループ化可能な文字数を計算します（内部メソッド）
    ///
    /// 隣接する文字が同じカテゴリに属する場合、それらをグループ化できるとみなし、
    /// 各位置から連続してグループ化可能な文字数を計算します。
    /// この情報は未知語処理において使用されます。
    fn compute_groupable(&mut self) {
        debug_assert!(!self.chars.is_empty());
        debug_assert_eq!(self.chars.len(), self.cinfos.len());

        self.groupable.resize(self.chars.len(), 1);
        let mut rhs = self.cinfos.last().unwrap().cate_idset();

        for i in (1..self.chars.len()).rev() {
            let lhs = self.cinfos[i - 1].cate_idset();
            if (lhs & rhs) != 0 {
                self.groupable[i - 1] = self.groupable[i] + 1;
            }
            rhs = lhs;
        }
    }

    /// 元の入力文字列への参照を返します
    ///
    /// # 戻り値
    ///
    /// 元の入力文字列への不変参照
    #[inline(always)]
    pub fn raw(&self) -> &str {
        &self.input
    }

    /// 文字配列への参照を返します
    ///
    /// 入力文字列を文字単位に分割した配列への参照を返します。
    ///
    /// # 戻り値
    ///
    /// 文字配列への不変参照
    #[inline(always)]
    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    /// 文字数を返します
    ///
    /// 入力文字列の文字数（バイト数ではない）を返します。
    ///
    /// # 戻り値
    ///
    /// 文字数
    #[inline(always)]
    pub fn len_char(&self) -> usize {
        self.chars.len()
    }

    /// 指定された文字位置に対応するバイト位置を返します
    ///
    /// 文字位置（0始まり）からバイト位置へのマッピングを提供します。
    /// これは、元の入力文字列内での部分文字列の抽出などに使用されます。
    ///
    /// # 引数
    ///
    /// * `pos_char` - 文字位置（0始まり）
    ///
    /// # 戻り値
    ///
    /// 対応するバイト位置
    #[inline(always)]
    pub fn byte_position(&self, pos_char: usize) -> usize {
        self.c2b[pos_char]
    }

    /// 指定された文字位置の文字属性情報を返します
    ///
    /// 指定された位置の文字の属性情報（カテゴリIDセットなど）を返します。
    ///
    /// # 引数
    ///
    /// * `pos_char` - 文字位置（0始まり）
    ///
    /// # 戻り値
    ///
    /// 文字属性情報
    #[inline(always)]
    pub fn char_info(&self, pos_char: usize) -> CharInfo {
        self.cinfos[pos_char]
    }

    /// 指定された文字位置からグループ化可能な文字数を返します
    ///
    /// 指定された位置から、同じカテゴリに属する文字が連続している数を返します。
    /// この情報は未知語処理において、連続する同種の文字をまとめて扱う際に使用されます。
    ///
    /// # 引数
    ///
    /// * `pos_char` - 文字位置（0始まり）
    ///
    /// # 戻り値
    ///
    /// グループ化可能な文字数
    #[inline(always)]
    pub fn groupable(&self, pos_char: usize) -> usize {
        self.groupable[pos_char]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentence() {
        let mut sent = Sentence::new();
        sent.set_sentence("自然");
        sent.compute_basic();
        assert_eq!(sent.chars(), &['自', '然']);
        assert_eq!(sent.byte_position(0), 0);
        assert_eq!(sent.byte_position(1), 3);
        assert_eq!(sent.byte_position(2), 6);
    }
}
