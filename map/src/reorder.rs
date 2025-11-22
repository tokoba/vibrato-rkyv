//! 並び替えマッピングを生成するユーティリティ
//!
//! このバイナリは、標準入力から読み込んだテキストに基づいて
//! 接続IDの使用頻度を計算し、並び替えマッピングファイル（lmap、rmap）を生成します。

use std::error::Error;
use std::fs::File;
use std::io::{BufRead, BufWriter, Write};
use std::path::PathBuf;

use vibrato_rkyv::dictionary::Dictionary;
use vibrato_rkyv::Tokenizer;

use clap::Parser;

/// コマンドライン引数
#[derive(Parser, Debug)]
#[clap(name = "reorder", about = "A program to produce reordered mapping.")]
struct Args {
    /// System dictionary in binary (in zstd).
    #[clap(short = 'i', long)]
    sysdic_in: PathBuf,

    /// Basename to which the reordered mappings are output.
    /// Two files *.lmap and *.rmap will be output.
    #[clap(short = 'o', long)]
    mapping_out: PathBuf,
}

/// メイン関数
///
/// 辞書をロードし、標準入力から読み込んだテキストをトークン化して
/// 接続IDの使用頻度を計算し、並び替えマッピングファイルを出力します。
///
/// # 戻り値
///
/// 実行が成功した場合は `Ok(())`、エラーが発生した場合はエラー情報
fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    eprintln!("Loading the dictionary...");
    let reader = zstd::Decoder::new(File::open(args.sysdic_in)?)?;
    let dict = Dictionary::read(reader)?;

    eprintln!("Reordering connection id mappings...");
    let tokenizer = Tokenizer::new(dict);
    let mut worker = tokenizer.new_worker();
    worker.init_connid_counter();

    let lines = std::io::stdin().lock().lines();
    for line in lines {
        let line = line?;
        worker.reset_sentence(line);
        worker.tokenize();
        worker.update_connid_counts();
    }
    let (lid_probs, rid_probs) = worker.compute_connid_probs();

    eprintln!("Writing connection id mappings...");
    {
        let mut output_filename = args.mapping_out.clone();
        output_filename.set_extension("lmap");
        let mut w = BufWriter::new(File::create(&output_filename).unwrap());
        for (i, p) in lid_probs {
            w.write_all(format!("{i}\t{p}\n").as_bytes())?;
        }
        println!("Wrote {output_filename:?}");
    }
    {
        let mut output_filename = args.mapping_out;
        output_filename.set_extension("rmap");
        let mut w = BufWriter::new(File::create(&output_filename).unwrap());
        for (i, p) in rid_probs {
            w.write_all(format!("{i}\t{p}\n").as_bytes())?;
        }
        println!("Wrote {output_filename:?}");
    }

    Ok(())
}
