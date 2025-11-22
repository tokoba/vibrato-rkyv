//! 数値型定義
//!
//! このモジュールは、レガシー形式で使用される特殊な数値型を定義します。
use bincode::{
    de::Decoder,
    enc::Encoder,
    error::{AllowedEnumVariants, DecodeError, EncodeError},
    Decode, Encode,
};

/// 0から2^31 - 1までの整数を表します。
///
/// この型は、32ビット整数の符号ビットが常にゼロであることを保証します。
/// これにより、正の整数のみを扱うことができ、ビット演算を安全に行うことができます。
#[derive(Clone, Copy, Default, Eq, PartialEq, Debug, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct U31(u32);

impl U31 {
    /// U31型の最大値（2^31 - 1）
    pub const MAX: Self = Self(0x7fff_ffff);

    /// 指定された値からU31を作成します。
    ///
    /// # 引数
    ///
    /// * `x` - U31型に変換する32ビット符号なし整数
    ///
    /// # 戻り値
    ///
    /// 値が範囲内（0 ≤ x ≤ 2^31 - 1）の場合は`Some(U31)`を、
    /// 範囲外の場合は`None`を返します。
    #[inline(always)]
    pub const fn new(x: u32) -> Option<Self> {
        if x <= Self::MAX.get() {
            Some(Self(x))
        } else {
            None
        }
    }

    /// U31型の内部値をu32として取得します。
    ///
    /// # 戻り値
    ///
    /// 内部に保持されている32ビット符号なし整数値
    #[inline(always)]
    pub const fn get(self) -> u32 {
        self.0
    }
}

const U31_VALID_RANGE: AllowedEnumVariants = AllowedEnumVariants::Range {
    min: 0,
    max: U31::MAX.get(),
};

impl<Context> Decode<Context> for U31 {
    /// デコーダーからU31値をデコードします。
    ///
    /// # 引数
    ///
    /// * `decoder` - 値をデコードするためのデコーダー
    ///
    /// # 戻り値
    ///
    /// デコードされたU31値、または範囲外の値の場合はエラー
    ///
    /// # エラー
    ///
    /// デコードされた値が有効範囲外の場合、`DecodeError::UnexpectedVariant`を返します。
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecodeError> {
        let x = Decode::decode(decoder)?;
        Self::new(x).ok_or(DecodeError::UnexpectedVariant {
            type_name: "U31",
            allowed: &U31_VALID_RANGE,
            found: x,
        })
    }
}

bincode::impl_borrow_decode!(U31);

impl Encode for U31 {
    /// U31値をエンコーダーにエンコードします。
    ///
    /// # 引数
    ///
    /// * `encoder` - 値をエンコードするためのエンコーダー
    ///
    /// # 戻り値
    ///
    /// エンコードが成功した場合は`Ok(())`、失敗した場合はエラー
    ///
    /// # エラー
    ///
    /// エンコード処理中にエラーが発生した場合、`EncodeError`を返します。
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncodeError> {
        Encode::encode(&self.0, encoder)?;
        Ok(())
    }
}
