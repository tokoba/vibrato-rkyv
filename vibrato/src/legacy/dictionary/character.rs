//! 文字プロパティ定義
//!
//! このモジュールは、`char.def`で定義される文字情報を管理します。
//! 各文字のカテゴリ、グループ化、未知語処理などの属性を保持します。

use std::fmt;

use bincode::{Decode, Encode};


const CATE_IDSET_BITS: usize = 18;
const CATE_IDSET_MASK: u32 = (1 << CATE_IDSET_BITS) - 1;
const BASE_ID_BITS: usize = 8;
const BASE_ID_MASK: u32 = (1 << BASE_ID_BITS) - 1;

/// `char.def`で定義される文字の情報
///
/// この構造体は、文字の各種属性を32ビット整数にパックして保持します。
///
/// # メモリレイアウト
///
/// ```text
/// cate_idset = 18 ビット
///    base_id =  8 ビット
///     invoke =  1 ビット
///      group =  1 ビット
///     length =  4 ビット
/// ```
#[derive(Default, Clone, Copy, Decode, Encode)]
pub struct CharInfo(u32);

impl fmt::Debug for CharInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CharInfo")
            .field("cate_idset", &self.cate_idset())
            .field("base_id", &self.base_id())
            .field("invoke", &self.invoke())
            .field("group", &self.group())
            .field("length", &self.length())
            .finish()
    }
}

impl CharInfo {
    /// カテゴリIDセットを取得します。
    ///
    /// # 戻り値
    ///
    /// 18ビットのカテゴリIDセット
    #[inline(always)]
    pub const fn cate_idset(&self) -> u32 {
        self.0 & CATE_IDSET_MASK
    }

    /// ベースIDを取得します。
    ///
    /// # 戻り値
    ///
    /// 8ビットのベースID
    #[inline(always)]
    pub const fn base_id(&self) -> u32 {
        (self.0 >> CATE_IDSET_BITS) & BASE_ID_MASK
    }

    /// invoke フラグを取得します。
    ///
    /// # 戻り値
    ///
    /// 未知語処理を起動するかどうか
    #[inline(always)]
    pub const fn invoke(&self) -> bool {
        (self.0 >> (CATE_IDSET_BITS + BASE_ID_BITS)) & 1 != 0
    }

    /// group フラグを取得します。
    ///
    /// # 戻り値
    ///
    /// 文字をグループ化するかどうか
    #[inline(always)]
    pub const fn group(&self) -> bool {
        (self.0 >> (CATE_IDSET_BITS + BASE_ID_BITS + 1)) & 1 != 0
    }

    /// 長さを取得します。
    ///
    /// # 戻り値
    ///
    /// 4ビットの長さ値
    #[inline(always)]
    pub const fn length(&self) -> u16 {
        (self.0 >> (CATE_IDSET_BITS + BASE_ID_BITS + 2)) as u16
    }
}

/// 文字から情報へのマッピング
///
/// この構造体は、各文字（Unicodeコードポイント）に対応する文字情報と
/// カテゴリ名のマッピングを保持します。
#[derive(Decode, Encode)]
pub struct CharProperty {
    /// 文字（コードポイント）から文字情報へのマッピング
    chr2inf: Vec<CharInfo>,
    /// カテゴリIDでインデックス化されたカテゴリ名のリスト
    categories: Vec<String>,
}
