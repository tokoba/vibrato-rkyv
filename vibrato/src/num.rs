//! 31ビット符号なし整数型を提供するモジュール
//!
//! このモジュールは、0から2^31-1までの範囲の整数を表現する`U31`型を定義します。
//! 32ビット整数の符号ビットが常にゼロであることを保証することで、
//! 特定の用途において安全かつ効率的な整数演算を可能にします。

use rkyv::{Archive, Deserialize, Serialize};

/// 0から2^31-1までの整数を表現する型
///
/// この型は、32ビット整数の符号ビットが常にゼロであることを保証します。
/// 内部的にはu32を使用していますが、最上位ビットが設定されていない値のみを許可します。
///
/// # 特徴
///
/// - rkyv によるゼロコピーシリアライゼーション対応
/// - 透過的な表現により、メモリレイアウトはu32と同一
/// - 各種比較演算とハッシュ化に対応
#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, PartialOrd, Ord, Archive, Serialize, Deserialize)]
#[rkyv(compare(PartialEq), derive(Clone, Copy))]
#[repr(transparent)]
pub struct U31(pub u32);

impl U31 {
    /// U31型で表現可能な最大値(2^31 - 1 = 0x7fff_ffff)
    pub const MAX: Self = Self(0x7fff_ffff);

    /// 指定されたu32値からU31を生成する
    ///
    /// 引数がU31の範囲内(0 ≤ x ≤ 2^31-1)であれば`Some(U31)`を返し、
    /// 範囲外であれば`None`を返します。
    ///
    /// # 引数
    ///
    /// * `x` - U31に変換するu32値
    ///
    /// # 戻り値
    ///
    /// * `Some(U31)` - 変換に成功した場合
    /// * `None` - xがU31の範囲を超えている場合
    ///
    /// # 例
    ///
    /// ```
    /// # use vibrato::num::U31;
    /// assert!(U31::new(100).is_some());
    /// assert!(U31::new(0x7fff_ffff).is_some());
    /// assert!(U31::new(0x8000_0000).is_none());
    /// ```
    #[inline(always)]
    pub const fn new(x: u32) -> Option<Self> {
        if x <= Self::MAX.get() {
            Some(Self(x))
        } else {
            None
        }
    }

    /// U31から内部のu32値を取得する
    ///
    /// # 戻り値
    ///
    /// 内部に保持されているu32値(0 ≤ 値 ≤ 2^31-1)
    #[inline(always)]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl ArchivedU31 {
    /// アーカイブされたU31をネイティブ表現に変換する
    ///
    /// rkyv でシリアライズされた U31 を、通常の U31 型に変換します。
    /// エンディアン変換などの必要な処理を自動的に行います。
    ///
    /// # 戻り値
    ///
    /// ネイティブ表現の U31 型
    pub fn to_native(self) -> U31 {
        U31(self.0.to_native())
    }
}
