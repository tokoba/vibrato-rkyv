//! トークン化のための辞書モジュール。
//!
//! このモジュールは、形態素解析に必要な辞書データの読み込み、構築、管理を行います。
//! 主な機能として以下を提供します:
//!
//! - システム辞書とユーザー辞書の読み込み
//! - ゼロコピーデシリアライゼーションによる高速な辞書アクセス
//! - メモリマップドファイルによる効率的なメモリ使用
//! - Zstandard圧縮辞書の透過的な展開とキャッシング
//! - プリセット辞書の自動ダウンロード機能
//!
//! # 辞書の読み込み方法
//!
//! 辞書は複数の方法で読み込むことができます:
//!
//! - [`Dictionary::from_path`]: ファイルパスから辞書を読み込む(推奨)
//! - [`Dictionary::read`]: リーダーから辞書を読み込む
//! - [`Dictionary::from_zstd`]: Zstandard圧縮辞書を読み込む
//! - [`Dictionary::from_preset_with_download`]: プリセット辞書をダウンロードして読み込む
//!
//! # 辞書のビルド
//!
//! [`SystemDictionaryBuilder`]を使用して、CSV形式のソースデータから辞書を構築できます。
pub mod builder;
pub(crate) mod character;
pub(crate) mod config;
pub(crate) mod connector;
pub(crate) mod fetch;
pub(crate) mod lexicon;
pub(crate) mod mapper;
pub(crate) mod unknown;
pub(crate) mod word_idx;

use std::fs::{self, File, Metadata, create_dir_all};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::ops::Deref;

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use memmap2::Mmap;
use rkyv::{Archived, access_unchecked};
use rkyv::rancor::Error;
use rkyv::util::AlignedVec;
use rkyv::{
    access, api::serialize_using, ser::allocator::Arena, ser::sharing::Share,
    ser::writer::IoWriter, ser::Serializer, util::with_arena, Archive, Deserialize,
    Serialize,
};
use sha2::{Digest, Sha256};

use crate::dictionary::character::{ArchivedCharProperty, CharProperty};
use crate::dictionary::connector::{ArchivedConnectorWrapper, Connector, ConnectorWrapper};
use crate::dictionary::lexicon::{ArchivedLexicon, Lexicon};
use crate::dictionary::mapper::ConnIdMapper;
use crate::dictionary::unknown::{ArchivedUnkHandler, UnkHandler};
use crate::errors::{Result, VibratoError};

pub use crate::dictionary::builder::SystemDictionaryBuilder;
pub use crate::dictionary::word_idx::WordIdx;

pub(crate) use crate::dictionary::lexicon::WordParam;

#[cfg(feature = "download")]
pub use crate::dictionary::config::PresetDictionaryKind;

/// Vibratoトークナイザーを識別するマジックバイト。
///
/// この定数の"0.6"というバージョンは、モデルフォーマットのバージョンを示しており、
/// クレートのセマンティックバージョンからは切り離されています。このマジックバイトは
/// 現在変更されることは想定されていません。これは辞書フォーマットの後方互換性を
/// 維持するポリシーに基づいています。
pub const MODEL_MAGIC: &[u8] = b"VibratoTokenizerRkyv 0.6\n";

const MODEL_MAGIC_LEN: usize = MODEL_MAGIC.len();
const RKYV_ALIGNMENT: usize = 16;
const PADDING_LEN: usize = (RKYV_ALIGNMENT - (MODEL_MAGIC_LEN % RKYV_ALIGNMENT)) % RKYV_ALIGNMENT;
const DATA_START: usize = MODEL_MAGIC_LEN + PADDING_LEN;

/// レガシーbincodeベースモデルのマジックバイトプレフィックス。
///
/// 旧バージョンのVibratoで使用されていたbincode形式の辞書ファイルを識別するための
/// プレフィックスです。
pub const LEGACY_MODEL_MAGIC_PREFIX: &[u8] = b"VibratoTokenizer 0.";

/// グローバルキャッシュディレクトリのパス。
///
/// ユーザー固有のシステムキャッシュディレクトリ内の`vibrato-rkyv`サブディレクトリを指します。
/// 各プラットフォームでの標準的なキャッシュディレクトリ:
/// - Linux: `$XDG_CACHE_HOME/vibrato-rkyv` または `$HOME/.cache/vibrato-rkyv`
/// - macOS: `$HOME/Library/Caches/vibrato-rkyv`
/// - Windows: `{FOLDERID_LocalAppData}/vibrato-rkyv`
pub static GLOBAL_CACHE_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let path = dirs::cache_dir()?.join("vibrato-rkyv");
    fs::create_dir_all(&path).ok()?;

    Some(path)
});

/// グローバルデータディレクトリのパス。
///
/// ユーザー固有のローカルデータディレクトリ内の`vibrato-rkyv`サブディレクトリを指します。
/// 各プラットフォームでの標準的なデータディレクトリ:
/// - Linux: `$XDG_DATA_HOME/vibrato-rkyv` または `$HOME/.local/share/vibrato-rkyv`
/// - macOS: `$HOME/Library/Application Support/vibrato-rkyv`
/// - Windows: `{FOLDERID_LocalAppData}/vibrato-rkyv`
pub static GLOBAL_DATA_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let path = dirs::data_local_dir()?.join("vibrato-rkyv");
    fs::create_dir_all(&path).ok()?;

    Some(path)
});

/// 辞書の読み込みモード。
///
/// 辞書ファイルを読み込む際の検証戦略を指定します。
/// 安全性とパフォーマンスのトレードオフを制御できます。
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum LoadMode {
    /// 読み込むたびに完全な検証を実行します(最も安全)。
    ///
    /// このモードでは、辞書データの整合性を毎回検証するため、
    /// 最も安全ですがパフォーマンスは低下します。
    /// キャッシュファイルは作成されません。
    Validate,
    /// 事前計算されたハッシュが一致する場合は検証をスキップします(繰り返しの読み込みで最速)。
    ///
    /// このモードでは、ファイルメタデータに基づくハッシュを使用して、
    /// 検証済みであることを確認します。高速な読み込みが可能ですが、
    /// ファイルが置き換えられるTOCTOU攻撃に対して脆弱です。
    TrustCache,
}

/// Zstandardアーカイブから展開された辞書のキャッシング戦略を指定します。
///
/// 辞書ファイルが圧縮されている場合、展開後のデータをどこにキャッシュするかを制御します。
pub enum CacheStrategy {
    /// 圧縮辞書と同じディレクトリに`.cache`サブディレクトリを作成します。
    ///
    /// この戦略は、キャッシュデータを元のファイルと並べて保持します。
    /// 親ディレクトリが書き込み可能でない場合は失敗します。
    Local,

    /// オペレーティングシステムに適した、共有のユーザー固有キャッシュディレクトリを使用します。
    ///
    /// ほとんどのアプリケーションに適したデフォルトの選択肢です。
    /// 特に辞書ファイルが読み取り専用の場所に保存されている場合に有用です。
    /// パスは`dirs::cache_dir()`によって決定されます。
    ///
    /// | プラットフォーム | 値                             | 例                               |
    /// | -------- | --------------------------------- | ------------------------------------- |
    /// | Linux    | `$XDG_CACHE_HOME` または `$HOME/.cache` | `/home/alice/.cache`                  |
    /// | macOS    | `$HOME/Library/Caches`            | `/Users/Alice/Library/Caches`         |
    /// | Windows  | `{FOLDERID_LocalAppData}`         | `C:\Users\Alice\AppData\Local`        |
    ///
    GlobalCache,

