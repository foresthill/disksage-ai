# CLAUDE.md — DiskSage Project Context

> このファイルは、Claude Code がこのプロジェクトを継続開発する際の**引き継ぎ文書**です。
> ディレクトリを開いたら最初に読んでください。

---

## プロジェクト概要

**DiskSage** は、AI が提案する容量管理 CLI / アプリ。macOS・Linux (将来 Windows) 対応の OSS プロジェクト。

**3行サマリー：**
- 既存ツール（CleanMyMac 等）は「勝手に消す」か「サイズを見せるだけ」で、中間がない
- DiskSage は「AI 判定」×「提案方式（絶対に自動削除しない）」×「OSS」
- コアは Bash CLI (v0.1)、将来 Rust + Tauri GUI 化（Phase 3+）

詳しくは `docs/企画書.md` を参照。

---

## プロジェクトの誕生経緯

2026年4月20日、創業者（Foresthill）が Mac（460GB SSD）の空き容量がまた 0 近くになった問題を Claude と一緒に解剖した一日の産物。

- iMobie の **39GB 孤児化した自分の iPhone バックアップ**（Manifest.db 不在で復元不能）を発見
- VM ボリューム 22.6GB（スワップ、要再起動）
- /Library に 53GB（ホーム外の見えない占有）
- Ollama モデル、Claude Code VM、Docker.raw、電子アプリキャッシュ...

**「既存ツールはどれもこの診断をしてくれなかった」** という悔しさがプロダクトの原点。

---

## 現在のステータス（v0.1.0 MVP）

### 完成しているもの

