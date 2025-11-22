//! トークン化用辞書
//!
//! このモジュールは、レガシー形式のトークン化に使用される辞書構造を定義します。
//! 辞書は、語彙辞書、接続コスト、文字プロパティ、未知語処理などの
//! コンポーネントから構成されます。
pub(crate) mod character;
pub(crate) mod connector;
pub(crate) mod lexicon;
pub(crate) mod mapper;
pub(crate) mod unknown;

use std::io::Read;

use bincode::{Decode, Encode};

use crate::legacy::common;
use crate::legacy::dictionary::character::CharProperty;
use crate::legacy::dictionary::connector::ConnectorWrapper;
use crate::legacy::dictionary::lexicon::Lexicon;
use crate::legacy::dictionary::mapper::ConnIdMapper;
use crate::legacy::dictionary::unknown::UnkHandler;
use crate::legacy::errors::{Result, VibratoError};


const MODEL_MAGIC: &[u8] = b"VibratoTokenizer 0.5\n";

/// 単語を含む辞書の種類
///
/// この列挙型は、トークン化された単語がどの辞書から取得されたかを示します。
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash, Decode, Encode)]
#[repr(u8)]
#[derive(Default)]
pub enum LexType {
    /// システム辞書
    ///
    /// 基本的な語彙を含むメインの辞書です。
    #[default]
    System,
    /// ユーザー辞書
    ///
    /// ユーザーが追加したカスタム語彙を含む辞書です。
    User,
    /// 未知語
    ///
    /// 辞書に登録されていない単語です。
    Unknown,
}

/// [`Dictionary`]の内部データ
///
/// この構造体は、辞書の実際のデータを保持します。
#[derive(Decode, Encode)]
pub struct DictionaryInner {
    /// システム辞書（語彙辞書）
    pub system_lexicon: Lexicon,
    /// ユーザー辞書（オプション）
    pub user_lexicon: Option<Lexicon>,
    /// 接続コスト計算用のコネクター
    pub connector: ConnectorWrapper,
    /// 接続ID変換用のマッパー（オプション）
    pub mapper: Option<ConnIdMapper>,
    /// 文字プロパティ
    pub char_prop: CharProperty,
    /// 未知語ハンドラー
    pub unk_handler: UnkHandler,
}

/// トークン化用辞書
///
/// この構造体は、形態素解析に必要なすべての辞書データを管理します。
pub struct Dictionary {
    /// 辞書の内部データ
    pub data: DictionaryInner,
}

impl Dictionary {
    /// 接続IDマッパーへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// マッパーが存在する場合は`Some(&ConnIdMapper)`を、存在しない場合は`None`を返します。
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) const fn mapper(&self) -> Option<&ConnIdMapper> {
        self.data.mapper.as_ref()
    }

    /// 生の辞書データから辞書を作成します。
    ///
    /// 引数は、[`Dictionary::write()`]関数によってエクスポートされた
    /// バイトシーケンスでなければなりません。
    ///
    /// # 使用例
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::fs::File;
    ///
    /// use vibrato::Dictionary;
    ///
    /// let reader = File::open("path/to/system.dic")?;
    /// let dict = Dictionary::read(reader)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # 引数
    ///
    /// * `rdr` - 辞書データを読み込むリーダー
    ///
    /// # 戻り値
    ///
    /// 読み込まれた辞書オブジェクト
    ///
    /// # エラー
    ///
    /// bincodeがエラーを生成した場合、そのエラーがそのまま返されます。
    /// また、マジックナンバーが一致しない場合もエラーが返されます。
    pub fn read<R>(rdr: R) -> Result<Self>
    where
        R: Read,
    {
        Ok(Self {
            data: Self::read_common(rdr)?,
        })
    }

    fn read_common<R>(mut rdr: R) -> Result<DictionaryInner>
    where
        R: Read,
    {
        let mut magic = [0; MODEL_MAGIC.len()];
        rdr.read_exact(&mut magic)?;
        if magic != MODEL_MAGIC {
            return Err(VibratoError::invalid_argument(
                "rdr",
                "The magic number of the input model mismatches.",
            ));
        }
        let config = common::bincode_config();
        let data = bincode::decode_from_std_read(&mut rdr, config)?;
        Ok(data)
    }
}
