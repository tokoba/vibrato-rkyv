//! 構造化パーセプトロンによるモデル学習のためのモジュール。
//!
//! このモジュールは、形態素解析器の学習に必要な機能を提供します。
//! 構造化パーセプトロンアルゴリズムを使用して、教師データから単語の素性や接続コストを学習します。
//!
//! # 概要
//!
//! - 学習設定の読み込みと構成
//! - コーパスからの訓練データ抽出
//! - 構造化パーセプトロンによる学習
//! - 学習済みモデルの辞書形式での出力
//!
//! # 使用例
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use std::fs::File;
//! use vibrato_rkyv::trainer::{Corpus, Trainer, TrainerConfig};
//! use vibrato_rkyv::{Dictionary, SystemDictionaryBuilder, Tokenizer};
//!
//! // 設定ファイルの読み込み
//! let lexicon_rdr = File::open("src/tests/resources/train_lex.csv")?;
//! let char_prop_rdr = File::open("src/tests/resources/char.def")?;
//! let unk_handler_rdr = File::open("src/tests/resources/train_unk.def")?;
//! let feature_templates_rdr = File::open("src/tests/resources/feature.def")?;
//! let rewrite_rules_rdr = File::open("src/tests/resources/rewrite.def")?;
//! let config = TrainerConfig::from_readers(
//!     lexicon_rdr,
//!     char_prop_rdr,
//!     unk_handler_rdr,
//!     feature_templates_rdr,
//!     rewrite_rules_rdr,
//! )?;
//!
//! // トレーナーの初期化
//! let trainer = Trainer::new(config)?
//!     .regularization_cost(0.01)
//!     .max_iter(300)
//!     .num_threads(20);
//!
//! // コーパスの読み込み
//! let corpus_rdr = File::open("src/tests/resources/corpus.txt")?;
//! let corpus = Corpus::from_reader(corpus_rdr)?;
//!
//! // モデルデータの出力先
//! let mut lexicon_trained = vec![];
//! let mut connector_trained = vec![];
//! let mut unk_handler_trained = vec![];
//! let mut user_lexicon_trained = vec![];
//!
//! // 学習の開始
//! let mut model = trainer.train(corpus)?;
//!
//! model.write_dictionary(
//!     &mut lexicon_trained,
//!     &mut connector_trained,
//!     &mut unk_handler_trained,
//!     &mut user_lexicon_trained,
//! )?;
//!
//! // 学習済みモデルの読み込みとトークナイザーの作成
//! let char_prop_rdr_again = File::open("src/tests/resources/char.def")?;
//! let dict = SystemDictionaryBuilder::from_readers(
//!     &*lexicon_trained,
//!     &*connector_trained,
//!     char_prop_rdr_again,
//!     &*unk_handler_trained,
//! )?;
//!
//! let tokenizer = Tokenizer::from_inner(dict);
//! let mut worker = tokenizer.new_worker();
//!
//! worker.reset_sentence("外国人参政権");
//! worker.tokenize();
//! assert_eq!(worker.num_tokens(), 4); // 外国/人/参政/権
//! # Ok(())
//! # }
//! ```

mod config;
mod corpus;
mod feature_extractor;
mod feature_rewriter;
mod model;

use std::num::NonZeroU32;

use hashbrown::{HashMap, HashSet};
use rucrf_rkyv::{Edge, FeatureProvider, FeatureSet, Lattice};

use crate::dictionary::word_idx::WordIdx;
use crate::dictionary::LexType;
use crate::errors::Result;
pub use crate::trainer::config::TrainerConfig;
pub use crate::trainer::corpus::{Corpus, Example, Word};
use crate::trainer::feature_extractor::FeatureExtractor;
use crate::trainer::feature_rewriter::FeatureRewriter;
pub use crate::trainer::model::Model;
use crate::trainer::model::ModelData;
use crate::utils::{self, FromU32};

/// 形態素解析器のトレーナー。
///
/// 構造化パーセプトロンアルゴリズムを使用して、コーパスから形態素解析モデルを学習します。
/// 学習では、単語の素性と接続コストを最適化し、正しい形態素分割を実現します。
pub struct Trainer {
    config: TrainerConfig,
    max_grouping_len: Option<usize>,
    provider: FeatureProvider,

    // Assume a dictionary word W is associated with id X and feature string F.
    // It maps F to a hash table that maps the first character of W to X.
    label_id_map: HashMap<String, HashMap<char, NonZeroU32>>,