    /// オペレーティングシステムに適した、共有のユーザー固有データディレクトリを使用します。
    ///
    /// `GlobalCache`に似ていますが、永続的で非ローミングのアプリケーションデータ用の
    /// ディレクトリを使用します。パスは`dirs::data_local_dir()`によって決定されます。
    ///
    /// | プラットフォーム | 値                                     | 例                               |
    /// | -------- | ----------------------------------------- | ------------------------------------- |
    /// | Linux    | `$XDG_DATA_HOME` または `$HOME/.local/share`  | `/home/alice/.local/share`            |
    /// | macOS    | `$HOME/Library/Application Support`       | `/Users/Alice/Library/Application Support` |
    /// | Windows  | `{FOLDERID_LocalAppData}`                 | `C:\Users\Alice\AppData\Local`        |
    ///
    GlobalData,
}

/// [`Dictionary`]の内部データ。
///
/// 辞書の実際のデータを保持する構造体です。
/// システム辞書、ユーザー辞書、接続コスト、文字プロパティ、未知語処理などの
/// すべての必要なコンポーネントを含みます。
#[derive(Archive, Serialize, Deserialize)]
pub struct DictionaryInner {
    system_lexicon: Lexicon,
    user_lexicon: Option<Lexicon>,
    connector: ConnectorWrapper,
    mapper: Option<ConnIdMapper>,
    char_prop: CharProperty,
    unk_handler: UnkHandler,
}

/// メモリバッファ(mmapまたはヒープ)を所有し、アーカイブされた辞書へのアクセスを提供するラッパー。
///
/// この列挙型は、辞書データを保持するための2つの異なるメモリ戦略を表します:
/// - `Mmap`: メモリマップドファイルによるゼロコピーアクセス
/// - `Aligned`: ヒープ上のアライメント済みバッファ
#[allow(dead_code)]
enum DictBuffer {
    Mmap(Mmap),
    Aligned(AlignedVec<16>),
}

/// トークン化のための読み取り専用辞書。
///
/// ゼロコピーデシリアライゼーションによって読み込まれた辞書です。
/// 2つのバリアントがあります:
/// - `Archived`: メモリマップまたはアライメント済みバッファから直接アクセスされる辞書
/// - `Owned`: ヒープ上に所有される辞書データ(レガシー形式の変換時などに使用)
pub enum Dictionary {
    Archived(ArchivedDictionary),
    Owned {
        dict: Arc<DictionaryInner>,
        _caching_handle: Option<Arc<std::thread::JoinHandle<Result<()>>>>,
    },
}

/// アーカイブ形式の辞書。
///
/// メモリバッファとアーカイブされた辞書データへの参照を保持します。
/// ゼロコピーアクセスを可能にし、高速な辞書参照を実現します。
pub struct ArchivedDictionary {
    _buffer: DictBuffer,
    data: &'static ArchivedDictionaryInner,
}

/// 辞書内部データへの参照(アーカイブ版または所有版)。
///
/// 辞書の実装の詳細を隠蔽し、アーカイブ版と所有版の両方に対して
/// 統一的なインターフェースを提供します。
pub(crate) enum DictionaryInnerRef<'a> {
    Archived(&'a ArchivedDictionaryInner),
    Owned(&'a DictionaryInner),
}

/// コネクタへの参照(アーカイブ版または所有版)。
///
/// 接続コスト計算のために使用されるコネクタデータへの参照を提供します。
pub(crate) enum ConnectorKindRef<'a> {
    Archived(&'a ArchivedConnectorWrapper),
    Owned(&'a ConnectorWrapper),
}

impl Deref for ArchivedDictionary {
    type Target = ArchivedDictionaryInner;
    fn deref(&self) -> &Self::Target {
        self.data
    }
}

/// 単語を含む語彙辞書の種類。
///
/// 形態素解析時に使用される辞書の種類を識別します。
/// システム辞書、ユーザー辞書、未知語の3種類があります。
#[derive(
    Clone, Copy, Eq, PartialEq, Debug, Hash,
    Archive, Serialize, Deserialize,
)]
#[rkyv(
    compare(PartialEq),
    derive(Debug, Eq, PartialEq, Hash, Clone, Copy),
)]
#[repr(u8)]
#[derive(Default)]
pub enum LexType {
    /// システム辞書。
    ///
    /// 基本的な語彙を含むメインの辞書です。
    #[default]
    System,
    /// ユーザー辞書。
    ///
    /// ユーザーが定義した追加の語彙を含む辞書です。
    User,
    /// 未知語。
    ///
    /// システム辞書にもユーザー辞書にも見つからない単語です。
    Unknown,
}

impl ArchivedLexType {
    /// この[`ArchivedLexType`]を対応する[`LexType`]に変換します。
    ///
    /// # 戻り値
    ///
    /// アーカイブされた列挙値に対応するネイティブの`LexType`値。
    pub fn to_native(&self) -> LexType {
        match self {
            ArchivedLexType::System => LexType::System,
            ArchivedLexType::User => LexType::User,
            ArchivedLexType::Unknown => LexType::Unknown,
        }
    }
}

impl Drop for Dictionary {
    fn drop(&mut self) {
        if let Dictionary::Owned { _caching_handle, .. } = self
            && let Some(handle_arc) = _caching_handle.take()
            && let Ok(handle) = Arc::try_unwrap(handle_arc)
            && let Err(e) = handle.join() {
                log::error!("[vibrato-rkyv] Background caching thread panicked: {:?}", e);
            }
    }
}

impl DictionaryInner {
    /// システム辞書への参照を取得します。
    ///
    /// # 戻り値
    ///
    /// システム辞書(`Lexicon`)への参照。
    #[inline(always)]
    pub(crate) const fn system_lexicon(&self) -> &Lexicon {
        &self.system_lexicon
    }

    /// ユーザー辞書への参照を取得します。
    ///
    /// # 戻り値
    ///
    /// ユーザー辞書が存在する場合は`Some(&Lexicon)`、存在しない場合は`None`。
    #[inline(always)]
    pub(crate) const fn user_lexicon(&self) -> Option<&Lexicon> {
        self.user_lexicon.as_ref()
    }

