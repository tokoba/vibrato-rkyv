//! MeCab形式のファイル処理を行うためのユーティリティモジュール
//!
//! このモジュールは、MeCabモデルとの互換性を提供するための機能を含んでいます。
//! MeCab形式のファイル(feature.def、left-id.def、right-id.def、model.def等)の
//! 読み込みと書き込みを行い、バイグラム情報の生成などをサポートします。

use std::io::{BufRead, BufReader, BufWriter, Read, Write};

use hashbrown::HashMap;
use regex::Regex;

use crate::errors::{Result, VibratoError};
use crate::trainer::TrainerConfig;
use crate::utils;

/// MeCabモデルからバイグラム素性情報を生成します。
///
/// この関数は、既存のMeCabモデルから小さな辞書を作成するのに便利です。
/// MeCab形式の各種定義ファイルを読み込み、バイグラム情報を抽出して
/// 新しいファイル形式に変換します。
///
/// # 引数
///
/// * `feature_def_rdr` - 素性定義ファイル `feature.def` のリーダー
/// * `left_id_def_rdr` - 左IDと素性のマッピングファイル `left-id.def` のリーダー
/// * `right_id_def_rdr` - 右IDと素性のマッピングファイル `right-id.def` のリーダー
/// * `model_def_rdr` - モデルファイル `model.def` のリーダー
/// * `cost_factor` - コストを整数にキャストする際に乗算される係数
/// * `bigram_left_wtr` - バイグラム左側情報ファイル `bi-gram.left` のライター
/// * `bigram_right_wtr` - バイグラム右側情報ファイル `bi-gram.right` のライター
/// * `bigram_cost_wtr` - バイグラムコストファイル `bi-gram.cost` のライター
///
/// # 戻り値
///
/// 正常に処理が完了した場合は `Ok(())` を返します。
///
/// # エラー
///
/// 変換に失敗した場合は [`VibratoError`] が返されます。
/// 以下の場合にエラーが発生する可能性があります:
///
/// * 入力ファイルの形式が不正な場合
/// * ID 0 が BOS/EOS でない場合
/// * 素性IDが未定義の場合
/// * ファイルの読み書き中にI/Oエラーが発生した場合
#[allow(clippy::too_many_arguments)]
pub fn generate_bigram_info(
    feature_def_rdr: impl Read,
    right_id_def_rdr: impl Read,
    left_id_def_rdr: impl Read,
    model_def_rdr: impl Read,
    cost_factor: f64,
    bigram_right_wtr: impl Write,
    bigram_left_wtr: impl Write,
    bigram_cost_wtr: impl Write,
) -> Result<()> {
    let mut left_features = HashMap::new();
    let mut right_features = HashMap::new();

    let mut feature_extractor = TrainerConfig::parse_feature_config(feature_def_rdr)?;

    let id_feature_re = Regex::new(r"^([0-9]+) (.*)$").unwrap();
    let model_re = Regex::new(r"^([0-9\-\.]+)\t(.*)$").unwrap();

    // right-id.def contains the right hand ID of the left context, and left-id.def contains the
    // left hand ID of the right context. The left-right naming in this code is based on position
    // between words, so these names are the opposite of left-id.def and right-id.def files.

    // left features
    let right_id_def_rdr = BufReader::new(right_id_def_rdr);
    for line in right_id_def_rdr.lines() {
        let line = line?;
        if let Some(cap) = id_feature_re.captures(&line) {
            let id = cap.get(1).unwrap().as_str().parse::<usize>()?;
            let feature_str = cap.get(2).unwrap().as_str();
            let feature_spl = utils::parse_csv_row(feature_str);
            if id == 0 && feature_spl.first().is_some_and(|s| s != "BOS/EOS") {
                return Err(VibratoError::invalid_format(
                    "right_id_def_rdr",
                    "ID 0 must be BOS/EOS",
                ));
            }
            let feature_ids = feature_extractor.extract_left_feature_ids(&feature_spl);
            left_features.insert(id, feature_ids);
        } else {
            return Err(VibratoError::invalid_format(
                "right_id_def_rdr",
                "each line must be a pair of an ID and features",
            ));
        }
    }
    // right features
    let left_id_def_rdr = BufReader::new(left_id_def_rdr);
    for line in left_id_def_rdr.lines() {
        let line = line?;
        if let Some(cap) = id_feature_re.captures(&line) {
            let id = cap.get(1).unwrap().as_str().parse::<usize>()?;
            let feature_str = cap.get(2).unwrap().as_str();
            let feature_spl = utils::parse_csv_row(feature_str);
            if id == 0 && feature_spl.first().is_some_and(|s| s != "BOS/EOS") {
                return Err(VibratoError::invalid_format(
                    "left_id_def_rdr",
                    "ID 0 must be BOS/EOS",
                ));
            }
            let feature_ids = feature_extractor.extract_right_feature_ids(&feature_spl);
            right_features.insert(id, feature_ids);
        } else {
            return Err(VibratoError::invalid_format(
                "left_id_def_rdr",
                "each line must be a pair of an ID and features",
            ));
        }
    }
    // weights
    let model_def_rdr = BufReader::new(model_def_rdr);
    let mut bigram_cost_wtr = BufWriter::new(bigram_cost_wtr);
    for line in model_def_rdr.lines() {
        let line = line?;
        if let Some(cap) = model_re.captures(&line) {
            let weight = cap.get(1).unwrap().as_str().parse::<f64>()?;
            let cost = -(weight * cost_factor) as i32;
            if cost == 0 {
                continue;
            }
            let feature_str = cap.get(2).unwrap().as_str().replace("BOS/EOS", "");
            let mut spl = feature_str.split('/');
            let left_feat_str = spl.next();
            let right_feat_str = spl.next();
            if let (Some(left_feat_str), Some(right_feat_str)) = (left_feat_str, right_feat_str) {
                let left_id = if left_feat_str.is_empty() {
                    String::new()
                } else if let Some(id) = feature_extractor.left_feature_ids().get(left_feat_str) {
                    id.to_string()
                } else {
                    continue;
                };
                let right_id = if right_feat_str.is_empty() {
                    String::new()
                } else if let Some(id) = feature_extractor.right_feature_ids().get(right_feat_str) {
                    id.to_string()
                } else {
                    continue;
                };
                writeln!(&mut bigram_cost_wtr, "{left_id}/{right_id}\t{cost}")?;
            }
        }
    }

    let mut bigram_right_wtr = BufWriter::new(bigram_right_wtr);
    for id in 1..left_features.len() {
        write!(&mut bigram_right_wtr, "{id}\t")?;
        if let Some(features) = left_features.get(&id) {
            for (i, feat_id) in features.iter().enumerate() {
                if i != 0 {
                    write!(&mut bigram_right_wtr, ",")?;
                }
                if let Some(feat_id) = feat_id {
                    write!(&mut bigram_right_wtr, "{}", feat_id.get())?;
                } else {
                    write!(&mut bigram_right_wtr, "*")?;
                }
            }
        } else {
            return Err(VibratoError::invalid_format(
                "right_id_def_rdr",
                format!("feature ID {id} is undefined"),
            ));
        }
        writeln!(&mut bigram_right_wtr)?;
    }

    let mut bigram_left_wtr = BufWriter::new(bigram_left_wtr);
    for id in 1..right_features.len() {
        write!(&mut bigram_left_wtr, "{id}\t")?;
        if let Some(features) = right_features.get(&id) {
            for (i, feat_id) in features.iter().enumerate() {
                if i != 0 {
                    write!(&mut bigram_left_wtr, ",")?;
                }
                if let Some(feat_id) = feat_id {
                    write!(&mut bigram_left_wtr, "{}", feat_id.get())?;
                } else {
                    write!(&mut bigram_left_wtr, "*")?;
                }
            }
            writeln!(&mut bigram_left_wtr)?;
        } else {
            return Err(VibratoError::invalid_format(
                "left_id_def_rdr",
                format!("feature ID {id} is undefined"),
            ));
        }
    }

    Ok(())
}