    label_id_map_unk: Vec<NonZeroU32>,
    regularization_cost: f64,
    max_iter: u64,
    num_threads: usize,
}

impl Trainer {
    /// 素性セットを抽出します。
    ///
    /// 指定された素性文字列から、unigram、left、rightの各素性を抽出し、
    /// 必要に応じて書き換えルールを適用します。
    ///
    /// # 引数
    ///
    /// * `feature_extractor` - 素性抽出器
    /// * `unigram_rewriter` - unigram素性の書き換え器
    /// * `left_rewriter` - left素性の書き換え器
    /// * `right_rewriter` - right素性の書き換え器
    /// * `feature_str` - 素性文字列
    /// * `cate_id` - カテゴリID
    ///
    /// # 戻り値
    ///
    /// 抽出された素性セット
    fn extract_feature_set(
        feature_extractor: &mut FeatureExtractor,
        unigram_rewriter: &FeatureRewriter,
        left_rewriter: &FeatureRewriter,
        right_rewriter: &FeatureRewriter,
        feature_str: &str,
        cate_id: u32,
    ) -> FeatureSet {
        let features = utils::parse_csv_row(feature_str);
        let unigram_features = if let Some(rewrite) = unigram_rewriter.rewrite(&features) {
            feature_extractor.extract_unigram_feature_ids(&rewrite, cate_id)
        } else {
            feature_extractor.extract_unigram_feature_ids(&features, cate_id)
        };
        let left_features = if let Some(rewrite) = left_rewriter.rewrite(&features) {
            feature_extractor.extract_left_feature_ids(&rewrite)
        } else {
            feature_extractor.extract_left_feature_ids(&features)
        };
        let right_features = if let Some(rewrite) = right_rewriter.rewrite(&features) {
            feature_extractor.extract_right_feature_ids(&rewrite)
        } else {
            feature_extractor.extract_right_feature_ids(&features)
        };
        FeatureSet::new(&unigram_features, &right_features, &left_features)
    }

    /// 指定された設定を使用して新しい [`Trainer`] を作成します。
    ///
    /// 辞書内の全単語と未知語に対して素性セットを抽出し、ラベルIDを割り当てます。
    ///
    /// # 引数
    ///
    ///  * `config` - 学習設定
    ///
    /// # 戻り値
    ///
    /// 初期化されたトレーナー
    ///
    /// # エラー
    ///
    /// モデルが大きくなりすぎる場合、[`VibratoError`](crate::errors::VibratoError) が返されます。
    pub fn new(mut config: TrainerConfig) -> Result<Self> {
        let mut provider = FeatureProvider::default();
        let mut label_id_map = HashMap::new();
        let mut label_id_map_unk = vec![];

        for word_id in 0..u32::try_from(config.surfaces.len()).unwrap() {
            let word_idx = WordIdx::new(LexType::System, word_id);
            let feature_str = config.dict.system_lexicon().word_feature(word_idx);
            let first_char = config.surfaces[usize::from_u32(word_id)]
                .chars()
                .next()
                .unwrap();
            let cate_id = config.dict.char_prop().char_info(first_char).base_id();
            let feature_set = Self::extract_feature_set(
                &mut config.feature_extractor,
                &config.unigram_rewriter,
                &config.left_rewriter,
                &config.right_rewriter,
                feature_str,
                cate_id,
            );
            let label_id = provider.add_feature_set(feature_set)?;
            label_id_map
                .raw_entry_mut()
                .from_key(feature_str)
                .or_insert_with(|| (feature_str.to_string(), HashMap::new()))
                .1
                .insert(first_char, label_id);
        }
        for word_id in 0..u32::try_from(config.dict.unk_handler().len()).unwrap() {
            let word_idx = WordIdx::new(LexType::Unknown, word_id);
            let feature_str = config.dict.unk_handler().word_feature(word_idx);
            let cate_id = u32::from(config.dict.unk_handler().word_cate_id(word_idx));
            let feature_set = Self::extract_feature_set(
                &mut config.feature_extractor,
                &config.unigram_rewriter,
                &config.left_rewriter,
                &config.right_rewriter,
                feature_str,
                cate_id,
            );
            label_id_map_unk.push(provider.add_feature_set(feature_set)?);
        }

        Ok(Self {
            config,
            max_grouping_len: None,
            provider,
            label_id_map,
            label_id_map_unk,
            regularization_cost: 0.01,
            max_iter: 100,
            num_threads: 1,
        })
    }

