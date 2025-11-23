# rkyv ゼロコピーデシリアライゼーション実装解説

## 概要

`vibrato-rkyv` は、元の `vibrato` プロジェクトを fork し、シリアライゼーションフレームワークを `bincode` から `rkyv` に変更することで、辞書のロード時間を劇的に短縮しました。

### パフォーマンス比較

**非圧縮ファイル (.dic) からのロード:**

| 条件 | 元の `vibrato` (ストリーム読み込み) | `vibrato-rkyv` (メモリマップ) | 高速化倍率 |
|:-----|:------------------------------------|:------------------------------|:-----------|
| コールドスタート (安全キャッシュ) | ~42秒 | **~1.1ms** | **~38,000倍** |
| ウォームスタート (チェックなし) | ~34秒 | **~2.9µs** | **~11,700,000倍** |
| ウォームスタート (安全キャッシュ) | ~34秒 | **~4.1µs** | **~8,300,000倍** |

**Zstd圧縮ファイル (.dic.zst) からのロード:**

| 条件 | 元の `vibrato` | `vibrato-rkyv` | 高速化倍率 |
|:-----|:--------------|:--------------|:-----------|
| 初回実行 (コールド) | ~4.6秒 | ~1.3秒 | ~3.5倍 |
| 2回目以降 (キャッシュヒット) | ~4.5秒 | **~6.5µs** | **~700,000倍** |

---

## 1. アーキテクチャの根本的な違い

### 1.1 従来の bincode ベースの実装

**元の vibrato (legacy) の辞書ロード方法:**

```rust
// vibrato/src/legacy/dictionary.rs:82-106

pub fn read<R>(rdr: R) -> Result<Self>
where
    R: Read,                                               // 汎用的な Read トレイト
{
    Ok(Self {
        data: Self::read_common(rdr)?,                     // 共通の読み込み処理を呼び出す
    })
}

fn read_common<R>(mut rdr: R) -> Result<DictionaryInner>
where
    R: Read,
{
    let mut magic = [0; MODEL_MAGIC.len()];
    rdr.read_exact(&mut magic)?;                           // マジックバイトを読み込み
    if magic != MODEL_MAGIC {
        return Err(VibratoError::invalid_argument(
            "rdr",
            "The magic number of the input model mismatches.",
        ));
    }
    let config = common::bincode_config();                 // bincode の設定を取得
    let data = bincode::decode_from_std_read(&mut rdr, config)?;  // ★ここで全データをデシリアライズ
    Ok(data)                                               // デシリアライズされたデータを返す
}
```

**問題点:**
- `bincode::decode_from_std_read()` が全データを読み込み、メモリ上に新しいデータ構造を構築
- 全てのバイト列を走査し、各フィールドを適切な型に変換する必要がある
- 辞書サイズが大きい（700MB）場合、この処理に数十秒かかる
- メモリコピーが大量に発生（ディスクからバッファ、バッファから構造体）

**データ構造 (legacy):**

```rust
// vibrato/src/legacy/dictionary.rs:38-46

#[derive(Decode, Encode)]                                 // bincode用の derive
pub struct DictionaryInner {
    pub system_lexicon: Lexicon,                           // システム辞書
    pub user_lexicon: Option<Lexicon>,                     // ユーザー辞書（オプション）
    pub connector: ConnectorWrapper,                       // 接続コスト行列
    pub mapper: Option<ConnIdMapper>,                      // 接続ID マッパー
    pub char_prop: CharProperty,                           // 文字プロパティ
    pub unk_handler: UnkHandler,                           // 未知語ハンドラ
}
```

### 1.2 新しい rkyv ベースの実装

**rkyv の核心的な違い:**

`rkyv` はゼロコピーデシリアライゼーションフレームワークです。従来のシリアライゼーションライブラリ（bincode、serde など）がデシリアライゼーション時に新しいデータ構造を構築するのに対し、`rkyv` はディスク上のバイト列を **そのまま** データ構造として扱います。

**新しいデータ構造 (rkyv):**

