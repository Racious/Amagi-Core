# Vault × Core 整合 — 開發理念、變更紀錄與驗測指南

> 對象：Step 2 + Step 3(a/b/c) + Step 4(a/b)。分支 `feat/vault-path-managed-pointer`。
> 設計總綱：amagi-vault `projects/amagi-core/pages/adr/adr-002-vault-core-integration.md`（D1–D10）。
> 本文供驗測使用：先讀「一、理念架構」建立全貌，再依「三、驗測指南」逐項驗證是否符合需求。

---

## 〇、接手指南（換機驗測，先讀這段）

### 1. 同步程式碼與知識庫（兩個 repo）
```
# Amagi Core（本檔所在 repo）
git fetch
git checkout feat/vault-path-managed-pointer
git pull

# amagi-vault（知識庫，另一個 repo；路徑見全局 CLAUDE.md 或自己記得的位置）
cd <amagi-vault 路徑>
git pull
```

### 2. 告知該機的 Claude / 天城（把這段貼進對話）
> 我們在驗測 amagi-vault × Amagi Core 整合的 Step 2~4。
> 請先讀 `docs/STEP3-4-PROGRESS.md`（進度）與 `docs/STEP3-4-VERIFICATION.md`（理念/變更/驗測），
> 然後陪我照 T1~T7 逐項驗測，記錄不符需求或有 bug 之處。

### 3. 啟動並驗測
```
npm install          # 保險起見（本次未動依賴）
npm run tauri:dev    # 啟動 app
```
接著依下方「三、驗測指南」T1~T7 操作。前置提醒：
- **T1** 會寫本機全局 `~/.claude/CLAUDE.md`（首次跑前先備份該檔）。
- **T5** 需先在 vault `sources/clips/` 放幾個 `.md`；**T6** 需先在 vault `_skills/` 放原生 `SKILL.md`。
- **T7** 會對 vault repo 真實 `commit + push`。
- 各機路徑不同步：本機 vault 路徑用 app「設定→知識庫(Vault)」設定即可（即 T1）。

---

## 一、開發理念與架構

### 核心理念
1. **純 Markdown + git，不引入執行期外掛**（抗鎖定）。能力強化全由 Amagi Core 承擔。
2. **以「消費者」切割產出**：AI 寫程式要遵守的 → 記憶（repo CLAUDE.md/AGENTS.md）；可被呼叫的流程 → 技能（repo .claude/skills）；老爺要查的 → 文件（vault）。
3. **多機同步優先**：本機路徑/指針由 Core 管理（不進 repo），知識與技能庫靠 git 跨機同步。
4. **非破壞**：所有寫入優先「新增/略過既存」，覆寫前備份，永不刪除老爺手做內容。
5. **人在迴圈**：來源 → 候選 → 老爺審核 → 才寫入。分類器可錯，審核兜底。

### 三層架構
```
裝置層（不同步）   Amagi Core App｜全局 ~/.claude/CLAUDE.md 受管指針｜Obsidian（選配）
        │ Core 自管 git
同步層（git）      amagi-vault：CLAUDE.md / index / sources/ / general·shared·projects / _skills / .drafts
        │ Core 分發（記憶·技能寫進工作 repo）
工作層（各 repo）  <project>/CLAUDE.md·AGENTS.md（記憶）｜.claude·.codex/skills（技能）
```

### 資料流
- **知識**：來源（貼上／檔案／剪藏）→ ingest 產候選 → 審核佇列 → 接受寫入 vault 對應層。
- **技能**：技能庫 `_skills/`（單一來源）→ Core 分發原生格式到各 repo。
- **同步**：Core 對 vault 做 pull / commit（作者 あまぎ）/ push。

---

## 二、逐項變更（理由 / 前後差別 / 影響）

### Step 2 — vault 路徑、受管指針、專案映射
- **新增**：`core/vault_manager`、`commands/vault_commands`、`Project.vault_folder`、`init_project_vault`。
- **理由**：多機同步命門——每台機需告訴 Claude vault 在哪，且每個專案要對應 vault 知識資料夾。
- **前後差別**：
  - 前：手動編輯全局 CLAUDE.md 寫 vault 指針；專案與 vault 無對應。
  - 後：設定頁按鈕選資料夾 → Core 自動寫「受管區塊」（冪等、`.bak`）；專案自動映射 `projects/<slug>`，可一鍵建知識資料夾骨架。