    /// 接続ID用のマッパーへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// マッパーが存在する場合は`Some(&ConnIdMapper)`、存在しない場合は`None`。
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) const fn mapper(&self) -> Option<&ConnIdMapper> {
        self.mapper.as_ref()
    }

    /// 文字プロパティへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// 文字プロパティ(`CharProperty`)への参照。
    #[inline(always)]
    pub(crate) const fn char_prop(&self) -> &CharProperty {
        &self.char_prop
    }

    /// 未知語ハンドラへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// 未知語ハンドラ(`UnkHandler`)への参照。
    #[inline(always)]
    pub(crate) const fn unk_handler(&self) -> &UnkHandler {
        &self.unk_handler
    }

    /// 指定された単語の素性文字列への参照を取得します。
    ///
    /// # 引数
    ///
    /// * `word_idx` - 単語のインデックス。辞書の種類と位置を含みます。
    ///
    /// # 戻り値
    ///
    /// 素性文字列への参照。
    #[inline(always)]
    pub fn word_feature(&self, word_idx: WordIdx) -> &str {
        match word_idx.lex_type {
            LexType::System => self.system_lexicon().word_feature(word_idx),
            LexType::User => self.user_lexicon().unwrap().word_feature(word_idx),
            LexType::Unknown => self.unk_handler().word_feature(word_idx),
        }
    }

    /// コネクタへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// 接続コスト計算に使用される`ConnectorWrapper`への参照。
    pub(crate) fn connector(&self) -> &ConnectorWrapper {
        &self.connector
    }

    /// 指定された単語のパラメータを取得します。
    ///
    /// # 引数
    ///
    /// * `word_idx` - 単語のインデックス。辞書の種類と位置を含みます。
    ///
    /// # 戻り値
    ///
    /// 単語のパラメータ(`WordParam`)。左接続ID、右接続ID、単語コストを含みます。
    #[inline(always)]
    pub(crate) fn word_param(&self, word_idx: WordIdx) -> WordParam {
        match word_idx.lex_type {
            LexType::System => self.system_lexicon().word_param(word_idx),
            LexType::User => self.user_lexicon().as_ref().unwrap().word_param(word_idx),
            LexType::Unknown => self.unk_handler().word_param(word_idx),
        }
    }

    /// 辞書データを`rkyv`フォーマットを使用してライターにシリアライズします。
    ///
    /// この関数の出力バイナリは、`Dictionary::from_path`などの`vibrato-rkyv`の
    /// 読み込みメソッドが期待する形式です。
    ///
    /// # Examples
    ///
    /// この例では、メモリ内のCSVデータから辞書を構築し、
    /// シリアライズされたバイナリをファイルに書き込む方法を示します。
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::fs::File;
    /// use std::io::Cursor;
    /// use vibrato_rkyv::dictionary::SystemDictionaryBuilder;
    ///
    /// // ソースデータからビルダーを使用して辞書インスタンスを作成します。
    /// let dict = SystemDictionaryBuilder::from_readers(
    ///     Cursor::new("東京,名詞,地名\n"),
    ///     Cursor::new("1 1 0\n"),
    ///     Cursor::new("DEFAULT 0 0 0\n"),
    ///     Cursor::new("DEFAULT,5,5,-1000\n"),
    /// )?;
    ///
    /// // 辞書をファイルにシリアライズします。
    /// let mut file = File::create("system.dic")?;
    /// dict.write(&mut file)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - 基礎となる`writer`への書き込みに失敗した場合(例: I/Oエラー)。
    /// - `rkyv`シリアライゼーションプロセスでエラーが発生した場合。
    pub fn write<W>(&self, mut wtr: W) -> Result<()>
    where
        W: Write,
    {
        wtr.write_all(MODEL_MAGIC)?;

        let padding_bytes = vec![0xFF; PADDING_LEN];
        wtr.write_all(&padding_bytes)?;

        with_arena(|arena: &mut Arena| {
            let writer = IoWriter::new(&mut wtr);
            let mut serializer = Serializer::new(writer, arena.acquire(), Share::new());
            serialize_using::<_, rkyv::rancor::Error>(self, &mut serializer)
        })
        .map_err(|e| {
            VibratoError::invalid_state("rkyv serialization failed".to_string(), e.to_string())
        })?;

        Ok(())
    }

    /// リーダーからユーザー辞書をリセットします。
    ///
    /// この関数は、辞書をシリアライズする前に呼び出す必要があります。
    /// ユーザー辞書を新しいデータで置き換えるか、削除します。
    ///
    /// # 引数
    ///
    /// * `user_lexicon_rdr` - ユーザー辞書データを含むリーダー。`None`の場合、ユーザー辞書が削除されます。
    ///
    /// # 戻り値
    ///
    /// 更新された`DictionaryInner`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - ユーザー辞書の読み込みに失敗した場合。
    /// - ユーザー辞書に無効な接続IDが含まれている場合。
    pub fn reset_user_lexicon_from_reader<R>(mut self, user_lexicon_rdr: Option<R>) -> Result<Self>
    where
        R: Read,
    {
        if let Some(user_lexicon_rdr) = user_lexicon_rdr {
            let mut user_lexicon = Lexicon::from_reader(user_lexicon_rdr, LexType::User)?;
            if let Some(mapper) = self.mapper.as_ref() {
                user_lexicon.map_connection_ids(mapper);
            }
            if !user_lexicon.verify(&self.connector) {
                return Err(VibratoError::invalid_argument(
                    "user_lexicon_rdr",
                    "includes invalid connection ids.",
                ));
            }
            self.user_lexicon = Some(user_lexicon);
        } else {
            self.user_lexicon = None;
        }
        Ok(self)
    }

    /// 指定されたマッピングを使用して接続IDを編集します。
    ///
    /// この関数は、辞書をシリアライズする前に呼び出す必要があります。
    /// 左接続IDと右接続IDのマッピングを適用して、接続コスト行列を再構成します。
    ///
    /// # 引数
    ///
    /// * `lmap` - 左接続IDのマッピングを含むイテレータ。
    /// * `rmap` - 右接続IDのマッピングを含むイテレータ。
    ///
    /// # 戻り値
    ///
    /// 更新された`DictionaryInner`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - マッパーの作成に失敗した場合。
    pub fn map_connection_ids_from_iter<L, R>(mut self, lmap: L, rmap: R) -> Result<Self>
    where
        L: IntoIterator<Item = u16>,
        R: IntoIterator<Item = u16>,
    {
        let mapper = ConnIdMapper::from_iter(lmap, rmap)?;
        self.system_lexicon.map_connection_ids(&mapper);
        if let Some(user_lexicon) = self.user_lexicon.as_mut() {
            user_lexicon.map_connection_ids(&mapper);
        }
        self.connector.map_connection_ids(&mapper);
        self.unk_handler.map_connection_ids(&mapper);
        self.mapper = Some(mapper);
        Ok(self)
    }
}

impl Dictionary {
    /// `DictionaryInner`から辞書を作成します。
    ///
    /// # 引数
    ///
    /// * `dict` - 辞書の内部データ。
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    pub fn from_inner(dict: DictionaryInner) -> Self {
        Self::Owned{ dict: Arc::new(dict), _caching_handle: None }
    }

    /// 辞書データを`rkyv`フォーマットを使用してライターにシリアライズします。
    ///
    /// この関数の出力バイナリは、`Dictionary::from_path`などの`vibrato-rkyv`の
    /// 読み込みメソッドが期待する形式です。
    ///
    /// # Examples
    ///
    /// この例では、メモリ内のCSVデータから辞書を構築し、
    /// シリアライズされたバイナリをファイルに書き込む方法を示します。
    ///
    /// ```no_run
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::fs::File;
    /// use std::io::Cursor;
    /// use vibrato_rkyv::{Dictionary, SystemDictionaryBuilder};
    ///
    /// // ソースデータからビルダーを使用して辞書インスタンスを作成します。
    /// let dict = SystemDictionaryBuilder::from_readers(
    ///     Cursor::new("東京,名詞,地名\n"),
    ///     Cursor::new("1 1 0\n"),
    ///     Cursor::new("DEFAULT 0 0 0\n"),
    ///     Cursor::new("DEFAULT,5,5,-1000\n"),
    /// )?;
    ///
    /// let dict = Dictionary::from_inner(dict);
    ///
    /// // 辞書をファイルにシリアライズします。
    /// let mut file = File::create("system.dic")?;
    /// dict.write(&mut file)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - 基礎となる`writer`への書き込みに失敗した場合(例: I/Oエラー)。
    /// - `rkyv`シリアライゼーションプロセスでエラーが発生した場合。
    ///
    /// # Panics
    ///
    /// `Dictionary::Archived`バリアントでこのメソッドが呼び出された場合にパニックします。
    pub fn write<W>(&self, wtr: W) -> Result<()>
    where
        W: Write,
    {
        match self {
            Dictionary::Owned { dict, ..} => dict.write(wtr),
            Dictionary::Archived(_) => unreachable!(),
        }
    }