- `disksage` — Bash CLI（Python 3 以外の依存なし）
  - `scan` — 10パターンの肥大箇所を検出、Markdown レポート生成
  - `scan --ai` — Claude API による文脈判定（BYOK、メタデータのみ送信）✅ v0.2 実装済み
  - `scan --quick` — flow-type パスをスキップ（TCC ダイアログ回避）
  - `serve` — ローカルWeb UI ✅（`disksage serve`：スキャン→`127.0.0.1:8765` でHTML配信→ブラウザ自動オープン、「再スキャン」ボタン=POST `/rescan`。Python標準ライブラリのみ・localhost限定。`cmd_serve` 内に http.server。GUI化(v0.4 Tauri)前の B1ステップ。`--ai` は非対話のため `--yes` 必須。初回/再スキャンは通常スキャン同等の時間（node_modules 大の環境で ~30s）。i18n対応＝ツールバーは DS_LANG で「再スキャン/Re-scan」）
    - **削除UI ✅**（「ぽちぽち削除」）：findings をチェック→POST `/delete`→**ゴミ箱へ移動（osascript、復元可・`rm`不使用）**→自動再スキャン。サーバは `<stamp>.findings.tsv` サイドカー（`DISKSAGE_SERVE=1` 時に `cmd_scan` が出力）の**ホワイトリスト照合**＝任意パス削除を防止。ワンクリック対象は `DELETABLE`＝iphone_backup/ollama_models/xcode_derived_data/coresimulator_caches/coresimulator_devices/ios_devicesupport（パス＝消す対象そのもの）。除外：electron_cache（パスがアプリ本体）・docker_raw（稼働中危険）・node_modules_aggregate（~/Development 誤爆）・apfs_snapshots/vm_swap。severity≠safe は赤⚠＋二重確認。全消しボタンは無し（設計原則：自動削除しない・ユーザー承認・ゴミ箱経由 に準拠）
  - `scan --html` — 自己完結HTMLレポート ✅（深刻度カラーカード・AIバッジ・ディスク使用量バー。外部依存なし・オフラインで開ける。`render_html` 内 Python、i18n対応。GUI化(v0.4)前の低コスト視覚化ステップ）。flow-type は `cmd_scan` で一度だけ計算し md/html で共有
  - レポート i18n ✅：日本語レポート全文対応。`t()`/`t_lookup()` + `CATALOG_EN`/`CATALOG_JA`（bash 3.2 互換、連想配列不使用）。見出し・各パターン説明/Action・Next Steps・AI判定欄・フッタを翻訳。CLI/help は英語のまま。日本語以外の言語コードは AI reasoning のみ翻訳し、雛形は英語フォールバック
  - **言語解決の強化 ✅（`config`＋macOSロケール自動判定）**：`ai_lang()` の優先順位を **①`DISKSAGE_LANG` env → ②config `lang=` → ③`$LC_ALL`/`$LANG` → ④macOS `AppleLocale`(defaults) → ⑤en** に。**バグ修正**: 従来は `$LANG` 未設定（GUI/serve 起動時は空が普通）だと英語に落ちていた＝日本人ユーザーが英語レポートになる問題。macOS は `AppleLocale=ja_JP` を持つのでそこから自動で `ja` に。ユーザーが明示選択できる **`disksage config lang ja`**（`$DISKSAGE_HOME/config` に key=value 保存、`ds_config_get`/`ds_config_set`/`cmd_config`）も追加。config はクロスプラットフォーム、AppleLocale は mac 限定 fallback（他OSは③まで）。実機検証：$LANG未設定でも ja 自動／config で上書き／env が最優先、を確認
  - `snapshot` — ディスク使用量記録
  - `trend` — 時系列表示
  - `reports` — 保存済みレポート一覧（時刻・`[html]`印）。各スキャンは `~/.disksage/scans/<stamp>.md`/`.html`/`.findings.tsv` として蓄積＝時系列の"断面"
  - `top [N]` — home 配下の**大きいフォルダ順**（DaisyDisk/Google Drive的）。`du -shx` で $HOME直下＋Library各subdir＋主要隠しキャッシュを集計しサイズ順表示。`cmd_top`。読み取り専用。**高速化済み**：各ルートを**並列 du**（temp file 分離→cat→sort）＝実機 ~23秒（旧: 逐次で120秒超タイムアウト）
  - `df`（別名 `usage`）— **スキャンなしの即・空き容量**（`cmd_df`）。`render_disk_usage` を再利用しコンテナ単位の「X% full」＋**ローカルスナップショット数**を表示。スナップショット>0 なら「削除済みブロックを保持＝何もしなくても空きが減る主因」と説明（`com.apple.os.update-*` は OS更新完了で自動消滅、と明記）。i18n対応。ユーザーの「df -h / の機能ないの？」＋「みるみる減る」現象への回答。実機で OS更新の準備スナップショット3個が空きを握り→解放で ~12GB 戻る挙動を実測確認（＝この現象は連続漏れでなくOS更新準備の"波"）
  - flow-type に**メディア退避アドバイス ✅**：「最近増えたファイル」の各行を拡張子で判定し、写真/動画/音声/制作ファイル（mov/mp4/heic/jpg/png/wav/logicx/psd/als 等）に `← 外付けへ退避` マーク＋フッタ注記「キャッシュは削除OK・でも写真/動画/制作ファイルは二度と戻せない→USB/外付けへコピーしてから削除」。md は `render_flow_type_section`（bash 3.2互換の `tr`+case 判定）、HTML は `render_html` 内 python `endswith(MEDIA_EXT)`＋黄色バナー。VMディスク(rootfs.img)等の非メディアは無印。カタログ `flow_archive`/`flow_media_note`。ユーザー指摘「キャッシュは削除でいいが写真・動画は退避 or 削除のアドバイスを」への対応
- スキャン高速化 ✅：flow-type(`find_flow_type_files`) は `node_modules`/`.git`/`Caches`/`.cache` を **-prune** して探索（実機 120秒超→66秒）。それらは集計パターンで別途検出済＝除外で高速化かつ blob 誤検出も解消。`top` は並列 du 化（→23秒）。実利用で判明した「勝手に増える」正体は Claude VM img(rootfs.img 10GB)・Zoom録画等＝flow-type が正しく拾う
- pattern check 並列化 ✅：`cmd_scan` の check_* を**バックグラウンド並列実行**（各 `$cdir/<name>` に出力→`wait`→cat）。library_caches(33GBの du)等が重く逐次だと --quick でも ~46秒かかっていたのを **~22秒**に短縮。wall-time≈最遅チェック。desktop の初回ロード遅延を解消するため導入
  - `help` / `version`