```rust
// vibrato/src/dictionary.rs:119-127

#[derive(Archive, Serialize, Deserialize)]                // rkyv用の derive
pub struct DictionaryInner {
    system_lexicon: Lexicon,                               // システム辞書
    user_lexicon: Option<Lexicon>,                         // ユーザー辞書（オプション）
    connector: ConnectorWrapper,                           // 接続コスト行列
    mapper: Option<ConnIdMapper>,                          // 接続ID マッパー
    char_prop: CharProperty,                               // 文字プロパティ
    unk_handler: UnkHandler,                               // 未知語ハンドラ
}
```

**重要な特徴:**
- `Archive` トレイトにより、アーカイブ版 `ArchivedDictionaryInner` が自動生成される
- アーカイブ版は、ディスク上のバイト列のレイアウトをそのまま表現
- デシリアライゼーション = 単なるポインタキャストで完了

---

## 2. ゼロコピーロードの実装詳細

### 2.1 メモリマップによる超高速ロード

**最速のロードパス: `from_path_unchecked`**

```rust
// vibrato/src/dictionary.rs:686-745

pub unsafe fn from_path_unchecked<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
    let path = path.as_ref();
    let mut file = File::open(path).map_err(|e| {          // ファイルを開く
        VibratoError::invalid_argument("path", format!("Failed to open dictionary file: {}", e))
    })?;
    let mut magic = [0u8; MODEL_MAGIC_LEN];
    file.read_exact(&mut magic)?;                          // マジックバイトのみ読み込み（検証用）

    if magic.starts_with(LEGACY_MODEL_MAGIC_PREFIX) {     // レガシー形式の検出
        #[cfg(not(feature = "legacy"))]
        return Err(VibratoError::invalid_argument(
            "path",
            "This appears to be a legacy bincode-based dictionary file. Please use a dictionary compiled for the rkyv version of vibrato.",
        ));

        // legacy機能が有効な場合の処理は省略...
    } else if !magic.starts_with(MODEL_MAGIC) {            // マジックバイトの検証
        return Err(VibratoError::invalid_argument(
            "path",
            "The magic number of the input model mismatches.",
        ));
    }

    let mmap = unsafe { Mmap::map(&file)? };               // ★ファイル全体をメモリマップ（OSによる遅延ロード）

    let Some(data_bytes) = &mmap.get(DATA_START..) else {  // データ部分の開始位置を取得
        return Err(VibratoError::invalid_argument(
            "path",
            "Dictionary file too small or corrupted.",
        ));
    };

    let archived = unsafe { access_unchecked::<ArchivedDictionaryInner>(data_bytes) };  // ★ゼロコピーアクセス（検証なし）
    let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };   // ★生ポインタを静的参照に変換
    Ok(
        Self::Archived(
            ArchivedDictionary {
                _buffer: DictBuffer::Mmap(mmap),           // mmapを保持してライフタイムを管理
                data,                                      // アーカイブされたデータへの参照
            }
        )
    )
}
```

**この実装の画期的な点:**

1. **メモリマップ (`mmap`)**:
   - ファイル全体を仮想メモリ空間にマップ
   - 実際のディスクI/Oは、アクセス時にOSが自動的にページ単位で実行（デマンドページング）
   - 700MBのファイルでも、マップ自体は瞬時に完了

2. **`access_unchecked` によるゼロコピー**:
   - バイト列を `ArchivedDictionaryInner` として **そのまま解釈**
   - データのコピーやパースが一切不要
   - ポインタキャストのみで完了（マイクロ秒オーダー）

3. **静的ライフタイム**:
   - `data: &'static ArchivedDictionaryInner` として参照を保持
   - `_buffer` フィールドで `Mmap` を所有し、ライフタイムを管理
   - ドロップ時に自動的にアンマップ

### 2.2 安全性とパフォーマンスのトレードオフ

**検証付きロード: `from_path`**

