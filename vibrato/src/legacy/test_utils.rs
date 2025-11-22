//! テストユーティリティ
//!
//! このモジュールは、レガシー形式のテストで使用されるユーティリティマクロを提供します。

/// ハッシュマップを簡潔に作成するためのマクロ
///
/// このマクロは、キーと値のペアからHashMapを作成する便利な方法を提供します。
///
/// # 使用例
///
/// ```ignore
/// let map = hashmap! {
///     "key1" => "value1",
///     "key2" => "value2",
/// };
/// ```
macro_rules! hashmap {
    ( $($k:expr => $v:expr,)* ) => {
        {
            #[allow(unused_mut)]
            let mut h = hashbrown::HashMap::new();
            $(
                h.insert($k, $v);
            )*
            h
        }
    };
    ( $($k:expr => $v:expr),* ) => {
        hashmap![$( $k => $v, )*]
    };
}

pub(crate) use hashmap;