- **影響**：會寫入全局 `~/.claude/CLAUDE.md`（僅 BEGIN/END 區塊，餘不動）；本機設定存 `%APPDATA%/AMAGI Core/vault.json`。

### Step 3a — 知識頁候選管線
- **新增**：`ReviewItemType::Wiki`、`core/wiki_exporter`、`commands/wiki_commands`（ingest_wiki_page / write_wiki_pages）、前端 `IngestPage`（/ingest）。
- **理由**：讓「對話結論／文件」能結構化寫入 vault，且經審核。
- **前後差別**：
  - 前：知識只能天城手動建檔；審核佇列只認記憶/技能。
  - 後：知識匯入頁建草稿 → 審核 → 寫入 vault 正式頁（自動加 frontmatter）。
- **影響**：審核佇列儲存沿用 `review-queue/queue.json`；既有審核頁與 store **排除 wiki 型別**，不影響記憶/技能流程。

### Step 3b — 檔案匯入 + 原始來源保存
- **新增**：`ingest_wiki_from_file`、`build_wiki_md` 支援 `source:`。
- **理由**：補齊 Karpathy 三層的「raw sources」層，知識可回溯出處。
- **前後差別**：前：只能貼上文字。後：可選檔案匯入，原文複製到 vault `sources/imported/`，正式頁 frontmatter 以 `source:` 回指。
- **影響**：在 vault 新增 `sources/imported/`（非破壞）。

### Step 3c — sources/clips 掃描 + 分類器
- **新增**：`core/clip_scanner`、`scan_vault_clips`。
- **理由**：讓 Web Clipper 等擷取工具落點的剪藏能自動轉成候選。
- **前後差別**：前：剪藏需人工處理。後：一鍵掃描 `sources/clips/` → 去重 → 規則式分類（troubleshooting/adr/spec/concept）→ 進佇列。
- **影響**：唯讀掃描 vault `sources/clips/`，不改原檔；含敏感資訊者略過。**採掃描式（非常駐監看）**。

### Step 4a — 技能庫跨專案分發
- **新增**：`core/skill_library`、`commands/skill_commands`、設定頁「技能庫」卡片。
- **理由**：跨專案共用技能應單一來源，避免各 repo 各自維護走鐘。
- **前後差別**：前：技能各 repo 獨立。後：vault `_skills/` 為單一來源，一鍵分發原生格式到所有專案的 `.claude/skills`、`.codex/skills`。
- **影響**：分發會**覆寫** repo 內對應的受管副本（這是預期行為）。

### Step 4b — Core 自管 vault git
- **新增**：`core/vault_git`、`vault_git_status/pull/sync`、設定頁 git 按鈕。
- **理由**：同步脫離 Obsidian，由 Core 承擔，Obsidian 回歸純瀏覽。
- **前後差別**：前：靠 Obsidian Git 外掛。後：設定頁可直接 狀態 / Pull(--ff-only) / 提交並推送（作者 あまぎ）。
- **影響**：「提交並推送」會對 vault repo 真實 commit + push；committer 為全域身分，作者為 あまぎ，不改 git config。

---

## 三、驗測指南（明天照表操課）

### 驗測前準備
1. 啟動：`npm run tauri:dev`（在 Amagi Core 目錄）。
2. 確認 `~/.claude/CLAUDE.md` 已有備份（`CLAUDE.bak` 或下載副本）——T1 會寫入真檔。
3. 確認 vault 在本機（如 `E:\projects\agents\amagi-vault`）且為 git repo。

> 記號：✅=預期結果，🔍=要去檢查的檔案/位置。