    /// すべてのデータをヒープバッファに読み込むことで、リーダーから辞書を作成します。
    ///
    /// これは、ファイルパスが利用できない場合(例: メモリ内バッファからの読み込み)の
    /// フォールバックです。すべてのコンテンツをメモリに読み込むため、
    /// `from_path`よりもメモリ効率が低くなります。
    ///
    /// # 引数
    ///
    /// * `rdr` - `std::io::Read`を実装するリーダー。
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - データを読み込めない場合。
    /// - コンテンツが無効な場合。
    pub fn read<R: Read>(mut rdr: R) -> Result<Self> {
        let mut magic = [0; MODEL_MAGIC_LEN];
        rdr.read_exact(&mut magic)?;

        if magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {
            return Err(VibratoError::invalid_argument(
                "rdr",
                "This appears to be a legacy bincode-based dictionary file. Please use a dictionary compiled for the rkyv version of vibrato.",
            ));
        }else if !magic.starts_with(MODEL_MAGIC) {
            return Err(VibratoError::invalid_argument(
                "rdr",
                "The magic number of the input model mismatches.",
            ));
        }

        let mut padding_buf = vec![0; PADDING_LEN];
        rdr.read_exact(&mut padding_buf)?;

        let mut buffer = Vec::new();
        rdr.read_to_end(&mut buffer)?;

        let mut aligned_bytes = AlignedVec::with_capacity(buffer.len());
        aligned_bytes.extend_from_slice(&buffer);

        let archived = access::<ArchivedDictionaryInner, Error>(&aligned_bytes).map_err(|e| {
            VibratoError::invalid_state(
                "rkyv validation failed. The dictionary file may be corrupted or incompatible."
                    .to_string(),
                e.to_string(),
            )
        })?;

        // SAFETY: AlignedVec ensures correct alignment for ArchivedDictionaryInner
        let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };

