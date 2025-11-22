//! Vibratoのテストモジュール群
//!
//! 各コンポーネント(connector、lexicon、tokenizer、trainer等)の
//! 動作を検証するテストを含みます。

mod connector;
mod lexicon;
mod tokenizer;

#[cfg(feature = "train")]
mod trainer;