### T1 — vault 路徑 + 受管指針（Step 2）
1. 設定頁 →「知識庫（Vault）」→「選擇 vault 資料夾並套用」→ 選 vault 資料夾。
- ✅ 出現綠字「已寫入/更新 ~/.claude/CLAUDE.md 受管區塊」。
- 🔍 `~/.claude/CLAUDE.md` 末尾有 `AMAGI-VAULT:BEGIN…END` 區塊，**上方人格內容一字未改**。
- 🔍 `~/.claude/CLAUDE.bak` 存在（寫入前版本）。
- 🔍 `%APPDATA%/AMAGI Core/vault.json` 內含正確路徑。
2. 再按一次套用。
- ✅ 顯示「更新」；🔍 區塊**只有一個**（冪等，不重複堆疊）。

### T2 — 專案映射 + 知識資料夾（Step 2）
1. 專案管理頁，看每個專案。
- ✅ 顯示「Vault：projects/<專案 kebab 名>」。
2. 按某專案「vault 資料夾」。
- ✅ 提示新增目錄/檔數。
- 🔍 vault 出現 `projects/<name>/pages/{adr,specs,business}/` 與 `index.md`；既存者略過、不覆寫。

### T3 — 知識匯入（貼上）→ 審核 → 寫入（Step 3a）
1. 知識匯入頁：選專案、歸層、型別（如 adr）、填標題與內容 →「建立草稿」。
- ✅ 出現在「待審核知識草稿」。
2. 按「接受並寫入 vault」。
- ✅ 綠字「已寫入 vault：<路徑>」，項目移到「已寫入」。
- 🔍 vault 對應層出現 `pages/<category>/<slug>.md`（general/shared 則 `pages/<slug>.md`），frontmatter 含 id/title/type/last_updated。
3. 對同標題再寫一次 → ✅ 提示「目標已存在，已略過」（非破壞）。

### T4 — 檔案匯入 + 來源保存（Step 3b）
1. 知識匯入頁 →「從檔案匯入…」選一個 .md/.txt。
- ✅ 建立草稿（標題=檔名）。
- 🔍 vault `sources/imported/<檔名>` 出現原文副本。
2. 接受並寫入。
- 🔍 正式頁 frontmatter 含 `source: sources/imported/<檔名>`。

### T5 — clips 掃描 + 分類（Step 3c）
1. 先在 vault `sources/clips/` 放 2–3 個 `.md`（其一含 `---\ntitle: 測試\n---` frontmatter；內容分別含「bug/錯誤」「決策」「一般筆記」）。
2. 知識匯入頁 →「掃描 sources/clips」。
- ✅ 提示「產生 N 筆新候選」。
- 🔍 候選型別：含 bug→troubleshooting、含決策→adr、其餘→concept。
3. 再按一次掃描 → ✅「沒有新的剪藏」（去重）。

### T6 — 技能庫分發（Step 4a）
1. 先在 vault `_skills/` 放 1–2 個原生 `SKILL.md` 格式檔（含 `---\nname: \"X\"\n---`）。
2. 設定頁 →「技能庫」→ 看數量 →「分發到所有專案」。
- ✅ 提示「已分發 N 技能到 M 專案」。
- 🔍 各專案 repo 出現 `.claude/skills/<slug>/SKILL.md` 與 `.codex/skills/<slug>/SKILL.md`。

### T7 — vault 自管 git（Step 4b）
> 注意：「提交並推送」會對 vault repo 真實 commit + push。
1. 設定頁 →「知識庫（Vault）」→「狀態」。
- ✅ 顯示 git status（剛才 T2–T5 寫入的檔應列出為未追蹤/變更）。
2. 「提交並推送」。
- ✅ 提示「已提交並推送」。
- 🔍 vault `git log -1` 作者為 `あまぎ <amagi.core@gmail.com>`，內容已推到 origin。
3. 「Pull」。
- ✅ 顯示已最新或拉取結果。

---

## 四、驗測注意事項
- **T1 寫真實全局 CLAUDE.md**：有 `.bak` 與下載副本墊底；若不滿意可還原。
- **T7 真實 git push**：需 vault repo 已設 origin 與認證；失敗會顯示 git 錯誤訊息。
- **安全過濾**：T3–T5 內容若含疑似密鑰會被擋下並提示——屬預期。
- **分類器**為規則式、可能誤判：審核時可在內容/型別上調整（或忽略）。
- **需求對照**：逐項對照「這是不是我們要的行為」；不符之處記下，回報天城調整。
