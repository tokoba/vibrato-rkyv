//! トレーナーの設定モジュール。
//!
//! このモジュールは、形態素解析モデルの学習に必要な設定を管理します。

use std::io::{BufRead, BufReader, Read};

use rkyv::{Archive, Deserialize, Serialize};

use crate::dictionary::character::CharProperty;
use crate::dictionary::connector::{ConnectorWrapper, MatrixConnector};
use crate::dictionary::lexicon::Lexicon;
use crate::dictionary::unknown::UnkHandler;
use crate::dictionary::{DictionaryInner, SystemDictionaryBuilder};
use crate::errors::{Result, VibratoError};
use crate::trainer::feature_extractor::FeatureExtractor;
use crate::trainer::feature_rewriter::{FeatureRewriter, FeatureRewriterBuilder};

/// トレーナーの設定。
///
/// 素性抽出器、素性書き換え器、辞書、表層形のリストを保持します。
#[derive(Archive, Serialize, Deserialize)]
pub struct TrainerConfig {
    pub(crate) feature_extractor: FeatureExtractor,
    pub(crate) unigram_rewriter: FeatureRewriter,
    pub(crate) left_rewriter: FeatureRewriter,
    pub(crate) right_rewriter: FeatureRewriter,
    pub(crate) dict: DictionaryInner,
    pub(crate) surfaces: Vec<String>,
}

impl TrainerConfig {
    /// 素性設定ファイルを解析します。
    ///
    /// feature.def ファイルから UNIGRAM と BIGRAM のテンプレートを読み込みます。
    ///
    /// # 引数
    ///
    /// * `rdr` - 素性設定ファイルのリーダー
    ///
    /// # 戻り値
    ///
    /// 素性抽出器
    ///
    /// # エラー
    ///
    /// ファイル形式が不正な場合、[`VibratoError`] が返されます。
    pub(crate) fn parse_feature_config<R>(rdr: R) -> Result<FeatureExtractor>
    where
        R: Read,
    {
        let reader = BufReader::new(rdr);

        let mut unigram_templates = vec![];
        let mut bigram_templates = vec![];

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(template) = line.strip_prefix("UNIGRAM ") {
                unigram_templates.push(template.to_string());
            } else if let Some(template) = line.strip_prefix("BIGRAM ") {
                let mut spl = template.split('/');
                let left = spl.next();
                let right = spl.next();
                let rest = spl.next();
                if let (Some(left), Some(right), None) = (left, right, rest) {
                    bigram_templates.push((left.to_string(), right.to_string()));
                } else {
                    return Err(VibratoError::invalid_format(
                        "feature.def",
                        "Invalid bigram template",
                    ));
                }
            } else {
                return Err(VibratoError::invalid_format("feature", ""));
            }
        }