```rust
// vibrato/src/dictionary.rs:538-654 (抜粋)

pub fn from_path<P: AsRef<std::path::Path>>(path: P, mode: LoadMode) -> Result<Self> {
    let path = path.as_ref();
    let mut file = File::open(path).map_err(|e| {          // ファイルを開く
        VibratoError::invalid_argument("path", format!("Failed to open dictionary file: {}", e))
    })?;
    let meta = &file.metadata()?;                          // ファイルメタデータを取得
    let mut magic = [0u8; MODEL_MAGIC_LEN];
    file.read_exact(&mut magic)?;                          // マジックバイトを読み込み

    // ... マジックバイト検証とlegacy対応は省略 ...

    let mmap = unsafe { Mmap::map(&file)? };               // ファイルをメモリマップ

    let Some(data_bytes) = &mmap.get(DATA_START..) else {  // データ部分を取得
        return Err(VibratoError::invalid_argument(
            "path",
            "Dictionary file too small or corrupted.",
        ));
    };

    let current_hash = compute_metadata_hash(meta);        // ★ファイルメタデータからハッシュを計算
    let hash_name = format!("{}.sha256", current_hash);
    let hash_path = path.parent().unwrap().join(".cache").join(&hash_name);

    if mode == LoadMode::TrustCache
        && hash_path.exists() {                            // ★ローカルキャッシュの検証ファイルをチェック
            let archived = unsafe { access_unchecked::<ArchivedDictionaryInner>(data_bytes) };  // ★検証スキップ
            let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
            return {
                Ok(
                    Dictionary::Archived(ArchivedDictionary { _buffer: DictBuffer::Mmap(mmap), data })
                )
            };
        }

    let global_cache_dir = GLOBAL_CACHE_DIR.as_ref().ok_or_else(|| {  // グローバルキャッシュディレクトリを取得
        VibratoError::invalid_state("Could not determine system cache directory.", "")
    })?;

    let hash_path = global_cache_dir.join(&hash_name);

    if mode == LoadMode::TrustCache
        && hash_path.exists() {                            // ★グローバルキャッシュの検証ファイルをチェック
            let archived = unsafe { access_unchecked::<ArchivedDictionaryInner>(data_bytes) };  // ★検証スキップ
            let data: &'static ArchivedDictionaryInner = unsafe { &*(archived as *const _) };
            return {
                Ok(
                    Dictionary::Archived(ArchivedDictionary { _buffer: DictBuffer::Mmap(mmap), data })
                )
            };
        }

    match access::<ArchivedDictionaryInner, Error>(data_bytes) {  // ★検証ファイルがない場合はフル検証
        Ok(archived) => {
            if mode == LoadMode::TrustCache {
                create_dir_all(global_cache_dir)?;
                File::create_new(hash_path)?;              // ★検証成功後、キャッシュファイルを作成
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
            // mmap がアライメント要件を満たさない場合のフォールバック
            let mut aligned_bytes = AlignedVec::with_capacity(data_bytes.len());
            aligned_bytes.extend_from_slice(data_bytes);   // アラインされたバッファにコピー

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
```

**キャッシュメカニズムの仕組み:**

1. **メタデータハッシュの計算**:
   ```rust
   // vibrato/src/dictionary.rs:1200-1261 (抜粋)

   #[inline(always)]
   pub(crate) fn compute_metadata_hash(meta: &Metadata) -> String {
       let mut hasher = Sha256::new();
       #[cfg(unix)]
       {
           use std::os::unix::fs::MetadataExt;
           hasher.update(meta.dev().to_le_bytes());      // デバイスID
           hasher.update(meta.ino().to_le_bytes());      // inode番号
           hasher.update(meta.size().to_le_bytes());     // ファイルサイズ
           hasher.update(meta.mtime().to_le_bytes());    // 変更時刻
           hasher.update(meta.mtime_nsec().to_le_bytes()); // 変更時刻（ナノ秒）
       }
       // ... Windows/その他プラットフォームの実装 ...
       hex::encode(hasher.finalize())                    // SHA256ハッシュを16進数文字列に変換
   }
   ```

2. **2段階のキャッシュ検索**:
   - **ローカルキャッシュ**: `辞書ファイルの親ディレクトリ/.cache/<hash>.sha256`
   - **グローバルキャッシュ**: `~/.cache/vibrato-rkyv/<hash>.sha256`

3. **キャッシュヒット時の動作**:
   - `.sha256` ファイルが存在 = この辞書は過去に検証済み
   - `access_unchecked` を使用して検証をスキップ
   - マイクロ秒オーダーでロード完了

4. **キャッシュミス時の動作**:
   - `access` (検証付き) でデータの整合性を確認
   - 検証成功後、グローバルキャッシュに `.sha256` ファイルを作成
   - 次回以降は高速ロードが可能に

---

## 3. データ構造のアーカイブ化