- 検出パターン追記 ✅：`library_caches`（~/Library/Caches 集計 >5GB・safe）/ `user_cache`（~/.cache 集計 >5GB・safe）。実利用診断で判明した「最大の犯人＝キャッシュ33G/18G」を今まで見逃していたのを塞いだ（`electron_cache` はアプリ個別のみで集計を拾えていなかった）。action は tool純正clean（brew cleanup / yarn cache clean / uv cache clean）と `disksage top` へ誘導。削除UI(DELETABLE)には**未追加**（コンテナ丸ごと削除は大ハンマー・稼働アプリ影響のため report のみ）
- serve UX刷新 ✅（progressive + 左サイドバー + レポート履歴画面）：serve は**ポート即bind＋スキャンをバックグラウンド**（`trigger_scan`/`scan_state`）。レポート未完成の間は `overview_page()`＝ディスク使用量バー（`disk_usage_html`）＋「スキャン中」＋2秒自動リフレッシュ→完了で findings に自動切替（`scan_state.active` 中は report にも meta refresh 注入）。全ページに**左サイドバー**（`sidebar()`：🔍スキャン=`/` / 📁レポート=`/reports` / **⚙️設定=`/settings`**、`SHIFT_STYLE` で本文200px右寄せ）。`/reports`=`reports_page()` 履歴一覧画面（クリックで `/?report=<stamp>`）。/rescan・/delete も `trigger_scan()` で非ブロッキング化。ESET的ダッシュボード志向の第一歩。ユーザーUX指摘「初回22秒待ちで離脱／履歴を画面で見たい」への対応
- **設定画面 ✅（`/settings`）**：`settings_page()`＝レポート言語の選択（自動/日本語/English、ラジオ）＋データ保存先表示＋「自動削除しない」明示。保存は POST `/settings`→`set_config_lang()` が `$DISKSAGE_HOME/config` の `lang=` を書換（値は en/ja/空 にホワイトリスト）→`?saved=1` で✅バナー。今日の `disksage config lang` と同じ config を GUI から操作＝CLI/GUI 一貫。**ユーザー指摘「以前サイドメニューにレポートと設定がある構成を伝えたのに、レポートしか出ていない」への対応**（サイドバーは既にあったが設定画面が未実装だった）。制約: 実行中 serve の UI 言語（SERVE_LANG）は起動時固定＝言語切替は新スキャン/再起動で全反映（画面に明記）
- serve レポート履歴 ✅：serve のツールバーに履歴 `<select>`（新しい順・「最新」タグ）。選ぶと `GET /?report=<stamp>` で過去レポート表示（stamp はサーバ側 `all_reports()` のホワイトリスト照合＝traversal防止）。過去レポートは読み取り専用（黄色バナー＋最新へ戻るリンク、削除パネル無し）。最新のみ削除パネル表示。`page(report_param)` / `toolbar()` / `old_banner()` / `friendly()`
- ディスク使用量表示 ✅改善：APFS は複数ボリュームが1コンテナの空きを共有するため、`df` の個別%は誤解を生む。`render_disk_usage`(md)/`render_html` を**サイズ(=コンテナ)でグルーピング**し、コンテナ単位で「使用/全体/実%/空き」を1本のバー＋ボリューム内訳（used順）で表示（`df -Pk` で数値取得、used=size-avail）。「起動ディスク」ラベルは `/` を含むコンテナ。i18n: lbl_startup/used/free/disk_volnote
- `desktop/` — **Tauri v2 デスクトップアプリ雛形 ✅（案A: serve のWeb UIをネイティブ窓で表示）**。`create-tauri-app`(vanilla) 生成→改変。`src-tauri/src/lib.rs` が起動時に `disksage serve`（`DISKSAGE_NO_BROWSER=1` で二重ブラウザ抑止）を spawn、終了時 kill。`src/index.html` は `127.0.0.1:8765` を polling→到達で遷移。**未ビルド（初回 tauri build は ~1-2GB DL・10分超のため保留、設定はJSON検証済）**。将来はネイティブUI(案B/v0.4)へ。`target/`/`node_modules` は gitignore 済
- **macOS リリースCI ⚙️（`.github/workflows/desktop-macos.yml`・設定済/CI未実行）**：tag `v*` push で Tauri app を **universal .dmg** ビルド→draft Release に添付。手動 dispatch はビルドのみ＝artifact アップロード。`tauri-apps/tauri-action@v1`（2026-06実在確認）＋ `checkout@v7`/`setup-bun@v2`/`rust-cache@v2`/`dtolnay/rust-toolchain@stable`（両arch target）。公式手順(tauri.app/distribute/pipelines/github) 準拠。**ローカルで Tauri ビルド不可のため初回CI実行で要検証**。**Win/Linux はエンジンが macOS専用(bash: tmutil/osascript)のため意図的に対象外** → Phase B「Rustエンジン化」後に拡張。残: 配布 .dmg を自己完結にするため `disksage` を Tauri resource に同梱（現状は殻がPATH/既知パスから解決＝要インストール）
- `scripts/make-app.sh` — ダブルクリック起動の `DiskSage.app`（macOS）生成 ✅。薄いランチャ＝Terminalで `disksage serve` を起動→ブラウザUI。ネイティブ(Rust/Tauri v0.4)ではなく低コストの「.app化」ステップ。`.app` は成果物なので gitignore（コミットしない）。CLIパスをビルド時に埋め込み＋実行時 `command -v disksage` フォールバック
- README.md（OSSリリース品質）
- LICENSE（Apache-2.0）
- CONTRIBUTING.md、CI（GitHub Actions: bash 構文 + ShellCheck）

