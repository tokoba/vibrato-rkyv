//! Vibratoの共通設定
//!
//! このモジュールは、Vibratoレガシー形式における共通の設定を提供します。
use bincode::config::{self, Fixint, LittleEndian};

/// シリアライゼーションの共通bincode設定を取得します。
///
/// この関数は、リトルエンディアンと固定長整数エンコーディングを使用する
/// bincode設定を返します。これにより、異なるプラットフォーム間での
/// 一貫したデータシリアライゼーションが保証されます。
///
/// # 戻り値
///
/// リトルエンディアンと固定長整数エンコーディングが設定された
/// bincode設定オブジェクト
pub const fn bincode_config() -> config::Configuration<LittleEndian, Fixint> {
    config::standard()
        .with_little_endian()
        .with_fixed_int_encoding()
}