### 3.1 rkyv によるアーカイブ版の自動生成

rkyv の `Archive` トレイトを derive することで、各データ構造のアーカイブ版が自動生成されます。

**辞書の主要データ構造:**

```rust
// vibrato/src/dictionary/lexicon.rs:22-29

#[derive(Archive, Serialize, Deserialize)]                // rkyv のderive マクロ
pub struct Lexicon {
    map: WordMap,                                          // 単語マップ（Trie構造）
    params: WordParams,                                    // 単語パラメータ（コスト、接続ID）
    features: WordFeatures,                                // 単語の素性文字列
    lex_type: LexType,                                     // 辞書タイプ（システム/ユーザー/未知語）
}
```

rkyv は以下を自動生成します：
- `ArchivedLexicon`: ディスク上のバイト列レイアウトを表現する型
- `LexiconResolver`: シリアライゼーション時の相対ポインタ解決用

**コネクタの例:**

```rust
// vibrato/src/dictionary/connector.rs:32-37

#[derive(Archive, Serialize, Deserialize)]
pub enum ConnectorWrapper {
    Matrix(MatrixConnector),                               // 行列形式の接続コスト
    Raw(RawConnector),                                     // 生の接続コスト
    Dual(DualConnector),                                   // デュアル形式
}
```

### 3.2 アーカイブ版でのアクセス

アーカイブ版の型は、元の型と同じインターフェースを提供するよう実装されています。

```rust
// vibrato/src/dictionary/connector.rs:66-81

impl ConnectorView for ArchivedConnectorWrapper {          // アーカイブ版でもトレイトを実装
    fn num_left(&self) -> usize {
        match self {
            Self::Matrix(c) => c.num_left(),               // アーカイブ版のMatrixConnector
            Self::Raw(c) => c.num_left(),                  // アーカイブ版のRawConnector
            Self::Dual(c) => c.num_left(),                 // アーカイブ版のDualConnector
        }
    }
    fn num_right(&self) -> usize {
        match self {
            Self::Matrix(c) => c.num_right(),
            Self::Raw(c) => c.num_right(),
            Self::Dual(c) => c.num_right(),
        }
    }
}
```

**重要な実装パターン:**

```rust
// vibrato/src/dictionary.rs:1285-1325 (抜粋)

impl ArchivedDictionaryInner {
    #[inline(always)]
    pub(crate) fn connector(&self) -> &ArchivedConnectorWrapper {  // アーカイブ版のコネクタを返す
        &self.connector
    }
    #[inline(always)]
    pub(crate) fn system_lexicon(&self) -> &ArchivedLexicon {      // アーカイブ版のlexiconを返す
        &self.system_lexicon
    }
    #[inline(always)]
    pub(crate) fn char_prop(&self) -> &ArchivedCharProperty {      // アーカイブ版のcharプロパティを返す
        &self.char_prop
    }
    #[inline(always)]
    pub(crate) fn unk_handler(&self) -> &ArchivedUnkHandler {      // アーカイブ版の未知語ハンドラを返す
        &self.unk_handler
    }
    #[inline(always)]
    pub(crate) fn word_param(&self, word_idx: WordIdx) -> WordParam {
        match word_idx.lex_type {
            LexType::System => self.system_lexicon().word_param(word_idx),
            LexType::User => self.user_lexicon().as_ref().unwrap().word_param(word_idx),
            LexType::Unknown => self.unk_handler().word_param(word_idx),
        }
    }
}
```

---

## 4. シリアライゼーション（辞書の書き込み）

### 4.1 rkyv 形式での書き込み

```rust
// vibrato/src/dictionary.rs:302-321

pub fn write<W>(&self, mut wtr: W) -> Result<()>
where
    W: Write,
{
    wtr.write_all(MODEL_MAGIC)?;                           // ★マジックバイトを書き込み

    let padding_bytes = vec![0xFF; PADDING_LEN];
    wtr.write_all(&padding_bytes)?;                        // ★アラインメント用のパディング

    with_arena(|arena: &mut Arena| {                       // rkyv のアリーナアロケータを使用
        let writer = IoWriter::new(&mut wtr);              // I/Oライターを作成
        let mut serializer = Serializer::new(writer, arena.acquire(), Share::new());  // シリアライザーを構成
        serialize_using::<_, rkyv::rancor::Error>(self, &mut serializer)  // ★シリアライズ実行
    })
    .map_err(|e| {
        VibratoError::invalid_state("rkyv serialization failed".to_string(), e.to_string())
    })?;

    Ok(())
}
```