### 検出パターン（v0.1）

| ID | 対象 | Severity |
|---|---|---|
| `apfs_snapshots` | APFS スナップショット > 3個 | high |
| `iphone_backup` | iPhone バックアップ（Manifest.db 有無で破損判定） | high/medium |
| `docker_raw` | Docker.raw > 10GB | medium |
| `ollama_models` | Ollama モデル > 10GB | medium |
| `node_modules_aggregate` | `~/Development/` 配下合計 > 10GB | low |
| `vm_swap` | macOS スワップ > 5GB | medium |
| `electron_cache` | Electron 系 Cache/Code Cache/GPUCache > 500MB | safe |
| `xcode_derived_data` | Xcode DerivedData > 5GB | safe |
| `coresimulator_caches` | CoreSimulator/Caches > 1GB | safe |
| `coresimulator_devices` | CoreSimulator/Devices > 5GB（削除でシミュレータ状態消失） | medium |
| `ios_devicesupport` | Xcode iOS DeviceSupport > 3GB（接続時に再DL） | safe |
| `flow_type` | 30日以内に作成の 500MB 超ファイル（レポート欄） | info |

---

## 技術方針

### 現 Phase（0.1 / 0.2）: Bash で書く

- 理由：依存最小、ロジック固定のために手早く書く、OSS 受け入れやすい
- Python 3 は macOS 標準で入ってるのでJSONの読み書きに使用

### Phase B（進行中）: Rust エンジン化 PoC

