//! ユーティリティ関数と型変換トレイトを提供するモジュール
//!
//! このモジュールには、CSV処理、型変換、その他のヘルパー関数が含まれています。
//! 主に以下の機能を提供します：
//!
//! - `FromU32`: u32からの型変換トレイト
//! - CSV行の解析と引用符処理
//! - テスト用のマクロ

#[cfg(feature = "train")]
use std::io::Write;

use csv_core::ReadFieldResult;

/// u32から他の型への変換を提供するトレイト
///
/// このトレイトは、u32値を実装型に変換する機能を定義します。
/// 標準ライブラリのFromトレイトとは異なり、特定の最適化や
/// プラットフォーム固有の仮定を行うことができます。
pub trait FromU32 {
    /// u32値から実装型を生成する
    ///
    /// # 引数
    ///
    /// * `src` - 変換元のu32値
    ///
    /// # 戻り値
    ///
    /// 変換された実装型の値
    fn from_u32(src: u32) -> Self;
}

#[cfg(any(target_pointer_width = "32", target_pointer_width = "64"))]
impl FromU32 for usize {
    /// u32値をusizeに変換する
    ///
    /// ポインタ幅が32ビットまたは64ビットであることが保証されているため、
    /// この変換は常に成功します。安全でないコードを使用して最適化を行います。
    ///
    /// # 引数
    ///
    /// * `src` - 変換元のu32値
    ///
    /// # 戻り値
    ///
    /// 変換されたusize値
    #[inline(always)]
    fn from_u32(src: u32) -> Self {
        // Since the pointer width is guaranteed to be 32 or 64,
        // the following process always succeeds.
        unsafe { Self::try_from(src).unwrap_unchecked() }
    }
}

#[cfg(feature = "train")]
/// CSVセルのデータを適切に引用符で囲んで書き出す
///
/// この関数は、バイト列をCSV形式のセルとして書き出します。
/// 必要に応じてダブルクォートやエスケープ処理を自動的に行います。
///
/// # 引数
///
/// * `wtr` - 書き込み先のWriterオブジェクト
/// * `data` - CSVセルとして書き込むバイト列
///
/// # 戻り値
///
/// * `Ok(())` - 書き込みに成功した場合
/// * `Err(std::io::Error)` - 書き込み中にI/Oエラーが発生した場合
///
/// # 機能ゲート
///
/// この関数は`train`フィーチャーが有効な場合のみ利用可能です。
pub fn quote_csv_cell<W>(mut wtr: W, mut data: &[u8]) -> std::io::Result<()>
where
    W: Write,
{
    let mut output = [0; 4096];
    let mut writer = csv_core::Writer::new();
    loop {
        let (result, nin, nout) = writer.field(data, &mut output);
        wtr.write_all(&output[..nout])?;
        if result == csv_core::WriteResult::InputEmpty {
            break;
        }
        data = &data[nin..];
    }
    let (result, nout) = writer.finish(&mut output);
    assert_eq!(result, csv_core::WriteResult::InputEmpty);
    wtr.write_all(&output[..nout])?;
    Ok(())
}

/// CSV形式の行を解析してフィールドのベクターに分割する
///
/// この関数は、CSV形式の文字列を解析し、各フィールドを個別の文字列として抽出します。
/// ダブルクォートで囲まれたフィールドや、フィールド内のカンマも正しく処理します。
///
/// # 引数
///
/// * `row` - 解析するCSV形式の文字列
///
/// # 戻り値
///
/// 解析されたフィールドを格納する文字列のベクター
///
/// # 例
///
/// ```
/// # use vibrato_rkyv::utils::parse_csv_row;
/// let fields = parse_csv_row("名詞,トスカーナ");
/// assert_eq!(fields, vec!["名詞", "トスカーナ"]);
///
/// let fields_with_quote = parse_csv_row("名詞,\"1,2-ジクロロエタン\"");
/// assert_eq!(fields_with_quote, vec!["名詞", "1,2-ジクロロエタン"]);
/// ```
pub fn parse_csv_row(row: &str) -> Vec<String> {
    let mut features = vec![];
    let mut rdr = csv_core::Reader::new();
    let mut bytes = row.as_bytes();
    let mut output = [0; 4096];
    loop {
        let (result, nin, nout) = rdr.read_field(bytes, &mut output);
        let end = match result {
            ReadFieldResult::InputEmpty => true,
            ReadFieldResult::Field { .. } => false,
            ReadFieldResult::End => true,
            _ => unreachable!(),
        };
        features.push(std::str::from_utf8(&output[..nout]).unwrap().to_string());
        if end {
            break;
        }
        bytes = &bytes[nin..];
    }
    features
}

#[cfg(test)]
/// HashMapリテラルを簡潔に記述するためのマクロ
///
/// このマクロは、ハッシュマップの初期化を簡潔に記述できるようにします。
/// キーと値のペアを`=>`演算子で指定し、カンマで区切って記述します。
///
/// # 例
///
/// ```ignore
/// let map = hashmap! {
///     "key1" => "value1",
///     "key2" => "value2",
/// };
/// ```
///
/// # 注意
///
/// このマクロはテスト時のみ利用可能です。
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

#[cfg(test)]
pub(crate) use hashmap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_csv_row() {
        assert_eq!(
            &["名詞", "トスカーナ"],
            parse_csv_row("名詞,トスカーナ").as_slice()
        );
    }

    #[test]
    fn test_parse_csv_row_with_quote() {
        assert_eq!(
            &["名詞", "1,2-ジクロロエタン"],
            parse_csv_row("名詞,\"1,2-ジクロロエタン\"").as_slice()
        );
    }
}