**ファイルフォーマット:**

```
┌─────────────────────────────────┐
│ マジックバイト (26 bytes)        │  "VibratoTokenizerRkyv 0.6\n"
├─────────────────────────────────┤
│ パディング (6 bytes)             │  0xFF × 6 (16バイトアラインメント用)
├─────────────────────────────────┤
│ rkyv アーカイブデータ            │  ゼロコピーで読めるバイナリ形式
│ - システム辞書                   │
│ - ユーザー辞書 (オプション)      │
│ - 接続コスト行列                 │
│ - 文字プロパティ                 │
│ - 未知語ハンドラ                 │
│ - その他メタデータ               │
└─────────────────────────────────┘
```

**アラインメントの重要性:**

```rust
// vibrato/src/dictionary.rs:51-56

pub const MODEL_MAGIC: &[u8] = b"VibratoTokenizerRkyv 0.6\n";

const MODEL_MAGIC_LEN: usize = MODEL_MAGIC.len();          // 26 bytes
const RKYV_ALIGNMENT: usize = 16;                          // rkyv は16バイトアラインメントを要求
const PADDING_LEN: usize = (RKYV_ALIGNMENT - (MODEL_MAGIC_LEN % RKYV_ALIGNMENT)) % RKYV_ALIGNMENT;  // 6 bytes
const DATA_START: usize = MODEL_MAGIC_LEN + PADDING_LEN;   // 32 bytes (16の倍数)
```

---

## 5. Zstd圧縮辞書の高速ロード

### 5.1 キャッシュ機構による最適化

```rust
// vibrato/src/dictionary.rs:855-996 (抜粋・簡略化)

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
    let zstd_file = File::open(zstd_path)?;                // 圧縮ファイルを開く
    let meta = zstd_file.metadata()?;                      // メタデータを取得

    let dict_hash = compute_metadata_hash(&meta);          // ★圧縮ファイルのメタデータからハッシュを計算
    let decompressed_dir = cache_dir.as_ref().to_path_buf();

    let decompressed_dict_path = decompressed_dir.join(format!("{}.dic", dict_hash));  // キャッシュファイルのパス

    if decompressed_dict_path.exists() {                   // ★キャッシュが存在する場合
        return Self::from_path(decompressed_dict_path, LoadMode::TrustCache);  // ★キャッシュから超高速ロード
    }

    // キャッシュが存在しない場合: 解凍処理
    if !decompressed_dir.exists() {
        create_dir_all(&decompressed_dir)?;                // キャッシュディレクトリを作成
    }

    let mut temp_file = tempfile::NamedTempFile::new_in(&decompressed_dir)?;  // 一時ファイルを作成

    {
        let mut decoder = zstd::Decoder::new(zstd_file)?;  // Zstd デコーダーを作成

        io::copy(&mut decoder, &mut temp_file)?;           // ★解凍してテンポラリファイルに書き込み
        temp_file.as_file().sync_all()?;                  // ディスクに同期
    }
    temp_file.seek(SeekFrom::Start(0))?;                   // ファイルの先頭に戻る

    let mut magic = [0; MODEL_MAGIC_LEN];
    temp_file.read_exact(&mut magic)?;                     // マジックバイトを確認

    // ... マジックバイト検証とlegacy処理 ...

    temp_file.seek(SeekFrom::Start(0))?;

    let mut data_bytes = Vec::new();
    temp_file.as_file_mut().read_to_end(&mut data_bytes)?;  // 全データを読み込み

    let mut aligned_bytes: AlignedVec = AlignedVec::with_capacity(data_bytes.len());
    aligned_bytes.extend_from_slice(&data_bytes);          // アラインされたバッファにコピー

    let Some(data_bytes) = &aligned_bytes.get(DATA_START..) else {
        return Err(VibratoError::invalid_argument(
            "path",
            "Dictionary file too small or corrupted.",
        ));
    };

    let _ = access::<ArchivedDictionaryInner, Error>(data_bytes).map_err(|e| {  // ★検証
        VibratoError::invalid_state(
            "rkyv validation failed. The dictionary file may be corrupted or incompatible."
                .to_string(),
            e.to_string(),
        )
    })?;

    temp_file.persist(&decompressed_dict_path)?;           // ★一時ファイルをキャッシュファイルとして永続化

    let decompressed_dict_hash = compute_metadata_hash(&File::open(&decompressed_dict_path)?.metadata()?);
    let decompressed_dict_hash_path = decompressed_dir.join(format!("{}.sha256", decompressed_dict_hash));

    File::create_new(decompressed_dict_hash_path)?;        // ★検証ファイルを作成

    Self::from_path(decompressed_dict_path, LoadMode::TrustCache)  // ★キャッシュからロード
}
```