- `engine/` — **Rust クロスプラットフォームエンジンの PoC ✅（ローカルビルド＆実行検証済）**。`disksage-engine df` が bash の `disksage df` 相当を再現。ディスク列挙は **`sysinfo`**（mac/win/linux 共通）、APFSスナップショット数は macOS 限定（`cfg(target_os="macos")` で tmutil、他OSは n/a）。`df`/`df --json` の2出力。将来 Tauri から in-process 呼び出し（bash spawn を廃止）する土台。
- **実測で判明した設計上の知見**：`sysinfo` は**コンテナ全体の「X% full」は概ね正確**だが、macOS APFS では **①ボリューム単位 used がコンテナ全体usedに潰れる ②一部ボリューム(xarts等)を列挙しない**。→ 忠実な内訳が要る箇所は Unix で `df`/statvfs 併用、`sysinfo` は可搬フォールバック（＆Windows経路）という方針。`cargo build` 7.8秒・target は gitignore（`target/`）
- **`scan` 移植 ✅（B-b・ローカル検証済）**：`engine/src/scan.rs`＋`util.rs` に分割（main.rs肥大回避）。OS非依存パターン2つを findings 化（id/path/size/severity/description/action、text/json 出力）：`ollama_models`(~/.ollama/models>10GB)・`node_modules_aggregate`(~/Development>10GB)。`dir_size` は **Unix で `st_blocks×512`（＝du相当）/ Windows は logical len** の cfg分岐、symlink非追従・権限エラー許容（bashの罠回避）。**fidelity検証**：初回は論理サイズで du より ~1.3GiB 過少→ブロック基準に修正で **Rust 17.6GiB vs bash du 17.4GiB（差~1%＝測定間の実変化）** に一致。ollama は実機で空(0B)＝両者とも非検出で一致。clippy クリーン
- **scan パターン量産 ✅（11パターン・bash照合済）**：`dir_pattern()`/`finding_if_over()`/`file_size()` ヘルパーで1行追加できる構造。**単純系8**＝`ollama_models`/`user_cache`(~/.cache)/`node_modules_aggregate`（クロスプラットフォーム）＋`library_caches`/`xcode_derived_data`/`ios_devicesupport`/`coresimulator_caches`/`docker_raw`（macパス＝不在OSで自然にスキップ）。**複雑系3**＝`iphone_backup`（複数ベンダー location＋`has_file_named` で Manifest.db 再帰探索＝有→medium/無→high の破損判定・DiskSage 看板機能）／`apfs_snapshots`（`cfg(macos)` で tmutil の TimeMachine 数>3）／`vm_swap`（`cfg(macos)` で /private/var/vm の swapfile*＋sleepimage 合計>5GB）。**bash 実機照合**：単純系は数値一致（node_modules 17.6/17.5 等）、複雑系3は当機で全て閾値未満→**Rustもbashも「検出なし」で一致**（TimeMachine 0・sleepimage 2GBのみ・backup空/86M）。実測で「なし」が正しいことまで確認（tmutil/vmdir/backup 生値を突合）
- **ファイル分割 ✅**：`scan.rs` が376行になったため走査ヘルパーを **`walk.rs`**（`entry_size`/`dir_size`/`node_modules_total`/`file_size`/`has_file_named`/`tildify`）に分離＝全ファイル300行以下（main167/scan270/util49/walk114）。300行ソフト規律に準拠
- 残る複雑系は無し（主要12パターン中 flow_type/electron_cache 以外は移植済＝ほぼ parity）。
- **engine ライブラリ化 ✅（in-process統合の第一歩）**：engine を lib+bin に分割（`lib.rs`＝`pub mod df/scan/util/walk`、`df.rs` にデータ関数＋トレイ用 `startup_free()`、`main.rs` は薄いCLI）。`scan::collect()` も pub 化＝desktop から呼べる。engine ビルド0.3秒・clippy クリーン・CLI回帰OK。**desktop トレイが engine を in-process 呼び出し**（`disksage_engine::df::startup_free()`＝lib.rs の sysinfo 直呼び重複を解消）。desktop 側の検証は Tauri ビルドが要るため**CIで検証**（ローカル2GBビルド回避）
- **Rust serve 化（本丸②）進行中**：`engine/src/serve.rs`＝純Rust HTTP（`tiny_http` 0.12・TLS無し/system依存なし）。`disksage-engine serve [--port N]`。**増分1 ✅**＝`/` にディスク使用量オーバービュー（`df::containers()` からバー生成＋スナップショット警告）＋左サイドバー（Scan/Reports/Settings、後2つは200スタブ）。実機で curl 検証（bash 不使用で UI 配信＝両コンテナの空き表示・404回避）。ビルド2.47秒・clippy クリーン・serve.rs 115行
- **serve Rust化の増分**：①オーバービュー ✅／**②findings 表示 ✅**（`Arc<Mutex<ScanState>>`＝背景スレッドで `scan::collect()`、`/` は即オーバービュー＋「Scanning…」＋2秒 meta refresh→完了で severity別カラーカード。実機で 即表示→~31秒後 findings 自動出現＝LOW node_modules・SAFE×2、CLIと一致を検証。serve.rs 224行）。**③/reports 履歴 ✅**（`reports.rs` 新設＝`$DISKSAGE_HOME/scans/*.html` を一覧・新しい順「latest」タグ、`/report?stamp=` で保存HTMLを配信、stamp は on-disk ホワイトリスト照合＝traversal を404で拒否・実機で `../../etc/passwd`→404 検証。`esc` を util へ共通化、serve.rs 231行に収め全ファイル300行以下維持）。**④/settings ✅**（`settings.rs` 新設＝言語ラジオ Auto/日本語/English、POST `/settings` で `$DISKSAGE_HOME/config` の `lang=` を書換=bash と同一ファイル・値は ""/ja/en ホワイトリスト、`?saved=1` で✅バナー、tiny_http の POST body は `req.as_reader()`＝ループ変数を `mut` に。実機で 表示/POST→303→config=lang=ja/空=lang行削除/ラジオ選択反映 を検証）。**⑤削除→ゴミ箱 ✅**（`trash.rs`＝`DELETABLE` ホワイトリスト[iphone_backup/ollama_models/xcode_derived_data/coresimulator_caches/coresimulator_devices/ios_devicesupport]・`to_trash` は mac=Finder/osascript / Linux=gio・trash-cli / 他=未対応・`expand_tilde` で ~ 展開。`findings.rs` に findings描画分離＝削除可パターンのみチェックボックス＋赤い「Move to Trash」フォーム(JS confirm付き)。POST `/delete` は **現findings由来の deletable パスのみ許可**＝任意パス削除を拒否、`percent_decode` でフォーム値復元、削除後 `trigger_scan`。設計原則[自動削除しない/ゴミ箱経由/ユーザー承認]を Rust でも死守。実機検証：削除可のみチェックボックス／不正パスPOST→ダミー生存（ホワイトリスト機能）／osascript でダミーが ~/.Trash へ移動＝ゴミ箱経由・復元可。全ファイル300行以下維持[serve233/findings81/trash75]）。**⑥a UI枠 i18n ✅**（`lang.rs`＝`OnceLock<bool>` に起動時解決[DISKSAGE_LANG→config→$LANG→AppleLocale→en]、`t(en,ja)` ヘルパー。serve/findings/reports/settings の**UI chrome**を日/英化＝サイドバー・見出し・空き/起動ディスク・検出結果・スキャン中バナー・削除ボタン/confirm/注記・設定各項目・レポート履歴・`<html lang>`。実機で JA=日本語／EN=英語 を検証。全ファイル300行以下[serve256/lang66]）。**⑥b findings本文 i18n ✅**：`scan.rs` の label/action を `lang::t(en,ja)`、複雑系(iphone_backup/apfs_snapshots/vm_swap)の description は `is_ja()` で日英分岐。serve は起動時 `lang::init()` → 背景スキャンが日本語findingsを生成。実機検証：serve(ja) で findings 本文も日本語（「~/Development 配下の node_modules 合計」等）／CLI `scan` は `init` 非呼び出しで英語のまま（開発ツール＝英語）。**＝serve の Rust 化が i18n 込みで bash serve と parity 到達 🎉**
- **本丸②の最終ステップ＝desktop の切替**：`desktop/src-tauri/lib.rs` の `start_serve()`（今は bash `disksage serve` を spawn）を **`disksage-engine serve` 起動**に変える → bash 依存が完全に消える → **Win/Linux 実現**。tray は既に in-process。切替後は #17 CI で Win/Linux ジョブ追加が可能に。desktop 変更は Tauri ビルド要＝CI検証（ローカル2GB回避）。残: 配布バイナリに engine を同梱 or PATH 解決（トレイの resolve と同様）

