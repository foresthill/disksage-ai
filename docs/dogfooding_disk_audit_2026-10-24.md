# DiskSage ドッグフーディング：ディスク棚卸し診断レポート

- **対象マシン**：Foresthill の Mac（起動ディスク 460GB SSD、`/dev/disk3s1s1`）
- **期間**：2026-10-22 〜 2026-10-24
- **記録**：Claude（DiskSage をドッグフーディングしながらの実地棚卸し）
- **凡例**：✅計測＝実際にコマンドで測った事実 ／ 🧩推論＝計測値から導いた解釈（断定ではない） ／ ⚙️対応＝実施した操作

> このレポートは「あとで1つ1つ見る」用に、**計測値と推論を分けて**書いています。
> 数値はすべてその時点の `df -h /` / `du -shx` / `tmutil` による実測です。

---

## 1. 結論（先に3行）

1. 🔴 **根本原因は Claude ワークスペース VM の増大**。使うほど育ち、460GB をほぼ使い切った環境では数日ごとに空きをゼロ近くへ押し戻す。
2. node_modules などを消しても VM がまた食うので、**それだけだと「賽の河原」**。VM 回収が本丸。
3. ⚙️ 応急としてキャッシュ＋不要 node_modules を削除し、2日間で合計 **約7GB** を回収（データ損失ゼロ）。

---

## 2. 根本原因：Claude VM の増大（本丸）

✅ **計測**（`du -shx ~/Library/Application\ Support/Claude/vm_bundles`）：

| 日付 | Claude VM サイズ |
|---|---|
| 2026-10-22 | **13 GB** |
| 2026-10-24 | **15 GB** |

→ ✅ 2日で **+2GB** 増加。中身は `rootfs.img`（~8GB）＋ `sessiondata.img`（~3GB）＋ `rootfs.img.zst` 等＝Claude Code/Desktop のワークスペース VM。

🧩 **推論**：460GB をほぼ使い切っている（恒常的に 95〜98% 使用）この環境では、この VM の自然増加が「また満杯」の最大の押し戻し要因。node_modules を数GB空けても、VM が数日で同等量を取り戻すため、**VM を回収しない限り綱渡りが続く**。
※ 「VM 増加＝唯一の原因」とまでは断定しない（Spotlight 再インデックス・各種キャッシュ再生成・ログ等の churn も常時ある）。ただし**単一で最大・かつ継続的に増える塊**なのは計測で確か。

### 推奨アクション（本丸）
- **Claude アプリ → 設定 → ストレージ** から整理する。
- ⚠️ `vm_bundles/` を**手で `rm` しない**（ワークスペースが初期化される）。DiskSage でも `claude_vm` パターンは severity=info・削除UI非対象にしてあり、手削除を促さず Claude 自身の整理へ誘導する設計。

---

## 3. 空き容量の推移（実測タイムライン）

✅ すべて `df -h /` の実測値。

| 時点 | 空き | 使用% | 出来事 |
|---|---|---|---|
| 2026-10-22 開始 | 2.7 GB | 82% | 棚卸し開始 |
| 〃 キャッシュ掃除後 | 5.1 GB | 70% | pnpm/npm/uv prune・clean＋`*.ShipIt`＋brew cleanup（+2.4GB）|
| 〃 node_modules×2 削除中 | 3.8 GB | 76% | 一時的に**減った**（下記 §5 の現象）|
| 〃 静止後 | 4.8 GB | 72% | Spotlight 収束で回復 |
| 〃 worktree node_modules 削除後 | 4.1 GB | 75% | （再び Spotlight の波）|
| **2026-10-24 朝** | **0.26 GB** | — | 🔴 危険域。作業ファイルすら作れず（ENOSPC）|
| 〃 ユーザ手動クリーン後 | 2.5 GB | 83% | `*.ShipIt`＋ms-playwright＋npm/pnpm（手実行）|
| 〃 uv/brew 追加 | 2.6 GB | 82% | |
| 〃 node_modules×2 削除後 | **4.3 GB** | 74% | poricomi / ocean-website（+1.6GB）|

✅ **2日間の回収合計：約 7GB**（キャッシュ再生成分＋node_modules）。いずれもデータ損失なし。

---

## 4. 削除した項目（すべて再生成／再取得可能）

### 4-1. キャッシュ類（消しても自動再生成）
- `~/Library/Caches/*.ShipIt`（アプリ更新DLの残骸。VSCode 1.4GB 等）→ 次回更新時に再生成
- `~/Library/Caches/ms-playwright`（Playwright のブラウザ群）→ 使用時に再DL
- `pnpm store prune`（未参照パッケージのみ）／`npm cache clean --force`／`uv cache clean`／`brew cleanup -s`