**処理フロー:**

1. **初回実行**:
   - `.dic.zst` ファイルを解凍
   - キャッシュディレクトリに `.dic` として保存
   - 検証ファイル `.sha256` を作成
   - 解凍されたファイルからロード（~1.3秒）

2. **2回目以降**:
   - キャッシュファイル `.dic` が存在を確認
   - `from_path` でメモリマップロード
   - `.sha256` が存在するため検証をスキップ
   - マイクロ秒オーダーで完了（~6.5µs）

---

## 6. 従来実装との詳細比較

### 6.1 メモリレイアウトとアクセスパターン

**従来の bincode (legacy):**

```
┌──────────────┐
│ ディスク     │
│ (bincode)    │
└──────┬───────┘
       │ read_exact()
       │ 全データ読み込み
       ↓
┌──────────────┐
│ バッファ     │
│ (Vec<u8>)    │
└──────┬───────┘
       │ bincode::decode
       │ パース＋変換
       ↓
┌──────────────┐
│ メモリ       │
│ (構造体)     │  ← 新しくアロケーション
└──────────────┘
```

- **コピー回数**: 2回（ディスク→バッファ、バッファ→構造体）
- **CPU時間**: 全データのパース＋変換が必要
- **メモリ使用量**: ファイルサイズの2倍以上（バッファ＋構造体）

**新しい rkyv:**

```
┌──────────────┐
│ ディスク     │
│ (rkyv)       │
└──────┬───────┘
       │ mmap()
       │ 仮想メモリマップ
       ↓
┌──────────────┐
│ 仮想メモリ   │  ← ディスクと直接マップ
│ (mmap)       │     実データは未ロード
└──────┬───────┘
       │ access_unchecked()
       │ ポインタキャストのみ
       ↓
┌──────────────┐
│ &Archived    │  ← 参照を取得（コピーなし）
└──────────────┘
```

- **コピー回数**: 0回（ゼロコピー）
- **CPU時間**: ポインタキャストのみ（ナノ秒オーダー）
- **メモリ使用量**: ファイルサイズと同じ（OS がページング管理）

### 6.2 データアクセス時の違い

**従来 (bincode):**

```rust
// legacy実装では、全データがメモリ上に展開済み
let lexicon: &Lexicon = &dict.data.system_lexicon;        // 通常のメモリアクセス
let word_param = lexicon.params.get(word_id);             // Vec<T>からの取得
```

**新実装 (rkyv):**

```rust
// rkyv実装では、アーカイブ版の参照を取得
let lexicon: &ArchivedLexicon = dict.system_lexicon();    // mmapされた領域への参照
let word_param = lexicon.word_param(word_idx);            // ページフォルトが発生する可能性あり
```

**デマンドページングの効果:**
- 初回アクセス時にOSがページをロード（ページフォルト）
- 2回目以降はページキャッシュから高速アクセス
- 使用しない部分はメモリにロードされない（メモリ効率的）

---

## 7. ベンチマーク実装の比較

### 7.1 従来実装のベンチマーク

```rust
// vibrato/benches/vibrato_init.rs:83-89

group.bench_function("vibrato/warm", |b| {
    let mut rdr = File::open(&dict_path).unwrap();         // ファイルを開く
    b.iter(|| {
        rdr.seek(SeekFrom::Start(0)).unwrap();             // ファイルの先頭に戻る
        std::hint::black_box(vibrato::Dictionary::read(&rdr).unwrap());  // ★毎回フルデシリアライズ
    })
});
```