        Ok(
            Self::Archived(
                ArchivedDictionary { _buffer: DictBuffer::Aligned(aligned_bytes), data }
            )
        )
    }

    /// メモリマッピングを使用してファイルパスから辞書を作成します。
    ///
    /// この関数は、辞書ファイルをメモリにマップしてゼロコピーアクセスを実現し、
    /// 高いパフォーマンスとメモリ効率を提供します。読み込み動作は`mode`パラメータで
    /// 設定でき、安全性とパフォーマンスのバランスを調整できます。
    ///
    /// また、`legacy`フィーチャーが有効な場合、レガシー(bincodeベース)辞書を
    /// 透過的に処理し、メモリに読み込みます。
    ///
    /// | モード | 検証 | キャッシュ書き込み | 用途 |
    /// |------|-------------|---------------|-----------|
    /// | `Validate` | 毎回完全検証 | ❌ | 最大の安全性 |
    /// | `TrustCache` | プルーフファイルが存在する場合はスキップ | ✅ | 高速な再読み込み |
    ///
    ///
    /// ## キャッシングメカニズム(`LoadMode::TrustCache`)
    ///
    /// 後続の読み込みを高速化するため、この関数は`TrustCache`モードが有効な場合に
    /// キャッシュメカニズムを使用します。辞書ファイルのメタデータ(サイズ、更新時刻など)から
    /// 一意のハッシュを生成し、対応する「プルーフファイル」(例: `<hash>.sha256`)を探して、
    /// 完全な検証を行わずに辞書の妥当性を証明します。
    ///
    /// このプルーフファイルの検索は2つの場所で行われます:
    /// 1.  **ローカルキャッシュ**: 辞書ファイルと同じディレクトリ内。これにより、
    ///     辞書と一緒に移動できるポータブルなキャッシュが可能になります。
    /// 2.  **グローバルキャッシュ**: システム全体のユーザー固有キャッシュディレクトリ
    ///     (例: Linux上の`~/.cache/vibrato-rkyv`)。
    ///
    /// いずれかの場所で有効なプルーフファイルが見つかった場合、辞書は追加の検証なしで
    /// 即座に読み込まれます。
    ///
    /// プルーフファイルが見つからない場合、関数は完全な検証を実行します。成功した場合、
    /// **グローバルキャッシュディレクトリに新しいプルーフファイルを作成**して、
    /// 次回の読み込みを高速化します。これにより、読み取り専用の場所にある辞書でも
    /// キャッシングの恩恵を受けることができます。
    ///
    /// # 引数
    ///
    /// - `path` - 辞書ファイルへのパス。
    /// - `mode` - 検証戦略を指定する[`LoadMode`]:
    ///   - `LoadMode::Validate`: 読み込むたびに辞書データの完全な検証を実行します。
    ///     これは最も安全なモードで、**キャッシュファイルを書き込みません**。
    ///     最大の安全性が必要な場合、またはファイル書き込みが禁止されている環境で使用します。
    ///   - `LoadMode::TrustCache`: 上記のキャッシュメカニズムを有効にします。
    ///     有効なプルーフファイルが見つかった場合、高速な未検証読み込みを試みます。
    ///     見つからない場合は、完全な検証にフォールバックし、成功時に
    ///     **グローバルキャッシュにプルーフファイルを作成**します。
    ///     **警告: このモードは、高いパフォーマンスを実現するためにファイルメタデータを
    ///     信頼して検証します。辞書ファイルが悪意のある攻撃者によって置き換えられる可能性が
    ///     ある場合、TOCTOU攻撃に対して脆弱です。ファイルの整合性が保証できない環境では
    ///     `LoadMode::Validate`を使用してください。**
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - ファイルを開けない、または読み込めない場合。
    /// - ファイルが破損している、無効な形式、またはマジックナンバーが一致しない場合。
    /// - ファイルが互換性のないバージョンのvibratoで作成された場合。
    /// - (`legacy`フィーチャーが無効)レガシーbincodeベースの辞書が提供された場合。
    pub fn from_path<P: AsRef<std::path::Path>>(path: P, mode: LoadMode) -> Result<Self> {
        let path = path.as_ref();
        let mut file = File::open(path).map_err(|e| {
            VibratoError::invalid_argument("path", format!("Failed to open dictionary file: {}", e))
        })?;
        let meta = &file.metadata()?;
        let mut magic = [0u8; MODEL_MAGIC_LEN];
        file.read_exact(&mut magic)?;

        if magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {
            #[cfg(not(feature = "legacy"))]
            return Err(VibratoError::invalid_argument(
                "path",
                "This appears to be a legacy bincode-based dictionary file. Please use a dictionary compiled for the rkyv version of vibrato.",
            ));

            #[cfg(feature = "legacy")]
            {
                use std::io::Seek;
                use crate::legacy;

                file.seek(io::SeekFrom::Start(0))?;

                let dict = legacy::Dictionary::read(file)?.data;

                let dict = unsafe {
                    use std::mem::transmute;

                    Arc::new(transmute::<legacy::dictionary::DictionaryInner, DictionaryInner>(dict))
                };

                return Ok(Self::Owned{ dict, _caching_handle: None });
            }
        } else if !magic.starts_with(MODEL_MAGIC) {
            return Err(VibratoError::invalid_argument(
                "path",
                "The magic number of the input model mismatches.",
            ));
        }

        let mmap = unsafe { Mmap::map(&file)? };

        let Some(data_bytes) = &mmap.get(DATA_START..) else {
            return Err(VibratoError::invalid_argument(
                "path",
                "Dictionary file too small or corrupted.",
            ));
        };

        let current_hash = compute_metadata_hash(meta);
        let hash_name = format!("{}.sha256", current_hash);
        let hash_path = path.parent().unwrap().join(".cache").join(&hash_name);

        if mode == LoadMode::TrustCache
            && hash_path.exists() {
                let archived = unsafe { access_unchecked::<ArchivedDictionaryInner>(data_bytes) };
                let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
                return {
                    Ok(
                        Dictionary::Archived(ArchivedDictionary { _buffer: DictBuffer::Mmap(mmap), data })
                    )
                };
            }

        let global_cache_dir = GLOBAL_CACHE_DIR.as_ref().ok_or_else(|| {
            VibratoError::invalid_state("Could not determine system cache directory.", "")
        })?;

        let hash_path = global_cache_dir.join(&hash_name);

        if mode == LoadMode::TrustCache
            && hash_path.exists() {
                let archived = unsafe { access_unchecked::<ArchivedDictionaryInner>(data_bytes) };
                let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
                return {
                    Ok(
                        Dictionary::Archived(ArchivedDictionary { _buffer: DictBuffer::Mmap(mmap), data })
                    )
                };
            }

        match access::<ArchivedDictionaryInner, Error>(data_bytes) {
            Ok(archived) => {
                if mode == LoadMode::TrustCache {
                    create_dir_all(global_cache_dir)?;
                    File::create_new(hash_path)?;
                }

                let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
                Ok(Self::Archived(
                    ArchivedDictionary {
                        _buffer: DictBuffer::Mmap(mmap),
                        data,
                    }
                ))
            }
            Err(_) => {
                let mut aligned_bytes = AlignedVec::with_capacity(data_bytes.len());
                aligned_bytes.extend_from_slice(data_bytes);

                let archived = access::<ArchivedDictionaryInner, Error>(&aligned_bytes).map_err(|e| {
                    VibratoError::invalid_state(
                        "rkyv validation failed. The dictionary file may be corrupted or incompatible.".to_string(),
                        e.to_string(),
                    )
                })?;

                let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
                Ok(Self::Archived(
                    ArchivedDictionary {
                        _buffer: DictBuffer::Aligned(aligned_bytes),
                        data,
                    }
                ))
            }
        }
    }

    /// 検証なしでメモリマッピングを使用してファイルパスから辞書を作成します。
    ///
    /// この関数は、データ検証をスキップして高速に読み込む`from_path`のバージョンです。
    /// 辞書ファイルをメモリマップしてゼロコピーアクセスを実現します。
    /// チェックサムなどによってファイルの整合性が既に確認されている状況を想定しています。
    ///
    /// # 引数
    ///
    /// * `path` - コンパイル済み辞書ファイルへのパス。
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - ファイルを開けない場合。
    /// - ファイルが小さすぎる場合。
    /// - マジックナンバーが不正な場合。
    ///
    /// この関数は、シリアライズされたデータ自体の整合性を検証しません。
    ///
    /// # Safety
    ///
    /// この関数はunsafeです。なぜなら、`rkyv`の検証ステップをバイパスして
    /// メモリマップされたデータに直接アクセスするためです。呼び出し側は、
    /// ファイルの内容が辞書の有効で破損していない表現であることを保証する必要があります。
    ///
    /// ファイルが破損または切り詰められている場合、この関数は無効なデータを
    /// 有効なポインタやオフセットであるかのように読み取る可能性があります。
    /// これにより、境界外メモリアクセス、パニック、またはその他の形式の未定義動作が
    /// 発生する可能性があります。
    ///
    /// ファイルの先頭のマジックナンバーチェックは、完全に異なるファイルタイプの
    /// 読み込みを防ぐのに役立ちますが、後続のデータの整合性を保証するものではありません。
    pub unsafe fn from_path_unchecked<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let mut file = File::open(path).map_err(|e| {
            VibratoError::invalid_argument("path", format!("Failed to open dictionary file: {}", e))
        })?;
        let mut magic = [0u8; MODEL_MAGIC_LEN];
        file.read_exact(&mut magic)?;

        if magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {
            #[cfg(not(feature = "legacy"))]
            return Err(VibratoError::invalid_argument(
                "path",
                "This appears to be a legacy bincode-based dictionary file. Please use a dictionary compiled for the rkyv version of vibrato.",
            ));

            #[cfg(feature = "legacy")]
            {
                use std::io::Seek;

                use crate::legacy;

                file.seek(io::SeekFrom::Start(0))?;

                let dict = legacy::Dictionary::read(file)?.data;

                let dict = unsafe {
                    use std::mem::transmute;

                    Arc::new(transmute::<legacy::dictionary::DictionaryInner, DictionaryInner>(dict))
                };

                return Ok(Self::Owned{ dict, _caching_handle: None });
            }
        } else if !magic.starts_with(MODEL_MAGIC) {
            return Err(VibratoError::invalid_argument(
                "path",
                "The magic number of the input model mismatches.",
            ));
        }

        let mmap = unsafe { Mmap::map(&file)? };

        let Some(data_bytes) = &mmap.get(DATA_START..) else {
            return Err(VibratoError::invalid_argument(
                "path",
                "Dictionary file too small or corrupted.",
            ));
        };

        let archived = unsafe { access_unchecked::<ArchivedDictionaryInner>(data_bytes) };
        let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
        Ok(
            Self::Archived(
                ArchivedDictionary {
                    _buffer: DictBuffer::Mmap(mmap),
                    data,
                }
            )
        )
    }

    /// 指定されたキャッシング戦略を使用してZstandard圧縮ファイルから辞書を読み込みます。
    ///
    /// この関数は、最も一般的なキャッシングシナリオに対してユーザーフレンドリーな
    /// インターフェースを提供します。より細かい制御が必要な場合は、
    /// [`from_zstd_with_options`]を参照してください。
    ///
    /// # 引数
    ///
    /// * `path` - Zstandard圧縮辞書ファイルへのパス。
    /// * `strategy` - [`CacheStrategy`]列挙型で定義される希望のキャッシング戦略。
    #[cfg_attr(feature = "legacy", doc = r"
    `legacy`フィーチャーが有効な場合、この関数はキャッシングがバックグラウンドで
    実行されている間に即座に戻り、応答性の高いユーザーエクスペリエンスを提供します。")]
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は、[`from_zstd_with_options`]のエラーに加えて、
    /// (`strategy`によって決定される)`cache_dir`が作成できない、
    /// または書き込めない場合にエラーを返します。
    pub fn from_zstd<P: AsRef<std::path::Path>>(path: P, strategy: CacheStrategy) -> Result<Self> {
        let path = path.as_ref();

        let cache_dir = match strategy {
            CacheStrategy::Local => {
                let parent = path.parent().ok_or_else(|| {
                    VibratoError::invalid_argument(
                        "path",
                        "Input path must have a parent directory for the Local cache strategy.",
                    )
                })?;
                let local_cache = parent.join(".cache");
                std::fs::create_dir_all(&local_cache)?;
                local_cache
            }
            CacheStrategy::GlobalCache => {
                let global_cache = GLOBAL_CACHE_DIR.as_ref().ok_or_else(|| {
                    VibratoError::invalid_state("Could not determine system cache directory.", "")
                })?;
                global_cache.to_path_buf()
            }
            CacheStrategy::GlobalData => {
                let local_data = GLOBAL_DATA_DIR.as_ref().ok_or_else(|| {
                    VibratoError::invalid_state("Could not determine local data directory.", "")
                })?;
                local_data.to_path_buf()
            }
        };

        Self::from_zstd_with_options(
            path,
            cache_dir,
            #[cfg(feature = "legacy")]
            false,
        )
    }

    /// 設定可能なキャッシングオプションを使用してZstandard圧縮ファイルから辞書を読み込みます。
    ///
    /// これは[`from_zstd`]の高度なバージョンで、キャッシュディレクトリの細かい制御を
    /// 可能にします。特定のディレクトリ構造や制限的なファイルシステム権限を持つ環境で
    /// 有用です。
    ///
    /// ## キャッシングメカニズム
    ///
    /// 実行ごとにファイルを展開するのを避けるため、この関数はキャッシュメカニズムを
    /// 採用しています。入力`.zst`ファイルのメタデータ(サイズや更新時刻など)から
    /// 一意のハッシュを生成します。このハッシュは、展開されたキャッシュのファイル名として
    /// 使用されます。
    ///
    /// 後続の実行時に、現在のメタデータハッシュに対応するキャッシュファイルが存在する場合、
    /// 展開ステップが完全にスキップされ、ほぼ瞬時の読み込みが可能になります。
    /// `.zst`ファイルが変更されると、そのメタデータハッシュが変更され、新しいキャッシュが
    /// 自動的に生成されます。
    ///
    /// # 引数
    ///
    /// * `path` - Zstandard圧縮辞書ファイルへのパス。
    /// * `cache_dir` - 展開された辞書キャッシュが保存されるディレクトリ。
    #[cfg_attr(feature = "legacy", doc = r" * `wait_for_cache` - (legacyフィーチャーのみ) `true`でレガシー(bincode)辞書が
    提供された場合、関数は新しい形式への変換とキャッシングが完了するまでブロックします。
    `false`の場合、完全に機能する辞書ですぐに戻り、キャッシングプロセスは
    バックグラウンドスレッドで実行されます。")]
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - `path`で指定されたファイルを開けない、または読み込めない場合(例: I/Oエラー)。
    /// - ファイルが有効なZstandard圧縮アーカイブでない場合。
    /// - 展開されたデータが有効な辞書ファイルでない場合(例: 破損データまたは不正なマジックナンバー)。
    /// - `cache_dir`で指定されたキャッシュディレクトリが作成できない、または書き込めない場合。
    #[cfg_attr(feature = "legacy", doc = r" - (legacyフィーチャーのみ) `wait_for_cache`が`true`のときにバックグラウンドキャッシングスレッドがパニックした場合。")]
    ///
    /// # Examples
    ///
    /// ### カスタムキャッシュディレクトリの指定
    ///
    /// ```no_run
    /// # use vibrato_rkyv::{Dictionary, errors::Result};
    /// # fn main() -> Result<()> {
    /// let dict = Dictionary::from_zstd_with_options(
    ///     "path/to/system.dic.zst",
    ///     "/tmp/my_app_cache",
    #[cfg_attr(feature = "legacy", doc = r"true, // バックグラウンドキャッシュ生成の完了を待つ")]
    /// )?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline(always)]
    pub fn from_zstd_with_options<P, Q>(
        path: P,
        cache_dir: Q,
        #[cfg(feature = "legacy")]
        wait_for_cache: bool,
    ) -> Result<Self>
    where
        P: AsRef<std::path::Path>,
        Q: AsRef<std::path::Path>,
    {
        let zstd_path = path.as_ref();
        let zstd_file = File::open(zstd_path)?;
        let meta = zstd_file.metadata()?;

        let dict_hash = compute_metadata_hash(&meta);
        let decompressed_dir = cache_dir.as_ref().to_path_buf();

        let decompressed_dict_path = decompressed_dir.join(format!("{}.dic", dict_hash));

        if decompressed_dict_path.exists() {
            return Self::from_path(decompressed_dict_path, LoadMode::TrustCache);
        }

        if !decompressed_dir.exists() {
            create_dir_all(&decompressed_dir)?;
        }

        let mut temp_file = tempfile::NamedTempFile::new_in(&decompressed_dir)?;

        {
            let mut decoder = zstd::Decoder::new(zstd_file)?;

            io::copy(&mut decoder, &mut temp_file)?;
            temp_file.as_file().sync_all()?;
        }
        temp_file.seek(SeekFrom::Start(0))?;

        let mut magic = [0; MODEL_MAGIC_LEN];
        temp_file.read_exact(&mut magic)?;

        #[cfg(feature = "legacy")]
        'l: {
            use std::thread;

            use crate::legacy;

            if !magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {
                break 'l;
            }

            let dict = legacy::Dictionary::read(
                zstd::Decoder::new(File::open(zstd_path)?)?
            )?.data;

            let dict = unsafe {
                use std::mem::transmute;

                Arc::new(transmute::<legacy::dictionary::DictionaryInner, DictionaryInner>(dict))
            };


            let dict_for_cache = Arc::clone(&dict);
            let handle = thread::spawn(move || -> Result<()> {
                let mut temp_file = tempfile::NamedTempFile::new_in(&decompressed_dir)?;

                dict_for_cache.write(&mut temp_file)?;

                temp_file.persist(&decompressed_dict_path)?;

                let dict_file = File::open(decompressed_dict_path)?;
                let decompressed_dict_hash = compute_metadata_hash(&dict_file.metadata()?);
                let decompressed_dict_hash_path = decompressed_dir.join(format!("{}.sha256", decompressed_dict_hash));

                File::create_new(decompressed_dict_hash_path)?;

                Ok(())
            });

            let _caching_handle = if wait_for_cache {
                handle.join().map_err(|e| {
                    let panic_msg = if let Some(s) = e.downcast_ref::<&'static str>() {
                        s.to_string()
                    } else if let Some(s) = e.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "Unknown panic".to_string()
                    };
                    VibratoError::ThreadPanic(panic_msg)
                })??;

                None
            } else {
                Some(std::sync::Arc::new(handle))
            };

            return Ok(Self::Owned { dict, _caching_handle });
        }

        if magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {
            return Err(VibratoError::invalid_argument(
                "path",
                "This appears to be a legacy bincode-based dictionary file. Please use a dictionary compiled for the rkyv version of vibrato.",
            ));
        } else if !magic.starts_with(MODEL_MAGIC) {
            return Err(VibratoError::invalid_argument(
                "path",
                "The magic number of the input model mismatches.",
            ));
        }

        temp_file.seek(SeekFrom::Start(0))?;

        let mut data_bytes = Vec::new();
        temp_file.as_file_mut().read_to_end(&mut data_bytes)?;

        let mut aligned_bytes: AlignedVec = AlignedVec::with_capacity(data_bytes.len());
        aligned_bytes.extend_from_slice(&data_bytes);

        let Some(data_bytes) = &aligned_bytes.get(DATA_START..) else {
            return Err(VibratoError::invalid_argument(
                "path",
                "Dictionary file too small or corrupted.",
            ));
        };

        let _ = access::<ArchivedDictionaryInner, Error>(data_bytes).map_err(|e| {
            VibratoError::invalid_state(
                "rkyv validation failed. The dictionary file may be corrupted or incompatible."
                    .to_string(),
                e.to_string(),
            )
        })?;

        temp_file.persist(&decompressed_dict_path)?;

        let decompressed_dict_hash = compute_metadata_hash(&File::open(&decompressed_dict_path)?.metadata()?);
        let decompressed_dict_hash_path = decompressed_dir.join(format!("{}.sha256", decompressed_dict_hash));

        File::create_new(decompressed_dict_hash_path)?;

        Self::from_path(decompressed_dict_path, LoadMode::TrustCache)
    }

    /// レガシー`bincode`ベースの辞書のリーダーから[`Dictionary`]インスタンスを作成します。
    ///
    /// この関数は、古い辞書形式を変換するための`compiler`などの内部ツールを
    /// 対象としています。辞書全体をメモリに読み込みます。
    ///
    /// この関数は、`legacy`フィーチャーが有効な場合にのみ使用できます。
    ///
    /// # 引数
    ///
    /// * `reader` - レガシー辞書データを読み込むリーダー。
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - リーダーからのデータ読み込みに失敗した場合。
    /// - レガシー辞書のデシリアライゼーションに失敗した場合。
    ///
    /// # Safety
    ///
    /// この関数は`unsafe`です。なぜなら、[`std::mem::transmute`]を使用して
    /// `bincode`でデシリアライズされた辞書構造をキャストするためです。
    /// このフォークは同一のメモリレイアウトを維持しているため、現在は安全です。
    #[cfg(feature = "legacy")]
    pub unsafe fn from_legacy_reader<R: std::io::Read>(reader: R) -> Result<Self> {
        let legacy_dict_inner = crate::legacy::Dictionary::read(reader)?.data;

        let rkyv_dict_inner = unsafe {
            std::mem::transmute::<
                crate::legacy::dictionary::DictionaryInner,
                DictionaryInner,
            >(legacy_dict_inner)
        };

        Ok(Self::Owned { dict: Arc::new(rkyv_dict_inner), _caching_handle: None })
    }

    /// プリセット辞書から`Dictionary`インスタンスを作成し、存在しない場合はダウンロードします。
    ///
    /// これは、プリコンパイル済み辞書を使い始めるための最も便利な方法です。
    /// この関数は、まず指定されたプリセット辞書が指定のディレクトリに既に存在するかを
    /// 確認します。存在し、整合性が検証された場合は直接読み込みます。
    /// それ以外の場合は、公式リポジトリから辞書をディレクトリにダウンロードし、
    /// その後読み込みます。
    ///
    /// ダウンロードされた辞書はZstandard圧縮されています。この関数は、
    /// メモリマッピングによる高速な後続読み込みのために、展開とキャッシングを
    /// 透過的に処理します。
    ///
    /// この関数は、`download`フィーチャーが有効な場合にのみ使用できます。
    ///
    /// # 引数
    ///
    /// * `kind` - 使用するプリセット辞書(例: `PresetDictionaryKind::Ipadic`)。
    /// * `dir` - 辞書が保存およびキャッシュされるディレクトリ。
    ///   永続的な場所を使用することを推奨します。
    ///
    /// # 戻り値
    ///
    /// 新しい`Dictionary`インスタンス。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - ダウンロードが失敗した場合(例: ネットワークの問題)。
    /// - ダウンロードされたファイルが破損している場合(ハッシュの不一致)。
    /// - キャッシュディレクトリの作成時にファイルシステム権限エラーがある場合。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use vibrato_rkyv::{Dictionary, Tokenizer, dictionary::PresetDictionaryKind};
    /// # let dir = Path::new("./cache_dir");
    /// // IPADICプリセット辞書をダウンロードして読み込みます。
    /// // 最初の呼び出しではファイルをダウンロードし、後続の呼び出しではキャッシュを使用します。
    /// let dictionary = Dictionary::from_preset_with_download(
    ///     PresetDictionaryKind::Ipadic,
    ///     dir,
    /// ).unwrap();
    ///
    /// let mut tokenizer = Tokenizer::new(dictionary);
    /// ```
    #[cfg(feature = "download")]
    pub fn from_preset_with_download<P: AsRef<std::path::Path>>(kind: PresetDictionaryKind, dir: P) -> Result<Self> {
        let dict_path = fetch::download_dictionary(kind, dir.as_ref())?;

        Self::from_zstd_with_options(
            dict_path,
            dir,
            #[cfg(feature = "legacy")]
            true,
        )
    }

    /// プリセット辞書ファイルをダウンロードし、そのパスを返します。
    ///
    /// ダウンロード後、辞書は[`Dictionary::from_zstd`]を使用して読み込むことができます。
    ///
    /// この関数は、`download`フィーチャーが有効な場合にのみ使用できます。
    ///
    /// # 引数
    ///
    /// * `kind` - ダウンロードするプリセット辞書(例: `PresetDictionaryKind::Ipadic`)。
    /// * `dir` - 辞書ファイルが保存されるディレクトリ。
    ///
    /// # 戻り値
    ///
    /// ダウンロードされたZstandard圧縮辞書ファイルへの`PathBuf`を含む`Result`。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - ダウンロードが失敗した場合。
    /// - ファイルが破損している場合。
    /// - ファイルシステム権限エラーがある場合。
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::path::Path;
    /// # use vibrato_rkyv::{Dictionary, dictionary::PresetDictionaryKind, CacheStrategy};
    /// # let dir = Path::new("./cache_dir");
    /// let dict_path = Dictionary::download_dictionary(
    ///     PresetDictionaryKind::UnidicCwj,
    ///     dir,
    /// ).unwrap();
    ///
    /// println!("辞書のダウンロード先: {:?}", dict_path);
    ///
    /// let dictionary = Dictionary::from_zstd(dict_path, CacheStrategy::Local).unwrap();
    /// ```
    #[cfg(feature = "download")]
    pub fn download_dictionary<P: AsRef<std::path::Path>>(kind: PresetDictionaryKind, dir: P) -> Result<std::path::PathBuf> {
        Ok(fetch::download_dictionary(kind, dir)?)
    }

    /// Zstandard圧縮辞書を指定されたパスに展開します。
    ///
    /// この関数は、`.zst`圧縮辞書を読み込み、その内容を検証し、
    /// 展開された辞書を`output_path`に書き込みます。
    ///
    /// これは、アプリケーションのセットアップ、テスト、または
    /// カスタムキャッシュ管理に有用な低レベルユーティリティです。
    ///
    /// # 引数
    ///
    /// * `input_path` - Zstandard圧縮辞書ファイルへのパス。
    /// * `output_path` - 展開された辞書が保存されるパス。
    ///
    /// # 戻り値
    ///
    /// 成功時は`Ok(())`。
    ///
    /// # エラー
    ///
    /// この関数は以下の場合にエラーを返します:
    /// - 入力ファイルを読み込めない場合。
    /// - 有効なZstandard圧縮アーカイブでない場合。
    /// - 展開されたデータが有効な辞書でない場合。
    /// - 出力パスに書き込めない場合。
    pub fn decompress_zstd<P, Q>(input_path: P, output_path: Q) -> Result<()>
    where
        P: AsRef<std::path::Path>,
        Q: AsRef<std::path::Path>,
    {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        let output_dir = output_path.parent().ok_or_else(|| {
            VibratoError::invalid_argument("output_path", "Output path must have a parent directory.")
        })?;
        std::fs::create_dir_all(output_dir)?;

        let zstd_file = File::open(input_path)?;
        let mut temp_file = tempfile::NamedTempFile::new_in(output_dir)?;

        let mut decoder = zstd::Decoder::new(zstd_file)?;
        io::copy(&mut decoder, &mut temp_file)?;

        temp_file.seek(SeekFrom::Start(0))?;
        let mut magic = [0; MODEL_MAGIC_LEN];
        temp_file.read_exact(&mut magic)?;

        if magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {
            return Err(VibratoError::invalid_argument(
                "path",
                "This appears to be a legacy bincode-based dictionary file. Please use a dictionary compiled for the rkyv version of vibrato.",
            ));
        } else if !magic.starts_with(MODEL_MAGIC) {
            return Err(VibratoError::invalid_argument(
                "path",
                "The magic number of the input model mismatches.",
            ));
        }

        temp_file.seek(SeekFrom::Start(0))?;
        let mut data_bytes = Vec::new();
        temp_file.as_file_mut().read_to_end(&mut data_bytes)?;

        let mut aligned_bytes: AlignedVec = AlignedVec::with_capacity(data_bytes.len());
        aligned_bytes.extend_from_slice(&data_bytes);

        let Some(data_bytes) = &aligned_bytes.get(DATA_START..) else {
            return Err(VibratoError::invalid_argument(
                "path",
                "Dictionary file too small or corrupted.",
            ));
        };

        let _ = access::<ArchivedDictionaryInner, Error>(data_bytes).map_err(|e| {
            VibratoError::invalid_state(
                "rkyv validation failed. The dictionary file may be corrupted or incompatible."
                    .to_string(),
                e.to_string(),
            )
        })?;

        temp_file.persist(output_path)?;

        Ok(())
    }
}

