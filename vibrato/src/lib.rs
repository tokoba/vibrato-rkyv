//! # Vibrato-rkyv
//!
//! Vibratoは、ビタビアルゴリズムに基づく高速な形態素解析（トークン化）の実装です。
//!
//! ## 概要
//!
//! このライブラリは、日本語テキストの形態素解析を行うための高速なトークナイザーを提供します。
//! rkyvシリアライゼーションフォーマットを使用することで、辞書の読み込みと初期化を高速化し、
//! ゼロコピーでのデータアクセスを実現しています。
//!
//! ## 主な機能
//!
//! - **高速な形態素解析**: ビタビアルゴリズムを用いた効率的なトークン化
//! - **ゼロコピーデシリアライゼーション**: rkyvを使用した高速な辞書読み込み
//! - **柔軟な辞書構築**: MeCab形式の辞書ファイルからのビルド
//! - **N-best解析**: 複数の解析候補の生成（実験的機能）
//! - **学習機能**: 構造化パーセプトロンによるモデル学習（trainフィーチャー有効時）
//!
//! ## 使用例
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use vibrato_rkyv::{Dictionary, SystemDictionaryBuilder, Tokenizer};
//!
//! let lexicon_csv = "京都,4,4,5,京都,名詞,固有名詞,地名,一般,*,*,キョウト,京都,*,A,*,*,*,1/5
//! 東京都,5,5,9,東京都,名詞,固有名詞,地名,一般,*,*,トウキョウト,東京都,*,B,5/9,*,5/9,*";
//! let matrix_def = "10 10\n0 4 -5\n0 5 -9";
//! let char_def = "DEFAULT 0 1 0";
//! let unk_def = "DEFAULT,0,0,100,DEFAULT,名詞,普通名詞,*,*,*,*,*,*,*,*,*,*,*,*";
//!
//!
//! let dict = SystemDictionaryBuilder::from_readers(
//!     lexicon_csv.as_bytes(),
//!     matrix_def.as_bytes(),
//!     char_def.as_bytes(),
//!     unk_def.as_bytes(),
//! )?;
//!
//! let tokenizer = Tokenizer::from_inner(dict);
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

/// 共通の型定義とユーティリティ
pub mod common;

/// 辞書データ構造とビルダー
pub mod dictionary;

/// エラー型の定義
pub mod errors;

/// 数値型のユーティリティ
pub mod num;

/// 文の内部表現
mod sentence;

/// トークン型の定義
pub mod token;

/// トークナイザーの実装
pub mod tokenizer;

/// 内部ユーティリティ関数
pub mod utils;

/// レガシーフォーマットのサポート
#[cfg(feature = "legacy")]
mod legacy;

/// MeCab形式ファイルの読み書き
///
/// `train`フィーチャーが有効な場合のみ利用可能です。
#[cfg(feature = "train")]
#[cfg_attr(docsrs, doc(cfg(feature = "train")))]
pub mod mecab;

/// モデル学習機能
///
/// `train`フィーチャーが有効な場合のみ利用可能です。
/// 構造化パーセプトロンを用いたモデルパラメータの学習を提供します。
#[cfg(feature = "train")]
#[cfg_attr(docsrs, doc(cfg(feature = "train")))]
pub mod trainer;

#[cfg(all(test, feature = "train"))]
mod test_utils;
#[cfg(test)]
mod tests;

// Re-exports
pub use dictionary::{CacheStrategy, Dictionary, LoadMode, SystemDictionaryBuilder};
pub use tokenizer::Tokenizer;

/// このライブラリのバージョン番号
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