### 7.2 新実装のベンチマーク

```rust
// vibrato/benches/vibrato_rkyv_init.rs:104-108

group.bench_function("vibrato-rkyv/from_path/warm", |b| {
    let _ = fs::read(&ctx.dict_path).unwrap();             // OSページキャッシュにロード
    let _ = Dictionary::from_path(&ctx.dict_path, LoadMode::TrustCache).unwrap();  // キャッシュファイル作成
    b.iter(|| std::hint::black_box(Dictionary::from_path(&ctx.dict_path, LoadMode::TrustCache).unwrap()))  // ★超高速ロード
});

group.bench_function("vibrato-rkyv/from_path_unchecked/warm", |b| {
    let _ = fs::read(&ctx.dict_path).unwrap();             // OSページキャッシュにロード
    let _ = Dictionary::from_path(&ctx.dict_path, LoadMode::TrustCache).unwrap();  // キャッシュファイル作成
    b.iter(|| std::hint::black_box(unsafe { Dictionary::from_path_unchecked(&ctx.dict_path) }.unwrap()))  // ★最速
});
```

---

## 8. 高速化の要因分析

### 8.1 I/O パターンの最適化

**従来実装:**
```
read() システムコール × 多数
  ↓
カーネル空間 → ユーザー空間へのコピー
  ↓
バッファの確保とコピー
  ↓
バイト列のパース
  ↓
構造体の構築
```

**新実装:**
```
mmap() システムコール × 1
  ↓
仮想メモリマッピング（実データロードなし）
  ↓
ポインタキャスト
  ↓
完了（アクセス時にページフォルト → OSが自動ロード）
```

### 8.2 キャッシュ効率

**メタデータハッシュの利点:**
- ファイル内容ではなく、メタデータ（inode、mtime等）からハッシュ計算
- ハッシュ計算が極めて高速（ファイル読み込み不要）
- `.sha256` ファイルの存在チェックのみで検証済みかを判定

**2段階キャッシュの利点:**
1. **ローカルキャッシュ**:
   - 辞書ファイルと同じディレクトリ
   - ポータブル（辞書と一緒に移動可能）

2. **グローバルキャッシュ**:
   - `~/.cache/vibrato-rkyv/`
   - 読み取り専用の場所に辞書がある場合も対応
   - システム全体で共有

---

## 9. 他プロジェクトへの展開可能性

### 9.1 適用可能なケース

`rkyv` によるゼロコピーデシリアライゼーションは、以下のような場合に有効です：

1. **大きな辞書・データベースを持つアプリケーション**
   - 機械学習モデル（重み行列、埋め込みベクトル）
   - 地理情報データ（地図、座標データ）
   - 辞書・ナレッジベース（翻訳辞書、知識グラフ）

2. **頻繁な起動・停止が発生するツール**
   - CLI ツール
   - サーバーレス関数（AWS Lambda等）
   - マイクロサービス

3. **メモリ効率が重要な環境**
   - 組み込みシステム
   - モバイルアプリケーション
   - コンテナ環境

### 9.2 実装のポイント

**データ構造の設計:**
```rust
#[derive(Archive, Serialize, Deserialize)]
pub struct YourData {
    // ポインタを含まない、シリアライズ可能な型のみ
    field1: Vec<u8>,              // OK
    field2: String,               // OK
    field3: HashMap<K, V>,        // OK (rkyv feature="hashbrown"が必要)
    // field4: Box<dyn Trait>,    // NG: トレイトオブジェクトは不可
}
```

**ファイルフォーマット:**
```rust
const MAGIC: &[u8] = b"YourApp 1.0\n";
const ALIGNMENT: usize = 16;  // rkyv の要求に応じて調整

pub fn write<W: Write>(data: &YourData, mut wtr: W) -> Result<()> {
    wtr.write_all(MAGIC)?;
    // アラインメント用のパディング
    let padding_len = (ALIGNMENT - (MAGIC.len() % ALIGNMENT)) % ALIGNMENT;
    wtr.write_all(&vec![0; padding_len])?;

    // rkyv でシリアライズ
    with_arena(|arena| {
        let writer = IoWriter::new(&mut wtr);
        let mut serializer = Serializer::new(writer, arena.acquire(), Share::new());
        serialize_using(data, &mut serializer)
    })?;
    Ok(())
}

pub fn read(path: impl AsRef<Path>) -> Result<&'static ArchivedYourData> {
    let file = File::open(path)?;
    let mmap = unsafe { Mmap::map(&file)? };

    // マジックバイト＋パディングをスキップ
    let data_start = MAGIC.len() + padding_len;
    let data_bytes = &mmap[data_start..];

    // ゼロコピーアクセス
    let archived = unsafe { access_unchecked::<ArchivedYourData>(data_bytes) };
    Ok(unsafe { &*(archived as *const _) })
}
```