    /// L1正則化のコストを変更します。
    ///
    /// この値が大きいほど、正則化が強くなります。
    /// デフォルト値は 0.01 です。
    ///
    /// # 引数
    ///
    /// * `cost` - 正則化コスト（0以上の値）
    ///
    /// # 戻り値
    ///
    /// 設定が更新されたトレーナー
    ///
    /// # パニック
    ///
    /// 値が0未満の場合、パニックします。
    pub fn regularization_cost(mut self, cost: f64) -> Self {
        assert!(cost >= 0.0);
        self.regularization_cost = cost;
        self
    }

    /// 最大反復回数を変更します。
    ///
    /// デフォルト値は 100 です。
    ///
    /// # 引数
    ///
    /// * `n` - 最大反復回数（1以上の値）
    ///
    /// # 戻り値
    ///
    /// 設定が更新されたトレーナー
    ///
    /// # パニック
    ///
    /// 値が1未満の場合、パニックします。
    pub fn max_iter(mut self, n: u64) -> Self {
        assert!(n >= 1);
        self.max_iter = n;
        self
    }

    /// マルチスレッドを有効化します。
    ///
    /// デフォルト値は 1（シングルスレッド）です。
    ///
    /// # 引数
    ///
    /// * `n` - スレッド数（1以上の値）
    ///
    /// # 戻り値
    ///
    /// 設定が更新されたトレーナー
    ///
    /// # パニック
    ///
    /// 値が1未満の場合、パニックします。
    pub fn num_threads(mut self, n: usize) -> Self {
        assert!(n >= 1);
        self.num_threads = n;
        self
    }

    /// 未知語の最大グルーピング長を指定します。
    ///
    /// デフォルトでは、長さは無制限です。
    ///
    /// このオプションは MeCab との互換性のためのものです。
    /// MeCab と同じ結果を得たい場合は、引数に `24` を指定してください。
    ///
    /// # 引数
    ///
    ///  * `max_grouping_len` - 未知語の最大グルーピング長。
    ///    デフォルト値は 0 で、無制限を示します。
    ///
    /// # 戻り値
    ///
    /// 設定が更新されたトレーナー
    pub const fn max_grouping_len(mut self, max_grouping_len: usize) -> Self {
        if max_grouping_len != 0 {
            self.max_grouping_len = Some(max_grouping_len);
        } else {
            self.max_grouping_len = None;
        }
        self
    }

    /// 訓練例からラティスを構築します。
    ///
    /// 正解パスのエッジ（正例）と辞書に含まれる全ての候補エッジ（負例）を追加します。
    ///
    /// # 引数
    ///
    /// * `example` - 訓練例
    ///
    /// # 戻り値
    ///
    /// 構築されたラティス
    ///
    /// # エラー
    ///
    /// ラティスの構築に失敗した場合、[`VibratoError`](crate::errors::VibratoError) が返されます。
    fn build_lattice(&mut self, example: &Example) -> Result<Lattice> {
        let Example { sentence, tokens } = example;

        let input_chars = sentence.chars();
        let input_len = sentence.len_char();

        // Add positive edges
        // 1. If the word is found in the dictionary, add the edge as it is.
        // 2. If the word is not found in the dictionary:
        //   a) If a compatible unknown word is found, add the unknown word edge instead.
        //   b) If there is no available word, add a virtual edge, which does not have any features.
        let mut edges = vec![];
        let mut pos = 0;
        for token in tokens {
            let len = token.surface().chars().count();
            let first_char = input_chars[pos];
            let label_id = self
                .label_id_map
                .get(token.feature())
                .and_then(|hm| hm.get(&first_char))
                .cloned()
                .map(Ok)
                .unwrap_or_else(|| {
                    self.config
                        .dict
                        .unk_handler()
                        .compatible_unk_index(sentence, pos, pos + len, token.feature())
                        .map_or_else(
                            || {
                                eprintln!(
                                    "adding virtual edge: {} {}",
                                    token.surface(),
                                    token.feature()
                                );
                                self.provider
                                    .add_feature_set(FeatureSet::new(&[], &[], &[]))
                            },
                            |unk_index| {
                                Ok(self.label_id_map_unk[usize::from_u32(unk_index.word_id)])
                            },
                        )
                })?;
            edges.push((pos, Edge::new(pos + len, label_id)));
            pos += len;
        }
        assert_eq!(pos, input_len);

        let mut lattice = Lattice::new(input_len).unwrap();

        for (pos, edge) in edges {
            lattice.add_edge(pos, edge).unwrap();
        }

        // Add negative edges
        for start_word in 0..input_len {
            let mut has_matched = false;

            let suffix = &input_chars[start_word..];

            for m in self
                .config
                .dict
                .system_lexicon()
                .common_prefix_iterator(suffix)
            {
                has_matched = true;
                let label_id = NonZeroU32::new(m.word_idx.word_id + 1).unwrap();
                let pos = start_word;
                let target = pos + m.end_char;
                let edge = Edge::new(target, label_id);
                // Skips adding if the edge is already added as a positive edge.
                if let Some(first_edge) = lattice.nodes()[pos].edges().first()
                    && edge == *first_edge {
                        continue;
                    }
                lattice.add_edge(pos, edge).unwrap();
            }

            self.config.dict.unk_handler().gen_unk_words(
                sentence,
                start_word,
                has_matched,
                self.max_grouping_len,
                |w| {
                    let id_offset = u32::try_from(self.config.surfaces.len()).unwrap();
                    let label_id = NonZeroU32::new(id_offset + w.word_idx().word_id + 1).unwrap();
                    let pos = start_word;
                    let target = w.end_char();
                    let edge = Edge::new(target, label_id);
                    // Skips adding if the edge is already added as a positive edge.
                    if let Some(first_edge) = lattice.nodes()[pos].edges().first()
                        && edge == *first_edge {
                            return;
                        }
                    lattice.add_edge(pos, edge).unwrap();
                },
            );
        }

        Ok(lattice)
    }