/// ファイルメタデータからハッシュを計算します。
///
/// この関数は、ファイルのメタデータ(サイズ、更新時刻、iノードなど)から
/// 一意のSHA256ハッシュを生成します。このハッシュは、キャッシュファイルの
/// 命名とファイルの同一性確認に使用されます。
///
/// # 引数
///
/// * `meta` - ハッシュを計算するファイルのメタデータ。
///
/// # 戻り値
///
/// メタデータのSHA256ハッシュの16進数表現文字列。
///
/// # プラットフォーム固有の動作
///
/// - Unix: デバイスID、iノード、サイズ、変更時刻を使用
/// - Windows: ファイルサイズ、最終書き込み時刻、作成時刻、ファイル属性を使用
/// - その他: ファイルタイプ、読み取り専用フラグ、サイズ、変更時刻、作成時刻を使用
#[inline(always)]
pub(crate) fn compute_metadata_hash(meta: &Metadata) -> String {
    let mut hasher = Sha256::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hasher.update(meta.dev().to_le_bytes());
        hasher.update(meta.ino().to_le_bytes());
        hasher.update(meta.size().to_le_bytes());
        hasher.update(meta.mtime().to_le_bytes());
        hasher.update(meta.mtime_nsec().to_le_bytes());
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        hasher.update(meta.file_size().to_le_bytes());
        hasher.update(meta.last_write_time().to_le_bytes());
        hasher.update(meta.creation_time().to_le_bytes());
        hasher.update(meta.file_attributes().to_le_bytes());
    }

    #[cfg(not(any(unix, windows)))]
    {
        use std::time::SystemTime;

        fn update_system_time(
            time: Result<SystemTime, std::io::Error>,
            hasher: &mut Sha256,
        ) {
            match time.and_then(|t| {
                t.duration_since(SystemTime::UNIX_EPOCH)
                    .map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))
            }) {
                Ok(duration) => {
                    hasher.update(duration.as_secs().to_le_bytes());
                    hasher.update(duration.subsec_nanos().to_le_bytes());
                }
                Err(_) => {
                    hasher.update([0u8; 12]);
                }
            }
        }

        let file_type = meta.file_type();
        let type_byte: u8 = if file_type.is_file() { 0x01 }
        else if file_type.is_dir() { 0x02 }
        else if file_type.is_symlink() { 0x03 }
        else { 0x00 };
        hasher.update([type_byte]);

        let readonly_byte: u8 = if meta.permissions().readonly() { 0x01 } else { 0x00 };
        hasher.update([readonly_byte]);

        hasher.update(meta.len().to_le_bytes());

        update_system_time(meta.modified(), &mut hasher);

        update_system_time(meta.created(), &mut hasher);
    }

    hex::encode(hasher.finalize())
}