**キャッシュ機構の実装:**
```rust
use sha2::{Digest, Sha256};

fn compute_file_hash(meta: &Metadata) -> String {
    let mut hasher = Sha256::new();
    // プラットフォーム固有のメタデータを使用
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hasher.update(meta.ino().to_le_bytes());
        hasher.update(meta.size().to_le_bytes());
        hasher.update(meta.mtime().to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

pub fn read_with_cache(path: impl AsRef<Path>) -> Result<YourData> {
    let path = path.as_ref();
    let meta = fs::metadata(path)?;
    let hash = compute_file_hash(&meta);

    let cache_path = cache_dir()?.join(format!("{}.verified", hash));

    if cache_path.exists() {
        // 検証済み: 高速パス
        return read_unchecked(path);
    }

    // 未検証: 検証してからキャッシュ
    let data = read_with_validation(path)?;
    File::create(cache_path)?;  // 検証ファイル作成
    Ok(data)
}
```

### 9.3 注意点と制約

1. **データ構造の制約**
   - `rkyv` は全てのRust型をサポートしているわけではない
   - トレイトオブジェクト、関数ポインタなどは扱えない
   - カスタム型には `Archive` の手動実装が必要な場合がある

2. **バージョニング**
   - データ構造の変更に対する後方互換性の管理が重要
   - マジックバイトにバージョン情報を含める
   - 構造変更時は新しいバージョンとして扱う

3. **安全性**
   - `access_unchecked` は unsafe
   - 破損したファイルを読むと未定義動作の可能性
   - 本番環境では検証機構を実装すべき

4. **メモリアラインメント**
   - 全てのプラットフォームでアラインメント要件が満たされるとは限らない
   - フォールバック処理（AlignedVec へのコピー）を用意すべき

---

## 10. まとめ

### 10.1 高速化の核心

`vibrato-rkyv` の劇的な高速化は、以下の3つの技術の組み合わせで実現されています：

1. **ゼロコピーデシリアライゼーション (rkyv)**
   - バイト列をそのままデータ構造として扱う
   - パース・変換処理が不要
   - メモリコピーなし

2. **メモリマップドI/O (mmap)**
   - ファイルを仮想メモリにマップ
   - OSによるデマンドページング
   - 実際の読み込みは最小限

3. **メタデータベースのキャッシュ機構**
   - ファイルハッシュではなくメタデータハッシュ
   - 検証ファイルによる高速な検証済み判定
   - 2段階キャッシュによる柔軟な対応

### 10.2 パフォーマンス数値の再確認

| シナリオ | 従来 | 新実装 | 高速化 |
|:---------|:-----|:-------|:-------|
| 非圧縮・ウォーム・検証なし | 34秒 | 2.9µs | **11,700,000倍** |
| 非圧縮・ウォーム・安全 | 34秒 | 4.1µs | **8,300,000倍** |
| 非圧縮・コールド・安全 | 42秒 | 1.1ms | **38,000倍** |
| 圧縮・2回目以降 | 4.5秒 | 6.5µs | **700,000倍** |

### 10.3 影響範囲

このアーキテクチャ変更により、以下が可能になりました：

- **CLI ツールの起動時間**: ほぼゼロ
- **サーバーアプリケーション**: 再起動時のダウンタイム最小化
- **マイクロサービス**: コールドスタート時間の大幅短縮
- **開発体験**: テストやデバッグのイテレーション高速化

### 10.4 今後の展開

`rkyv` によるゼロコピーデシリアライゼーションは、Rust エコシステムにおいて大規模データを扱うアプリケーションのデファクトスタンダードになる可能性があります。本実装は、その実践的な応用例として、他のプロジェクトにも参考になるでしょう。
