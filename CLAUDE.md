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
  - `scan --html` — 自己完結HTMLレポート ✅（深刻度カラーカード・AIバッジ・ディスク使用量バー。外部依存なし・オフラインで開ける。`render_html` 内 Python、i18n対応。GUI化(v0.4)前の低コスト視覚化ステップ）。flow-type は `cmd_scan` で一度だけ計算し md/html で共有
  - レポート i18n ✅：`DISKSAGE_LANG`（`$LANG` 自動判定）で日本語レポート全文対応。`t()`/`t_lookup()` + `CATALOG_EN`/`CATALOG_JA`（bash 3.2 互換、連想配列不使用）。見出し・各パターン説明/Action・Next Steps・AI判定欄・フッタを翻訳。CLI/help は英語のまま。日本語以外の言語コードは AI reasoning のみ翻訳し、雛形は英語フォールバック
  - `snapshot` — ディスク使用量記録
  - `trend` — 時系列表示
  - `help` / `version`
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