### 4-2. node_modules（`npm/pnpm install` で完全復元＝データ損失ゼロ）
| 削除日 | プロジェクト | サイズ | 備考 |
|---|---|---|---|
| 10-22 | receptron/mulmoclaude | 1252 MB | ユーザ確認済 |
| 10-22 | foresthill-personal/devlog-igrek | 858 MB | ユーザ確認済 |
| 10-22 | ai-mailer/.claude/worktrees/upbeat-cohen-ebbb45 | 1158 MB | マージ済 worktree |
| 10-24 | poricomi-prototype-claude-web | 818 MB | 不要（とりあえず）|
| 10-24 | ocean-website-ai | 814 MB | 不要（とりあえず）|

---

## 5. 「削除したのに空きが減った」現象の解明

✅ **計測**：node_modules を 2.1GB 削除した直後、空きが 5.1→3.8GB と**逆に減少**。
✅ 原因調査：
- `tmutil listlocalsnapshots /` → **スナップショット 0 個**（空きを握っていない）。
- `pgrep -x mds_stores` → **Spotlight（mds_stores）稼働中**＝削除した node_modules ツリーを**再インデックス中**で一時的に書き込み。
- 静止後に 4.8GB へ**自動回復**。

🧩 **推論**：これがユーザの言う「みるみる減る」の正体の一つ。**連続的なリークではなく、Spotlight 再インデックス等の一時的な波**。削除自体は確実に効いている（対象ディレクトリの不在も確認済）。

---

## 6. 現在の node_modules インベントリ（2026-10-24 時点・残存）

✅ `find ~/Development -type d -name node_modules -prune` → `du -sxm` 実測。合計 約20GB（削除前）。残っている上位：

| サイズ | プロジェクト | メモ |
|---|---|---|
| 1158 MB | AI-Driven/ai-mailer | 現役？ |
| 1056 MB | AI-Driven/ai-mailer/scripts/pii-ner-poc | PoC（終了候補）|
| 950 MB | AI-Driven/manhole-guessr | |
| 946 MB | AI-Driven/currency-exchange-system | |
| 891 MB | AI-Driven/ceotaro/devlog | |
| 890 MB | Igrek/igrek-kintai | |
| ~~818 MB~~ | ~~poricomi-prototype-claude-web~~ | ✅削除済 |
| ~~814 MB~~ | ~~ocean-website-ai~~ | ✅削除済 |
| 795 MB | AI-Driven/chrono-gua-ai/web | |
| 760 MB | AI-Driven/kotone-scribe-ai/web | |
| 746 MB | AI-Driven/hhms-railmap-ad-ai | |
| 712 MB | AI-Driven/dream-analyzer-ai | |

> 判断基準：node_modules は**どれも消して安全**（再インストールで復元）。コストは「次に触るとき `install` し直す時間」だけ。現役プロジェクトは残し、終了/休眠中のものから消すのが実用的。

---

## 7. 今後の運用メモ（恒久対策の候補）

- 🧩 **本丸は Claude VM**。定期的に Claude 設定→ストレージで整理すれば、この綱渡りの頻度が下がるはず（VM が最大・継続増加のため）。
- マージ済み worktree は `git worktree remove <path>` で本体ごと片付けるのが正式（node_modules だけでなくフォルダも消える）。
- 460GB を恒常的に使い切る構成自体がボトルネック。中期的には外付け/NAS への退避先が要る（企画書でも「バックアップ戦略ゼロ」はペルソナの既知課題）。
- DiskSage 的学び：**「削除して安全な再生成物（キャッシュ/node_modules）」と「手削除NGの管理対象（Claude VM）」を severity で区別して提示する**設計が、まさにこの実地棚卸しで有効だった。

---

## 付録：使用した確認コマンド（再現用）

```bash
# 空き容量（コンテナ単位）
df -h /

# 大きいフォルダ（ホーム配下の node_modules）
find ~/Development -type d -name node_modules -prune | while read -r d; do
  printf '%6s MB\t%s\n' "$(du -sxm "$d" | cut -f1)" "$d"; done | sort -rn

# スナップショットが空きを握っていないかの確認
tmutil listlocalsnapshots /

# Spotlight 再インデックス中か
pgrep -x mds_stores

# Claude VM の実サイズ
du -shx ~/Library/Application\ Support/Claude/vm_bundles
```

_記録：Foresthill × Claude（DiskSage ドッグフーディング）_