### メニューバー常駐 ✅: Tauri トレイ（SwiftBar 実験→撤去→自前トレイに置換）

**アイデア（ユーザー発案）**: 空き容量を右上メニューバーに常時表示し「0近くになる前に気づく」＝DiskSage の中核価値（未然察知）のアンビエント層。**この発想は正しい**（実験中に実機が 923MB まで落ちたのを `💾` が赤で即警告＝有効性を実証）。

**やったこと（記録）**: PoC として SwiftBar/xbar プラグイン `menubar/disksage.30s.sh` を作成（`disksage-engine df --json` を30秒毎→`💾 <free>`、85%橙/95%赤）。動作確認のため実機に `brew install --cask swiftbar` して起動・検証（PR #20 でマージ）。

**なぜ撤去したか（記録）**:
1. **Mac 専用**：SwiftBar/xbar は macOS 限定。プロジェクト方針は Tauri→**マルチプラットフォーム**なので方向がズレる。
2. **サードパーティに任意スクリプトを常駐自動実行させるモデル**が、DiskSage の「Privacy first / Transparent」原則と相性が悪い。初回に**オートメーション/カレンダー/リマインダー**権限を要求（他プラグイン用の宣言）し、ユーザーが「危険」と感じて拒否＝正しい判断。
3. SwiftBar 自体は MIT の正規 OSS で無害だが、上記2点で不採用。

**撤去内容**: `brew uninstall --cask swiftbar`＋プラグイン symlink＋`defaults delete com.ameba.SwiftBar`（2026-09-16 実施）。repo からも `menubar/` を削除。

