# Windows パス対応表（検出パターンの OS 別マッピング）

> DiskSage の検出パターンを Windows に広げるための**引き継ぎ資産**。
> macOS と Windows ではツールのデータ置き場が違うため、ここで対応を一覧化する。
> 記述は**公式/一次ソースで確認したものだけ ✅**、不確実は **⚠️要検証** と明示する（推測を断定で書かない）。

最終更新: 2026-11-22

---

## 大前提：`~/...` パターンは Rust 無改修で Windows でも解決される（コード確認済み ✅）

エンジンのパス展開は `engine/src/patterns.rs` の `expand()`：

```rust
fn expand(home: &Path, p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),   // "~/" を home に連結
        None => PathBuf::from(p),
    }
}
```

- `home_dir()`（`engine/src/util.rs`）は **Windows で `%USERPROFILE%`** を返す（`#[cfg(windows)] std::env::var_os("USERPROFILE")`）。
- Rust の `Path::join` は **Windows でも `/` を区切りとして受け付ける**。

∴ `~/AppData/Local/npm-cache/_cacache` は Windows で `C:\Users\<user>\AppData\Local\npm-cache\_cacache` に解決される。
**builtin.json に Windows パスを足すだけでよく、Rust コードの変更は不要**（＝パターンの JSON 外出し #67 の成果）。

---

## A. 既に Windows でも動くパターン（`~/.foo` 形式＝全OS共通）

変更不要。mac と同じパスが Windows の `%USERPROFILE%\.foo` に一致する。

