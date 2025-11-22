//! テスト用ユーティリティ
//!
//! テストコードで使用する便利なマクロや関数を提供します。

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