**置換（実装済 ✅・ユーザー目視確認済）**: `desktop/src-tauri/src/lib.rs` に **Tauri トレイ**を実装。`setup_tray()`＝`TrayIconBuilder`（id=`disksage-tray`）で `set_title(free_title())` にバー表示、メニュー（Open DiskSage / Quit）、30秒毎に別スレッドから `tray_by_id().set_title()` で更新。空きは **`sysinfo` で in-process 取得**（`free_title()`＝起動ディスク`/`の available、fallback は最大disk）＝**サブプロセスも権限要求も無し**（SwiftBar の権限ダイアログ問題が消える）。Cargo に `tauri features=["tray-icon"]`＋`sysinfo`。`cargo build` 38秒で通過→起動・無クラッシュ→**ユーザーがメニューバーに `💾 <空き>` を目視確認**（2026-09-16）。`TrayIcon::set_title` は macOS対応/Win非対応/Linux部分（[APIリファレンス](https://docs.rs/tauri/latest/tauri/tray/struct.TrayIcon.html)）＝Win/Linux では将来アイコン＋tooltip＋メニューで空きを見せる。
- engine crate 統合 ✅：desktop の `free_title()` は `disksage_engine::df::startup_free()`＋`util::human()` を呼ぶ（sysinfo 直呼び重複を解消・CLI と同一ロジック）。desktop Cargo に `disksage-engine = { path = "../../engine" }`
- 残（磨き込み）: Dockアイコン非表示でメニューバー専用化（`ActivationPolicy::Accessory`）／ログイン時自動起動／窓を閉じても常駐／配布は #17 の .dmg CI に同梱

### 将来 Phase（0.3+）: Rust リライト

- スキャンエンジンを Rust (walkdir, tokio) に
- UIは Tauri 2.x (Rust + WebView) で GUI 化
- Electron を避ける理由：**容量管理アプリ自体が重い自己矛盾を回避**

詳細は `docs/企画書.md` 7.1節。

---

## 絶対に守る設計原則

1. **Never auto-delete.** DiskSage は「提案」のみ。削除は常にユーザー承認。`rm -rf` を自動実行する機能は絶対に追加しない。
2. **Safer than CleanMyMac.** 削除は常にゴミ箱経由。「完全削除」は追加の明示承認が必要。
3. **Privacy first.** `--ai` モードでも、送るのはファイル名・パス・メタデータのみ。**ファイル内容は絶対に送信しない**。
4. **BYOK default.** Claude API 使うなら自分のキー。マネージド API は Pro 版（将来）の差別化要素。
5. **Transparent.** OSS、判定ロジックは全部読める。ブラックボックスは作らない。

---

## 次にやること（優先順）

### すぐ（Phase 1）

- [ ] GitHub リポジトリ作成、disksage を push
- [ ] README の `<your-org>` を実際の組織名・ユーザー名に置換
- [ ] 実際の Mac で `disksage scan` を走らせて動作確認
- [ ] パターンの誤検出・見逃しを洗い出してチューニング
- [ ] CI（GitHub Actions で ShellCheck）
- [ ] `CONTRIBUTING.md` を書く

### 近い将来（Phase 2）

- [x] `--ai` モードの実装（Claude API 連携）✅ 完了（curl + structured outputs）
  - 入力：パス・サイズ・メタデータのみ（ファイル内容は送信しない）
  - **マスキング実装済み**：送信前に `$HOME`→`~`（ユーザー名除去）、ユーザー固有フォルダ名→`<dirN>` に匿名化。既知ツール/ベンダー名（iMobie/.ollama/node_modules 等）は判定精度維持のため保持
  - プレビュー＝実送信物（マスク済みパスを確認画面に表示）、レポートは実パス（index で突合）
  - 出力：判定（safe_to_delete / archive_then_delete / review_first / keep）+ confidence + 理由
  - BYOK・2プロバイダ対応（`ai_resolve` で自動判定）：`ANTHROPIC_API_KEY`（本家 / x-api-key）または `OPENROUTER_API_KEY`（OpenRouter / Bearer、`/api/v1/messages` が Anthropic 互換）。`DISKSAGE_AI_PROVIDER` / `DISKSAGE_AI_BASE_URL` で強制可
  - モデルは `DISKSAGE_MODEL`（既定 本家=`claude-opus-4-8` / OpenRouter=`anthropic/claude-opus-4.8`）
  - 送信前にプライバシープレビュー画面で確認（`--yes` でスキップ可）
  - 実装メモ：`ai_mask_findings` / `build_ai_request` / `parse_ai_response` / `ai_preview_and_confirm` / `run_ai_analysis` / `render_ai_section`（`disksage` 内）
  - 既知の残留：description 文に app/ベンダー名が残る（判定シグナルかつ低機微・プレビューで可視）。残課題：API キーの OS キーチェーン保管（現状は環境変数のみ）、flow-type ファイルの AI 判定対象化、description のマスキング強化
  - **監査ログ ✅**：`--ai-log`（or `DISKSAGE_AI_LOG=1`）で `~/.disksage/ai-logs/<日時>/` に `request.json`（実送信物・マスク済）/`response.json`（生レスポンス）/`masking.tsv`（real→masked 対応表＋anonymized列）を保存。`run_ai_analysis` 内で curl 後・rm 前に出力、`write_masking_log`（Python）で real/masked findings を index 突合。マスキング効果の定量確認に使える。`masking.tsv` は実パス含む＝ローカル限定・非共有。副産物: coresimulator_devices が `Developer/CoreSimulator` 未allowlistで過剰マスクと判明（今後 SAFE に追加検討）
- [ ] パターンを JSON に外出し（現在は bash にハードコード）
- [ ] より多くのパターン（Photos Library、Mail、iCloud Optimized、Time Machine local）
- [ ] Homebrew tap 対応
- [ ] 外付けドライブへの退避ウィザード

### 中期（Phase 3）

- [ ] Rust リライト
- [ ] Windows 対応（WSL2 VHDX、Docker Desktop、Adobe等）
- [ ] Linux 対応（Flatpak、systemd journal、Docker）

### 長期（Phase 4）

- [ ] Tauri GUI 版
- [ ] Sudden Growth Detector（急増検知）
- [ ] パターンライブラリのコミュニティ化（PR 受け入れ態勢）
- [ ] Pro 版（マネージド API、複数端末、分析ダッシュボード）

---

## よくやりがちな罠（避ける）

### Bash スクリプトの罠

- `set -euo pipefail` は**使わない**。find/du/stat で permission denied が頻発するため、個別エラーは許容する必要がある。現状 `set -uo pipefail` のみ。
- `find | head -1` は `pipefail` 下で SIGPIPE エラーを起こす。`|| true` でガード。
- スペース入りパスは必ずクォート。`"$path"` で囲む。

### macOS 固有

- `df -h /` は**システムボリュームしか見えない**。`df -h` で全ボリューム、`diskutil apfs list` でコンテナ構造を確認。
- `tmutil listlocalsnapshots /` は Time Machine のみ。`diskutil apfs listSnapshots /` も必要。
- **フルディスクアクセス権限**がないと `~/Library/` 配下でダイアログ連発。README にセットアップガイド追加予定。

### プライバシー

- **ファイル名にも個人情報が含まれる場合がある**。AI モードに送る前に、ユーザーが確認できる画面が必要。
- **API キーは Keychain 保管**。平文保存は禁止。

---

## ユーザーペルソナ

### プライマリ

- **Tech Founder / CEO-Engineer**（30代後半〜40代）
- 10+ プロジェクト並走、Docker + Ollama + Claude Code で開発
- 200+ MVP の個人ポートフォリオ、Node/Bun/TypeScript 中心
- バックアップ戦略ゼロ、クラウド課金を避けたい、NAS は欲しいが買ってない
- 「整理する時間がない」が口癖
- CleanMyMac は「勝手に判断される感じが嫌」で離脱

### セカンダリ

- AI ネイティブ開発者（Whisper、SD、LLM モデル大量保持）
- プライバシー志向層（OSS・透明性重視）
- Claude Code / Codex ヘビーユーザー

---

## 関連ドキュメント

- `README.md` — ユーザー向け、OSS 公開用
- `docs/企画書.md` — 詳細な企画書（元々 docx）
- `docs/要件定義.md` — 機能要件・非機能要件
- `docs/セッション_20260420.md` — プロジェクト発端となった1日のログ
- `LICENSE` — Apache-2.0

---

## 開発者向けメモ

### テスト

現状ユニットテストはなし。MVP 完成後に bats-core で書く予定。

```bash
# 動作テスト（手動）
./disksage scan
./disksage snapshot
./disksage trend
```

### コーディング規約

- Bash: Google Shell Style Guide 準拠
- Python 3 embedded: PEP 8 準拠
- コメントは日本語 OK（国内ユーザー多数想定）だが、関数名・変数名は英語
- 将来の Rust: rustfmt + clippy

### パターン追加の手順（当面）

1. `disksage` の `scan_patterns()` 関数に検出ロジックを追加
2. `add_finding "$findings_file" <pattern_id> <path> <size_bytes> <severity> "<description>" "<action>"`
3. README の「What DiskSage Detects」テーブルに追記
4. PR 作成

将来（v0.2+）、パターンを `patterns/*.json` に外出しする計画。

---

## コミュニケーション

- リポジトリ: https://github.com/<your-org>/disksage（作成予定）
- Issues: バグ報告・機能要望
- Discussions: 設計議論、パターン提案
- 想定コミュニティチャネル: Discord（日英併記）

---

## ライセンスポリシー

- コード: **Apache License 2.0**
- パターンライブラリ: **CC-BY-SA 4.0**（コミュニティ知識の共有財産として）

---

_このプロジェクトは Foresthill と Claude の共同作業で生まれました。記録としての会話ログは `docs/セッション_20260420.md`。_
