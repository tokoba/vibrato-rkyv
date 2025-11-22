//! コーパスデータ構造のモジュール。
//!
//! このモジュールは、学習用コーパスの読み込みと管理に必要なデータ構造を提供します。

use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::ops::{Deref, DerefMut};

use crate::errors::{Result, VibratoError};
use crate::sentence::Sentence;

/// 表層形と素性のペアの表現。
///
/// 学習データの単語を表します。
pub struct Word {
    surface: String,

    // Since a vector of strings consumes massive memory, a single string is stored and divided as
    // needed.
    feature: String,
}

impl Word {
    /// 新しい単語を作成します。
    ///
    /// # 引数
    ///
    /// * `surface` - 表層形
    /// * `feature` - 素性文字列
    ///
    /// # 戻り値
    ///
    /// 作成された単語
    pub(crate) fn new(surface: &str, feature: &str) -> Self {
        Self {
            surface: surface.to_string(),
            feature: feature.to_string(),
        }
    }

    /// 表層形の文字列を返します。
    ///
    /// # 戻り値
    ///
    /// 表層形
    pub fn surface(&self) -> &str {
        &self.surface
    }

    /// 連結された素性文字列を返します。
    ///
    /// # 戻り値
    ///
    /// 素性文字列
    pub fn feature(&self) -> &str {
        &self.feature
    }
}

/// 文の表現。
///
/// 学習データの1つの例文を表します。
pub struct Example {
    /// トークンの連結。
    pub(crate) sentence: Sentence,

    /// トークンのリスト。
    pub(crate) tokens: Vec<Word>,
}

impl Example {
    /// 例文を指定されたシンクに書き込みます。
    ///
    /// # 引数
    ///
    /// * `wtr` - 書き込み先
    ///
    /// # 戻り値
    ///
    /// 書き込み成功時は `Ok(())`
    ///
    /// # エラー
    ///
    /// 書き込みに失敗した場合、I/Oエラーが返されます。
    pub fn write<W>(&self, wtr: W) -> Result<()>
    where
        W: Write,
    {
        let mut wtr = BufWriter::new(wtr);
        for word in &self.tokens {
            writeln!(&mut wtr, "{}\t{}", word.surface, word.feature)?;
        }
        writeln!(&mut wtr, "EOS")?;
        Ok(())
    }

    /// トークンのスライスを返します。
    ///
    /// # 戻り値
    ///
    /// トークンのスライス
    pub fn tokens(&self) -> &[Word] {
        &self.tokens
    }
}

/// コーパスの表現。
///
/// 学習データの例文集合を表します。
pub struct Corpus {
    /// 例文のリスト。
    pub(crate) examples: Vec<Example>,
}

impl Corpus {
    /// 指定されたシンクからコーパスを読み込みます。
    ///
    /// コーパスファイルは、各行が「表層形\t素性」の形式で、
    /// 文の終わりに「EOS」が含まれる形式を想定しています。
    ///
    /// # 引数
    ///
    /// * `rdr` - コーパスのリーダー
    ///
    /// # 戻り値
    ///
    /// 読み込まれたコーパス
    ///
    /// # エラー
    ///
    /// 入力形式が不正な場合、[`VibratoError`] が返されます。
    pub fn from_reader<R>(rdr: R) -> Result<Self>
    where
        R: Read,
    {
        let buf = BufReader::new(rdr);

        let mut examples = vec![];
        let mut tokens = vec![];
        for line in buf.lines() {
            let line = line?;
            let mut spl = line.split('\t');
            let surface = spl.next();
            let feature = spl.next();
            let rest = spl.next();
            match (surface, feature, rest) {
                (Some(surface), Some(feature), None) => {
                    tokens.push(Word {
                        surface: surface.to_string(),
                        feature: feature.to_string(),
                    });
                }
                (Some("EOS"), None, None) => {
                    let mut sentence = Sentence::new();
                    let mut input = String::new();
                    for token in &tokens {
                        input.push_str(token.surface());
                    }
                    if !input.is_empty() {
                        sentence.set_sentence(input);
                        examples.push(Example { sentence, tokens });
                    }
                    tokens = vec![];
                }
                _ => {
                    return Err(VibratoError::invalid_format(
                        "rdr",
                        "Each line must be a pair of a surface and features or `EOS`",
                    ))
                }
            }
        }

        Ok(Self { examples })
    }
}

impl Deref for Corpus {
    type Target = [Example];

    fn deref(&self) -> &Self::Target {
        &self.examples
    }
}

impl DerefMut for Corpus {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.examples
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_corpus() {
        let corpus_data = "\
トスカーナ\t名詞,トスカーナ
地方\t名詞,チホー
に\t助詞,ニ
行く\t動詞,イク
EOS
火星\t名詞,カセー
猫\t名詞,ネコ
EOS
";

        let corpus = Corpus::from_reader(corpus_data.as_bytes()).unwrap();

        assert_eq!(2, corpus.examples.len());

        let sentence1 = &corpus.examples[0];

        assert_eq!("トスカーナ地方に行く", sentence1.sentence.raw());

        assert_eq!(4, sentence1.tokens.len());
        assert_eq!("トスカーナ", sentence1.tokens[0].surface());
        assert_eq!("名詞,トスカーナ", sentence1.tokens[0].feature());
        assert_eq!("地方", sentence1.tokens[1].surface());
        assert_eq!("名詞,チホー", sentence1.tokens[1].feature());
        assert_eq!("に", sentence1.tokens[2].surface());
        assert_eq!("助詞,ニ", sentence1.tokens[2].feature());
        assert_eq!("行く", sentence1.tokens[3].surface());
        assert_eq!("動詞,イク", sentence1.tokens[3].feature());

        let sentence2 = &corpus.examples[1];

        assert_eq!("火星猫", sentence2.sentence.raw());

        assert_eq!(2, sentence2.tokens.len());
        assert_eq!("火星", sentence2.tokens[0].surface());
        assert_eq!("名詞,カセー", sentence2.tokens[0].feature());
        assert_eq!("猫", sentence2.tokens[1].surface());
        assert_eq!("名詞,ネコ", sentence2.tokens[1].feature());
    }
}