impl<'a> DictionaryInnerRef<'a> {
    /// コネクタへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// アーカイブ版または所有版のコネクタへの参照。
    #[inline(always)]
    pub fn connector(&self) -> ConnectorKindRef<'a> {
        match self {
            DictionaryInnerRef::Archived(archived) => ConnectorKindRef::Archived(archived.connector()),
            DictionaryInnerRef::Owned(owned) => ConnectorKindRef::Owned(owned.connector()),
        }
    }

    /// 指定された単語のパラメータを取得します。
    ///
    /// # 引数
    ///
    /// * `word_idx` - 単語のインデックス。辞書の種類と位置を含みます。
    ///
    /// # 戻り値
    ///
    /// 単語のパラメータ(`WordParam`)。左接続ID、右接続ID、単語コストを含みます。
    #[inline(always)]
    pub(crate) fn word_param(&self, word_idx: WordIdx) -> WordParam {
        match self {
            DictionaryInnerRef::Archived(archived_dict) => {
                archived_dict.word_param(word_idx)
            },
            DictionaryInnerRef::Owned(dict) => {
                dict.word_param(word_idx)
            },
        }
    }
}

impl ArchivedDictionaryInner {
    /// コネクタへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// アーカイブされた`ConnectorWrapper`への参照。
    #[inline(always)]
    pub(crate) fn connector(&self) -> &ArchivedConnectorWrapper {
        &self.connector
    }
    /// システム辞書への参照を取得します。
    ///
    /// # 戻り値
    ///
    /// アーカイブされたシステム辞書(`ArchivedLexicon`)への参照。
    #[inline(always)]
    pub(crate) fn system_lexicon(&self) -> &ArchivedLexicon {
        &self.system_lexicon
    }
    /// ユーザー辞書への参照を取得します。
    ///
    /// # 戻り値
    ///
    /// アーカイブされたユーザー辞書への参照。
    #[inline(always)]
    pub(crate) fn user_lexicon(&self) -> &Archived<Option<Lexicon>> {
        &self.user_lexicon
    }
    /// 文字プロパティへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// アーカイブされた文字プロパティ(`ArchivedCharProperty`)への参照。
    #[inline(always)]
    pub(crate) fn char_prop(&self) -> &ArchivedCharProperty {
        &self.char_prop
    }
    /// 未知語ハンドラへの参照を取得します。
    ///
    /// # 戻り値
    ///
    /// アーカイブされた未知語ハンドラ(`ArchivedUnkHandler`)への参照。
    #[inline(always)]
    pub(crate) fn unk_handler(&self) -> &ArchivedUnkHandler {
        &self.unk_handler
    }
    /// 指定された単語のパラメータを取得します。
    ///
    /// # 引数
    ///
    /// * `word_idx` - 単語のインデックス。辞書の種類と位置を含みます。
    ///
    /// # 戻り値
    ///
    /// 単語のパラメータ(`WordParam`)。左接続ID、右接続ID、単語コストを含みます。
    #[inline(always)]
    pub(crate) fn word_param(&self, word_idx: WordIdx) -> WordParam {
        match word_idx.lex_type {
            LexType::System => self.system_lexicon().word_param(word_idx),
            LexType::User => self.user_lexicon().as_ref().unwrap().word_param(word_idx),
            LexType::Unknown => self.unk_handler().word_param(word_idx),
        }
    }

    /// 指定された単語の素性文字列への参照を取得します。
    ///
    /// # 引数
    ///
    /// * `word_idx` - 単語のインデックス。辞書の種類と位置を含みます。
    ///
    /// # 戻り値
    ///
    /// 素性文字列への参照。
    #[inline(always)]
    pub fn word_feature(&self, word_idx: WordIdx) -> &str {
        match word_idx.lex_type {
            LexType::System => self.system_lexicon().word_feature(word_idx),
            LexType::User => self.user_lexicon().as_ref().unwrap().word_feature(word_idx),
            LexType::Unknown => self.unk_handler().word_feature(word_idx),
        }
    }
}