| パターン | パス | 確度・ソース |
|---|---|---|
| `ollama_models` | `~/.ollama/models` | ✅ [Ollama FAQ](https://docs.ollama.com/faq)（全OSで `$HOME/.ollama/models`・`OLLAMA_MODELS` で変更可） |
| `android_avd` | `~/.android/avd` | ✅ [Android docs](https://developer.android.com/tools/variables)（`$HOME/.android/avd`） |
| `gradle_cache` | `~/.gradle` | ✅ Gradle は全OSで `GRADLE_USER_HOME` 既定＝`<home>/.gradle`（旧 ⚠️要検証 → 解消） |
| `user_cache` | `~/.cache` | ⚠️ Unix 規約。Windows のツールは `~/.cache` を使わないものが多い（HuggingFace 等は使う）＝部分対応 |

---

## B. Windows 専用パスの追加が要るパターン（mac が `~/Library` や `~/.npm` で分岐）

本 PR（feature/20261122-windows-paths）で builtin.json に Windows パスを追加。全て **report のみ**（DELETABLE 非対象）＝ワンクリック削除しないので誤削除リスクなし。

| パターン | mac/linux（既存） | 追加する Windows パス | 確度・ソース |
|---|---|---|---|
| `npm_cache` | `~/.npm/_cacache` | `~/AppData/Local/npm-cache/_cacache` | ✅ [npm folders](https://docs.npmjs.com/cli/v10/configuring-npm/folders)（キャッシュ根＝`%LocalAppData%/npm-cache`）。`_cacache` サブフォルダは cacache モジュールの全OS共通規約（folders ページ自体には記載なし＝軽微な注記） |
| `pnpm_store` | `~/Library/pnpm/store`, `~/.local/share/pnpm/store` | `~/AppData/Local/pnpm/store` | ✅ [pnpm store](https://pnpm.io/settings/store#storedir)（公式がリテラル `~` パスで明記）。`$PNPM_HOME`/`$XDG_DATA_HOME` が先だが env 展開は未対応＝既定ケースのみ検出 |
| `android_sdk` | `~/Library/Android/sdk`, `~/Android/Sdk` | `~/AppData/Local/Android/Sdk` | ✅ [studio-config](https://developer.android.com/studio/intro/studio-config)（「located at `%USERPROFILE%\AppData\Local\Android\SDK` by default」と明記。Windows FS は大小無視なので `Sdk`/`SDK` どちらでも一致） |

---

## C. Windows ネイティブの大食い（mac に無い＝将来の新パターン候補・未実装）

Windows 実機で棚卸しして必要性を判断する。**どれも report のみ**（システム管理ファイルは提案に留め、削除は OS の正規手順へ誘導）。

| 対象 | Windows パス | 備考 | 確度 |
|---|---|---|---|
| WSL2 ディストロ VHDX | `~/AppData/Local/Packages/<Distro>/LocalState/ext4.vhdx` | 開発者で**巨大化**。Distro 名が環境依存＝走査が要る | ⚠️要検証（Distro 名パターン） |
| ページファイル | `C:\pagefile.sys` | 通常 RAM 相当。システム管理＝提案のみ（削除不可・`~` 外） | ✅ 既知 |
| ハイバネーション | `C:\hiberfil.sys` | RAM 相当。`powercfg /h off` の領域。提案のみ | ✅ 既知 |
| Windows Update キャッシュ | `C:\Windows\SoftwareDistribution\Download` | 要管理者。提案のみ（`~` 外・絶対パス） | ✅ 既知 |
| Temp | `~/AppData/Local/Temp` | 一時ファイル | ✅ 既知 |

> 注：`C:\...` の絶対パスは現状の `expand()`（`~/` 専用）では表現できない。pagefile/hiberfil/Windows Update を入れるなら **絶対パス対応か専用ロジック**が要る（＝Rust 改修が必要・B とは別工数）。

---

## D. macOS 専用（Windows では N/A）

Windows では検出不要。mac 固有の仕組み。

- `apfs_snapshots` / `vm_swap`（APFS＝mac固有。Windows は VSS/シャドウコピーで別機構）
- `xcode_derived_data` / `ios_devicesupport` / `coresimulator_caches` / `coresimulator_devices`（Xcode＝mac専用）
- `library_caches`（`~/Library/Caches`＝mac固有。Windows は `%LocalAppData%` に分散＝1:1対応なし）
- `photos_library` / `apple_mail`（mac アプリ）

---

## E. 要・個別対応（単純な dir×閾値では表現できない）

| パターン | 状況 | 次アクション |
|---|---|---|
| `iphone_backup` | mac は `scan.rs` の bespoke ロジック（Manifest.db 探索で破損判定）。Windows は `%APPDATA%\Apple Computer\MobileSync\Backup`（旧iTunes）または `%USERPROFILE%\Apple\MobileSync\Backup`（新 Apple Devices アプリ） | ⚠️要検証（Windows 実機で実パス確認）＋ `scan.rs` に Windows location 追加 |
| `claude_vm` | mac は `~/Library/Application Support/Claude/vm_bundles`。**Windows で同じ VM(`vm_bundles`/`rootfs.img`)を使うか不明**。Windows の Claude Desktop データは install 形態で分岐（直exe=`%APPDATA%\Claude` / MS Store=`%LOCALAPPDATA%\Packages\Claude_pzs8sxrjxfjjc\...`） | ⚠️要検証。**mac の VM パスを Windows に流用してはいけない**（#123 のハルシネーション教訓）。Windows 実機で vm_bundles 相当の有無を確認してから |
| `docker_raw` | mac は `Docker.raw`。Windows(WSL2) は `~/AppData/Local/Docker/wsl/` 配下の `ext4.vhdx` 等だが**版でファイル名が変動** | ⚠️要検証。サブフォルダ走査 or 版別ファイル名の特殊ロジックが要る |

---

## 確度レジェンド

- ✅ **公式/一次ソース確認済み** — 本文のリンク先で裏取り
- ⚠️ **要検証** — 一般的慣習 or 環境依存。Windows 実機 or 追加ソースで確認が必要

## ランタイム検証の切り分け（重要）

本表の「パスの正しさ」は公式ドキュメントで確認済み（✅）。ただし **Windows 実機で実際に検出が光るか**（＋ゴミ箱削除が動くか）は別物＝**Windows 実機でのランタイム検証が必要**。mac 開発環境では確認できない。

## ソース一覧

- Ollama models: https://docs.ollama.com/faq
- npm cache: https://docs.npmjs.com/cli/v10/configuring-npm/folders
- pnpm store: https://pnpm.io/settings/store#storedir
- Android SDK（既定の設置先）: https://developer.android.com/studio/intro/studio-config
- Android 環境変数: https://developer.android.com/tools/variables
- Claude Desktop データ保存先: https://claude.com/docs/third-party/claude-desktop/data-storage