        Ok(FeatureExtractor::new(&unigram_templates, &bigram_templates))
    }

    /// 書き換えルールを解析します。
    ///
    /// 行を解析し、パターンと書き換え文字列のペアを抽出します。
    ///
    /// # 引数
    ///
    /// * `line` - 書き換えルールの行
    ///
    /// # 戻り値
    ///
    /// パターンと書き換え文字列のタプル
    ///
    /// # エラー
    ///
    /// ルールの形式が不正な場合、[`VibratoError`] が返されます。
    fn parse_rewrite_rule(line: &str) -> Result<(Vec<&str>, Vec<&str>)> {
        let mut spl = line.split_ascii_whitespace();
        let pattern = spl.next();
        let rewrite = spl.next();
        let rest = spl.next();
        if let (Some(pattern), Some(rewrite), None) = (pattern, rewrite, rest) {
            Ok((pattern.split(',').collect(), rewrite.split(',').collect()))
        } else {
            Err(VibratoError::invalid_format(
                "rewrite.def",
                "invalid rewrite rule",
            ))
        }
    }

    /// 書き換え設定ファイルを解析します。
    ///
    /// rewrite.def ファイルから unigram、left、right の書き換えルールを読み込みます。
    ///
    /// # 引数
    ///
    /// * `rdr` - 書き換え設定ファイルのリーダー
    ///
    /// # 戻り値
    ///
    /// (unigram書き換え器, left書き換え器, right書き換え器) のタプル
    ///
    /// # エラー
    ///
    /// ファイル形式が不正な場合、[`VibratoError`] が返されます。
    fn parse_rewrite_config<R>(
        rdr: R,
    ) -> Result<(FeatureRewriter, FeatureRewriter, FeatureRewriter)>
    where
        R: Read,
    {
        let reader = BufReader::new(rdr);

        let mut unigram_rewriter_builder = FeatureRewriterBuilder::new();
        let mut left_rewriter_builder = FeatureRewriterBuilder::new();
        let mut right_rewriter_builder = FeatureRewriterBuilder::new();

        let mut builder = None;
        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            match line {
                "[unigram rewrite]" => builder = Some(&mut unigram_rewriter_builder),
                "[left rewrite]" => builder = Some(&mut left_rewriter_builder),
                "[right rewrite]" => builder = Some(&mut right_rewriter_builder),
                line => {
                    if let Some(builder) = builder.as_mut() {
                        let (pattern, rewrite) = Self::parse_rewrite_rule(line)?;
                        builder.add_rule(&pattern, &rewrite);
                    } else {
                        return Err(VibratoError::invalid_format(
                            "rewrite.def",
                            "Invalid rewrite rule",
                        ));
                    }
                }
            }
        }

        Ok((
            FeatureRewriter::from(unigram_rewriter_builder),
            FeatureRewriter::from(left_rewriter_builder),
            FeatureRewriter::from(right_rewriter_builder),
        ))
    }

    /// リーダーから学習設定を読み込みます。
    ///
    /// # 引数
    ///
    /// * `lexicon_rdr` - 辞書ファイル `lex.csv` のリーダー
    /// * `char_prop_rdr` - 文字定義ファイル `char.def` のリーダー
    /// * `unk_handler_rdr` - 未知語ハンドラファイル `unk.def` のリーダー
    /// * `feature_templates_rdr` - 素性定義ファイル `feature.def` のリーダー
    /// * `rewrite_rules_rdr` - 書き換え定義ファイル `rewrite.def` のリーダー
    ///
    /// # 戻り値
    ///
    /// 学習設定
    ///
    /// # エラー
    ///
    /// 入力形式が不正な場合、[`VibratoError`] が返されます。
    pub fn from_readers<L, C, U, F, R>(
        mut lexicon_rdr: L,
        char_prop_rdr: C,
        unk_handler_rdr: U,
        feature_templates_rdr: F,
        rewrite_rules_rdr: R,
    ) -> Result<Self>
    where
        L: Read,
        C: Read,
        U: Read,
        F: Read,
        R: Read,
    {
        let feature_extractor = Self::parse_feature_config(feature_templates_rdr)?;
        let (unigram_rewriter, left_rewriter, right_rewriter) =
            Self::parse_rewrite_config(rewrite_rules_rdr)?;

        let mut lexicon_data = vec![];
        lexicon_rdr.read_to_end(&mut lexicon_data)?;
        let lex_entries = Lexicon::parse_csv(&lexicon_data, "lex.csv")?;
        let connector = MatrixConnector::from_reader(b"1 1\n0 0 0".as_slice())?;
        let char_prop = CharProperty::from_reader(char_prop_rdr)?;
        let unk_handler = UnkHandler::from_reader(unk_handler_rdr, &char_prop)?;

        let dict = SystemDictionaryBuilder::build(
            &lex_entries,
            ConnectorWrapper::Matrix(connector),
            char_prop,
            unk_handler,
        )?;

        let surfaces = lex_entries.into_iter().map(|e| e.surface).collect();

        Ok(Self {
            feature_extractor,
            unigram_rewriter,
            left_rewriter,
            right_rewriter,
            dict,
            surfaces,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::num::NonZeroU32;

    #[test]
    fn test_parse_feature_config() {
        let config = "
            # feature 1
            UNIGRAM uni:%F[0]
            BIGRAM bi:%L[0]/%R[1]

            # feature 2
            UNIGRAM uni:%F[0]/%t
            BIGRAM bi:%L[0],%L[1]/%R[1],%R[0]
        ";
        let mut feature_extractor = TrainerConfig::parse_feature_config(config.as_bytes()).unwrap();

        // unigram features
        assert_eq!(
            vec![NonZeroU32::new(1).unwrap(), NonZeroU32::new(2).unwrap()],
            feature_extractor.extract_unigram_feature_ids(&["a", "b"], 2)
        );
        assert_eq!(
            vec![NonZeroU32::new(3).unwrap(), NonZeroU32::new(4).unwrap()],
            feature_extractor.extract_unigram_feature_ids(&["b", "c"], 2)
        );
        assert_eq!(
            vec![NonZeroU32::new(1).unwrap(), NonZeroU32::new(2).unwrap()],
            feature_extractor.extract_unigram_feature_ids(&["a", "c"], 2)
        );
        assert_eq!(
            vec![NonZeroU32::new(3).unwrap(), NonZeroU32::new(5).unwrap()],
            feature_extractor.extract_unigram_feature_ids(&["b", "c"], 3)
        );

        // left features
        assert_eq!(
            vec![NonZeroU32::new(1), NonZeroU32::new(2)],
            feature_extractor.extract_left_feature_ids(&["a", "b"])
        );
        assert_eq!(
            vec![NonZeroU32::new(3), NonZeroU32::new(4)],
            feature_extractor.extract_left_feature_ids(&["b", "c"])
        );
        assert_eq!(
            vec![NonZeroU32::new(1), NonZeroU32::new(5)],
            feature_extractor.extract_left_feature_ids(&["a", "c"])
        );
        assert_eq!(
            vec![NonZeroU32::new(3), NonZeroU32::new(4)],
            feature_extractor.extract_left_feature_ids(&["b", "c"])
        );

        // right features
        assert_eq!(
            vec![NonZeroU32::new(1), NonZeroU32::new(2)],
            feature_extractor.extract_right_feature_ids(&["a", "b"])
        );
        assert_eq!(
            vec![NonZeroU32::new(3), NonZeroU32::new(4)],
            feature_extractor.extract_right_feature_ids(&["b", "c"])
        );
        assert_eq!(
            vec![NonZeroU32::new(3), NonZeroU32::new(5)],
            feature_extractor.extract_right_feature_ids(&["a", "c"])
        );
        assert_eq!(
            vec![NonZeroU32::new(3), NonZeroU32::new(4)],
            feature_extractor.extract_right_feature_ids(&["b", "c"])
        );
    }

    #[test]
    fn test_parse_rewrite_config() {
        let config = "
            # unigram feature
            [unigram rewrite]
            a,*,*  $1,$2,$3
            *,*,*  $1,$3,$2

            # left feature
            [left rewrite]
            a,*,*  $2,$1,$3
            *,*,*  $2,$3,$1

            # right feature
            [right rewrite]
            a,*,*  $3,$1,$2
            *,*,*  $3,$2,$1
        ";
        let (unigram_rewriter, left_rewriter, right_rewriter) =
            TrainerConfig::parse_rewrite_config(config.as_bytes()).unwrap();

        // unigram features
        assert_eq!(
            vec!["a", "b", "c"],
            unigram_rewriter.rewrite(&["a", "b", "c"]).unwrap()
        );
        assert_eq!(
            vec!["x", "c", "b"],
            unigram_rewriter.rewrite(&["x", "b", "c"]).unwrap()
        );

        // left features
        assert_eq!(
            vec!["b", "a", "c"],
            left_rewriter.rewrite(&["a", "b", "c"]).unwrap()
        );
        assert_eq!(
            vec!["b", "c", "x"],
            left_rewriter.rewrite(&["x", "b", "c"]).unwrap()
        );

        // right features
        assert_eq!(
            vec!["c", "a", "b"],
            right_rewriter.rewrite(&["a", "b", "c"]).unwrap()
        );
        assert_eq!(
            vec!["c", "b", "x"],
            right_rewriter.rewrite(&["x", "b", "c"]).unwrap()
        );
    }
}
