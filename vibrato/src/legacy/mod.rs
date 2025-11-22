//! レガシー形式サポートモジュール
//!
//! このモジュールは、Vibratoのレガシー辞書形式をサポートするための機能を提供します。
//! Viterbiアルゴリズムに基づく高速なトークン化（形態素解析）を実装しています。
//!
//! # 概要
//!
//! レガシーフォーマットは、MeCabなどの従来の形態素解析器で使用されていた
//! 辞書形式との互換性を保つために提供されています。このモジュールを使用することで、
//! 既存の辞書データをVibratoで利用することができます。
//!
//! # 使用例
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::fs::File;
//!
//! use vibrato::{SystemDictionaryBuilder, Tokenizer};
//!
//! // 生の辞書ファイルセットを読み込む
//! let dict = SystemDictionaryBuilder::from_readers(
//!     File::open("src/tests/resources/lex.csv")?,
//!     File::open("src/tests/resources/matrix.def")?,
//!     File::open("src/tests/resources/char.def")?,
//!     File::open("src/tests/resources/unk.def")?,
//! )?;
//! // または、コンパイル済み辞書を読み込む
//! // let reader = File::open("path/to/system.dic")?;
//! // let dict = Dictionary::read(reader)?;
//!
//! let tokenizer = Tokenizer::new(dict);
//! let mut worker = tokenizer.new_worker();
//!
//! worker.reset_sentence("京都東京都");
//! worker.tokenize();
//! assert_eq!(worker.num_tokens(), 2);
//!
//! let t0 = worker.token(0);
//! assert_eq!(t0.surface(), "京都");
//! assert_eq!(t0.range_char(), 0..2);
//! assert_eq!(t0.range_byte(), 0..6);
//! assert_eq!(t0.feature(), "京都,名詞,固有名詞,地名,一般,*,*,キョウト,京都,*,A,*,*,*,1/5");
//!
//! let t1 = worker.token(1);
//! assert_eq!(t1.surface(), "東京都");
//! assert_eq!(t1.range_char(), 2..5);
//! assert_eq!(t1.range_byte(), 6..15);
//! assert_eq!(t1.feature(), "東京都,名詞,固有名詞,地名,一般,*,*,トウキョウト,東京都,*,B,5/9,*,5/9,*");
//! # Ok(())
//! # }
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
compile_error!("`target_pointer_width` must be 32 or 64");

mod common;
pub mod dictionary;
pub mod errors;
mod num;

pub use dictionary::Dictionary;