    /// 学習を開始し、モデルを返します。
    ///
    /// コーパス内の各例文からラティスを構築し、構造化パーセプトロンによって
    /// 素性の重みを学習します。学習後、未使用の素性を削除してモデルを最適化します。
    ///
    /// # 引数
    ///
    /// * `corpus` - 学習に使用するコーパス
    ///
    /// # 戻り値
    ///
    /// 学習済みモデル
    ///
    /// # エラー
    ///
    /// 文のコンパイルやラティスの構築に失敗した場合、
    /// [`VibratoError`](crate::errors::VibratoError) が返されます。
    pub fn train(mut self, mut corpus: Corpus) -> Result<Model> {
        let mut lattices = vec![];
        for example in &mut corpus.examples {
            example.sentence.compile(self.config.dict.char_prop());
            lattices.push(self.build_lattice(example)?);
        }

        let trainer = rucrf_rkyv::Trainer::new()
            .regularization(rucrf_rkyv::Regularization::L1, self.regularization_cost)
            .unwrap()
            .max_iter(self.max_iter)
            .unwrap()
            .n_threads(self.num_threads)
            .unwrap();
        let model = trainer.train(&lattices, self.provider);

        // Remove unused feature strings
        let mut used_right_features = HashSet::new();
        let unigram_feature_keys: Vec<_> = self
            .config
            .feature_extractor
            .unigram_feature_ids
            .keys()
            .cloned()
            .collect();
        let left_feature_keys: Vec<_> = self
            .config
            .feature_extractor
            .left_feature_ids
            .keys()
            .cloned()
            .collect();
        let right_feature_keys: Vec<_> = self
            .config
            .feature_extractor
            .right_feature_ids
            .keys()
            .cloned()
            .collect();
        for k in &unigram_feature_keys {
            let id = self
                .config
                .feature_extractor
                .unigram_feature_ids
                .get(k)
                .unwrap();
            if model
                .unigram_weight_indices()
                .get(usize::from_u32(id.get() - 1))
                .cloned()
                .flatten()
                .is_none()
            {
                self.config.feature_extractor.unigram_feature_ids.remove(k);
            }
        }
        for feature_ids in model.bigram_weight_indices() {
            for (feature_id, _) in feature_ids {
                used_right_features.insert(*feature_id);
            }
        }
        for k in &left_feature_keys {
            let id = self
                .config
                .feature_extractor
                .left_feature_ids
                .get(k)
                .unwrap();
            if let Some(x) = model.bigram_weight_indices().get(usize::from_u32(id.get()))
                && x.is_empty() {
                    self.config.feature_extractor.left_feature_ids.remove(k);
                }
        }
        for k in &right_feature_keys {
            let id = self
                .config
                .feature_extractor
                .right_feature_ids
                .get(k)
                .unwrap();
            if !used_right_features.contains(&id.get()) {
                self.config.feature_extractor.right_feature_ids.remove(k);
            }
        }

        Ok(Model {
            data: ModelData {
                config: self.config,
                raw_model: model,
            },
            merged_model: None,
            user_entries: vec![],
        })
    }
}
