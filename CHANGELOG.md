# Changelog

AMAGI Core 的所有重要變更記錄於此。版本遵循語意化版號（major.minor.patch）。

## v0.9.0

封鎖項丟棄灰名單 ＋ vault git 同步 app 強制層（ADR-008）＋ output style 分發。全數經 Codex 交叉審查與實機驗證。

### 功能
- **output style 分發**：設定頁新增「分發 output styles」——把 vault `_output-styles/*.md`（天城應對模式正本）覆蓋分發到 `~/.claude/output-styles/`（README 與 dot-prefixed 排除、冪等、per-file temp+rename 原子寫入），並 ensure `~/.claude/settings.json` 的 `outputStyle` 預設（缺檔建最小 JSON 帶「天城」／缺欄位補預設且保留其他欄位與鍵序／已有值位元組級不動／JSON 壞檔 fail-safe 不寫只警告；讀剝 BOM、寫回 UTF-8 無 BOM）。回報分發款式名、settings 動作、缺 `name:` 警告；無 style 可分發明確告知不靜默。
- **封鎖項丟棄灰名單**（adr-007）：誤判 hit 級勾選靜音（混合卡不連坐）、灰名單 vault 權威源隨 git 跨機、審核頁可檢視可解除。
- **vault git 同步 app 強制層**（adr-008）：Pull／提交並推送整合 `pull --rebase`——衝突三段分類安全退回、push race 重試、不代合 daily、不 force push；AppError 結構化變體（GitConflict／GitSyncUnpushed）讓錯誤性質一眼可辨。

### 品質
- 灰名單：設計三輪（含 Junie/Gemini 三方複審）＋實作兩輪閉環＋實機九步；adr-008：設計審＋實作兩輪＋實機 smoke 四步；output style：Codex 審無高中風險、三低風險全採納修正＋UI 引導式實機＋切換三步實測。cargo test 216 綠、vue-tsc 0。

## v0.8.2

vault 路徑防護閘 ＋ 移除專案連帶清佇列殘項 ＋ 設 vault 路徑偵測 doctrine 源檔並提議一併部署。全數經 Codex 交叉審查與實機驗證。

### 功能
- **設 vault 路徑偵測 doctrine 源檔、提議一併部署**：`set_vault_path` 偵測 vault 內 `general/_meta/global-agent-config.md`（回傳 `hasDoctrineSource`）。設定頁選定 vault 後若偵測到源檔，跳確認對話提議一併「同步全域 doctrine」（破壞性整檔覆蓋須經確認、可婉拒）；首次啟動引導新增 inline 可選步驟「部署全域 doctrine」，補齊新機首啟一步就緒的主要場景。自動部署失敗訊息與「設定失敗」分流、部署期間防重入。

### 修正
- **移除專案連帶清佇列殘項**：移除專案時一併清除該專案殘留在 `queue.json` 的所有項目（修批測發現④：孤兒項無法從 UI 觸及卻只增不減）。採 best-effort——清理失敗僅記錄、不阻斷專案移除（避免專案已移除卻回報失敗且無法重試）。

### 安全
- **vault 路徑防護閘**：拒絕將 Amagi-Vault 知識庫（vault 根或其下任一路徑）註冊或初始化為專案——否則同步會覆寫 vault 根 CLAUDE.md 等規範文件（修 2026-07-03 跨機事故）。六條寫入路徑防守深度（sync/init/promote/bridge×3/技能分發），canonical 比對 `is_same_or_under` fail-closed。

### 品質
- vault 路徑防護閘經 Codex 兩輪審（cargo test 176 綠）；本版 ①④ 經 Codex 交叉審查四輪全數閉合＋`guided-smoke-test` 實機七項逐一通過；cargo test 178 綠、vue-tsc 0 錯。

## v0.8.1

封鎖卡可直接定位並開啟命中檔案＋學習頁單一入口。

### 修正
- **封鎖卡檔案定位**：學習流程改為把 diff 依檔切段、逐檔過安全過濾，封鎖卡以「📄 檔案路徑」分段列出各自命中規則與遮罩片段——使用者不必再自己翻 diff 找位置。路徑主解析走段內 `+++ b/` 行（rename 取 b 側、刪檔退 a 側、含空白路徑無歧義）；支援 git C-style quoted 路徑（`core.quotePath` 預設下的中文檔名可正確還原）；同一規則在多個檔案命中會各自列出（原整段掃描每規則僅報第一筆）。處置建議同步改寫為三條（開檔修正／誤判丟棄／已 commit 先換金鑰）。
- **命中檔一鍵開啟**：封鎖卡上的「📄 檔名」為可點連結，點擊即開檔案總管並選中該檔（複用 reveal_in_explorer 的相對路徑安全驗證）。
- **學習頁單一入口**：移除冗餘的「掃描 Git 變更」鈕（學習內部本就自行掃描）——僅留「開始學習」，變更概要卡（分支/diff 概要/最近提交）於學習後自動附上。
- 經 Codex 審查兩輪（R1 quoted header 缺口已修、R2 判閉合）＋實機驗證（中文檔名封鎖卡、點檔名開啟、單鈕學習流）；cargo test 165 綠。

## v0.8.0

vault-first Phase 3 補完（整個系統一致 vault-first）＋ 三項體驗/防護修正。全數經 Codex 設計/程式碼交叉審查與實機批測。

### 功能
- **skill/wiki 入庫即出列（Phase 3）**：技能與 wiki 寫入 vault 成功後與記憶一致從佇列**出列**，`Synced` 狀態全面退役（`mark_synced` 移除）；啟動 migration 擴為清全型別殘留 Synced（備份 `queue.premigration-p3.bak` 可回滾）。佇列常態只剩待審與忽略項。
- **promote 反轉 vault-first**：「升級為共用」改純 vault 檔案操作（`promote_memory(project_id, memory_id)`，零佇列參與）——先寫 shared、讀回驗證後才刪專案檔；同 id 嚴格預檢（不可讀受管檔 fail-closed、同 id 異檔名拒寫第二份）；中斷可續跑收斂；衍生物失敗語意明確（內聯 Err 可重試、錨點 warning 回報）。
- **記憶庫頁直讀 vault**：新增 `list_vault_memories` 掃 vault 三層為資料源，修復 Phase 1 後「佇列篩 synced 恆空白、升級鈕無對象」的斷鏈。
- **記憶身分鍵（id frontmatter 最小版）**：記憶檔寫出 `id:`；loader 優先讀 frontmatter id、legacy 檔 fallback 檔名片段；格式守門不變。

### 修正
- **學習去重**：規則式候選對佇列既有項去重，重按「學習變更」冪等、候選不再無限增生。
- **初始化判定**：「已初始化」改以 `.amagi/config.json` 為憑證（原以 `.amagi/` 目錄存在誤判）；init 按鈕常駐、可補齊骨架（冪等）。
- **封鎖項可視化**：學習結果「已封鎖」計數與 pending 技能拆分；審核頁新增唯讀封鎖區塊（遮罩片段＋處置建議＋確認丟棄出列）；狀態機防護——封鎖項不可接受/忽略/編輯，「全部接受」碰不到。

### 安全
- promote 全程非破壞 fail-closed（撞名不同內容/不可讀目標/身分集合不完整一律 Err、不覆寫不刪源）；封鎖項後端唯讀防洗白。經設計審查（6 發現全採納）＋程式碼審三輪（R1/R2/R3）＋兩輪實機批測。

## v0.7.0

記憶同步反轉為 vault-first（杜絕幽靈復活）＋ 全域 doctrine 一鍵部署（跨機零手改）＋ UI 修正。

### 功能
- **記憶同步 vault-first（Phase 1）**：vault 檔案集合為唯一權威；記憶成功寫入 vault 後從佇列**出列**，不再標 `Synced` 長留。索引/內聯/孤兒處理改由 vault 現有檔重建（新增 `read_memory_dir`/`load_*_from_vault`），移除「以佇列集合刪 vault 檔」的清理（非受管檔一律忽略、不刪）；一次性 migration 清佇列殘留 Synced（備份可回滾）。**根治「vault 端刪除記憶、同步又復活」的幽靈。**
- **全域 doctrine 一鍵部署（步驟5）**：新增「同步全域 doctrine」——app 讀 vault `general/_meta/global-agent-config.md`（AMAGI-DOCTRINE 標記界定）整檔部署到本機 `~/.claude/CLAUDE.md`、`~/.codex/AGENTS.md`。fail-closed（render+safety 皆在寫入前）、原子 temp+rename、首次 `.predeploy.bak`＋rolling `.bak`、第二檔失敗交易式回滾、Codex override/32KiB 警告。**跨機只需 pull vault → 按鈕一鍵，零手改。**

### 修正
- **確認對話框**：`window.confirm`（Tauri webview 無效）→ `@tauri-apps/plugin-dialog` 的 `ask`（同步全域部署、移除專案）。
- **同步預覽**：改真 line-diff（LCS，比對現況 vs 新內容）＋ CRLF/LF 正規化＋濾掉無變更檔，修「diff 亂標紅」「同步完仍一直有東西」。
- **dev 自我重載**：vite watch 忽略生成的 agent 檔／`.amagi`，dogfood 同步 app 自身時不再整頁重載。

### 安全
- 記憶/全域檔寫入皆 fail-closed + 備份 + 原子/交易式；非受管檔一律忽略不刪。經 Codex 設計審查 + 多輪程式碼外審收斂。

## v0.6.0

跨機記憶讀取鏈補完 + 工作流生成器統一：修復專案層記憶跨機讀取鏈缺口，並讓 init／sync 共用同一份生成器，杜絕紀律被覆寫。

### 修正
- **跨機記憶讀取鏈缺口回填**：新增 `reconcile_project_memory_from_vault`，sync/preview 前把「vault 有、本機佇列無」的合法專案記憶檔回填佇列，避免 B 機 sync 把 pull 下來的專案記憶當孤兒誤刪；三道守門（frontmatter 格式齊備防洗白、佇列 id 碰撞防護、檔名一致性）+ `created` 時區對稱換算（防負時區漂移）+ 讀檔前 symlink TOCTOU 重驗。

### 功能
- **AGENTS.md／CLAUDE.md 生成器統一**：`init_project` 改呼叫與 `sync` 相同的 `markdown::build_agents_md`／`build_claude_md`，避免「init 寫豐富版、首次 sync 用薄版覆寫」造成紀律漂移；移除過時的 `build_initial_agents_md`／`build_initial_claude_md`。
- **開發工作流薄錨**：專案層 AGENTS.md／CLAUDE.md 新增「開發工作流」薄錨區塊（指向全域 doctrine + 本機軌跡檔），全域 `CLAUDE.md` 不重覆帶。
- **工作流軌跡模板改版**：`.amagi/workflow-state.md` 從勾選式步驟改為「計畫步驟＋逐步結果＋證據欄」，並新增 4.5 交叉審查步驟；完成後歸檔至 `.amagi/history/`。

## v0.5.0

交接（handoff）文件路由規範調整：交接改為各專案一份覆寫式活頁，daily 回歸純每日流水。

### 功能
- **交接落各專案 handoff.md**：handoff 新增專屬桶，落 `projects/<name>/handoff.md`（檔名固定、覆寫式快照、單一真實來源、需指定專案）；取代原「併入頂層 daily/、非破壞」做法。
- **daily 純每日流水**：doc_router 不再自動落 daily；多專案於同日檔內以 `## [專案名]` section 分隔。
- **開場讀取 handoff-first**：專案/全域指針生成器與新專案 index 模板改為先讀 `handoff.md`，再讀 index/knowledge/reports。

### 安全
- handoff 覆寫前以 `symlink_metadata` 攔截 symlink（含 dangling）防越界寫入，其餘 I/O 錯誤 fail-closed（經 Codex 三輪外審收斂）。

## v0.4.1

0.4.0 後整體重測與舊碼安全稽核（兩輪 Codex）後的安全強化與 UI 修正；無新功能。

### 修正
- **衝突偵測強化**：危險 git 操作偵測拓寬——`push --force`／`--force-with-lease` 旗標後置、`-c` 前綴的 `reset --hard` 等原本繞過的寫法皆可命中。
- **全域錨點縱深**：刷新錨點內聯索引前多過一層敏感資訊過濾（命中則略過刷新並回報，不阻斷記憶落 vault）。
- **記憶目錄清理加固**：孤兒清理刪除前重新驗證 canonical 路徑仍在記憶目錄下（symlink/競態輕量防護）。
- **記憶庫頁透鏡**：升級最後一筆專案記憶後，透鏡不再卡在空白下拉，會自動退回「全部」。
- **錯誤訊息可讀**：後端錯誤在 UI 不再顯示「[object Object]」，改顯可讀訊息（差異匯出/工作流/引導式執行等）。
- **日期時區**：frontmatter 顯示日期改用本地時區，修台北凌晨差一天。

## v0.4.0

記憶讀取鏈修復：記憶索引內聯進 AI 必讀的錨點/指針，讓 vault 記憶真的被讀取。

### 功能
- **記憶索引內聯**：實測發現 AI 不會主動跟讀「薄指標」指向的 vault 記憶；改為把記憶**索引**直接內聯進 AI 必讀的檔——全域/共用記憶 → `~/.claude/CLAUDE.md`、`~/.codex/AGENTS.md`；各專案記憶 → 該專案 `CLAUDE.md`/`AGENTS.md`。索引隨同步自動刷新。
- **記憶升級改 queue-first**：升級（專案→共用）先原子定案佇列、再讓 vault 對齊；中途失敗按「同步」即自我校正、不重複。
- **同步對帳清理**：同步時清掉 vault 記憶目錄中的孤兒檔，使「同步＝ vault 完全對齊佇列」。

### 修正
- 記憶索引摘要改取 `description`（原誤抓 frontmatter 分隔線）。
- 記憶標題/摘要含特殊標記時，內聯前中和，避免破壞受管區塊。

### 安全
- 記憶目錄的寫入與刪除統一經安全閘（路徑驗證 + 防越界，fail-closed）；不安全時拒絕並回報，絕不在 vault 外寫/刪。

## v0.3.1

修正：應用程式圖示在工作列、系統匣、視窗標題列的清晰度。

### 修正
- **圖示模糊/雜訊**：更新後工作列圖示模糊、系統匣與標題列出現白點雜訊。
  - 工作列（視窗 ICON_BIG）改餵 256px，取代 Tauri 預設挑到、被放大而糊的小圖。
  - 系統匣改用 32px 小尺寸專用圖（大圖硬縮到 ~16px 會產生鋸齒雜訊）。
  - 標題列（ICON_SMALL, ~16px）以原生 WM_SETICON 另設 32px 小圖，與工作列高解析並存。

## v0.3.0

技能管理改版：雙頁籤、透鏡式分發與取消分發。

### 功能
- **技能管理雙頁籤**：拆為「技能庫」（搜尋 / 已分發篩選 / 精簡條列，點開看完整 SKILL.md）與「技能分發」兩頁；技能變多也不再把版面拖長。
- **透鏡式分發**：選一個目標（全域 / 某專案）當視角，只顯示一欄開關；已分發者排上方、未分發排下方，每列標示目前分發位置。專案再多也只是下拉選項，不擠版。
- **全域涵蓋語意**：勾全域＝本機所有專案（含日後新增）共用；專案視角下全域技能鎖定顯示「全域」、不可單獨取消。
- **取消分發**：可移除已分發的副本，套用前先列出「＋新增 / −移除」差異、確認才執行；只刪分發副本，vault `_skills/` 正本永不受影響。

### 修正
- **技能庫摘要**：修正卡片摘要誤把 frontmatter `name:` 當內文，改優先顯示 `description`。

### 其他
- **切頁防殘影**：技能資料改用 store 快取、啟動預熱，消除每次進「技能管理」頁的載入閃爍。
- 忽略派生技能副本（`.codex/skills`、`.claude/skills`）與 git worktree 殘留，不進版控。

## v0.2.0

vault 知識庫整合：記憶與技能中樞、跨機鑰匙鏈、技能分發與收編。

### 功能
- **vault 知識庫整合**：指定 vault 資料夾為耐久資產單一來源；首次啟動引導（偵測未設→引導設定，建議掛 git）；vault git 同步（pull / commit-push）。
- **跨機鑰匙鏈**：設定 vault 時於本機 `~/.claude/CLAUDE.md` 與 `~/.codex/AGENTS.md` 寫入受管區塊（僅替換標記區、先備份 .bak），讓 AI 對話自動指向本機 vault；專案產生路徑無關的薄指針，換機不錯位。
- **技能單一來源 + 分發**：vault `_skills/` 為技能正本；選擇性分發 UI（技能 × 目標矩陣）分發到全域或指定專案；引導一次性全域分發（新機一鍵就緒）。
- **技能收編**：掃描散落於全域/專案的技能、收編進 vault 單一來源（自動排除官方內建技能，避免過時副本）。
- **文件路由器**：AI 產出的耐久文件依 frontmatter `type` 自動歸入 vault 對應桶（knowledge / reports / 頂層 daily）。
- **知識匯入**：把討論結論、筆記或檔案匯入 vault，經審核後寫入；可掃描 sources/clips 產生候選。
- **Codex 審查工作流**：支援 Step 4.5 外部交叉審查（技能化交辦單 + 報告回寫 vault）。

### 修正
- 設定 vault 成功訊息補上 `~/.codex/AGENTS.md`（原僅報 claude，實際雙寫）。

## v0.1.7

更新殘留自動清理與檔案定位修復。

### 修正
- **檔案定位**：差異匯出頁的檔案定位圖示，修正正反斜線混用導致 `explorer /select` 無法選中檔案（只開到檔案總管首頁）的問題。

### 功能
- **更新殘留自動清理**：app 啟動時自動清除 `%TEMP%` 中殘留的舊更新安裝包（比對自身命名，僅刪自己的），避免隨每次更新累積占用空間。

## v0.1.6

差異匯出中文檔名修復與檔案總管整合。

### 修正
- **差異匯出**：修正中文（及所有非 ASCII）檔名因 git 八進位轉義（`core.quotePath`）導致被誤判「讀取失敗」而略過的問題；改以 `core.quotePath=false` 取得真實 UTF-8 路徑，讀檔與產生 diff 正常。

### 功能
- **檔案總管整合**：差異匯出頁新增「📂 開啟目錄」按鈕（用檔案總管開啟專案根目錄）；各檔案項加定位圖示，可在檔案總管中選中該檔（對中文檔名亦適用）。

## v0.1.5

多主題系統與差異匯出修復。

### 功能
- **多主題**：新增 8 套專業配色——淺色 Daylight／Catppuccin Latte，深色 Midnight／Tokyo Night／Catppuccin Mocha／Nord／Everforest／Rosé Pine——並支援「跟隨系統」。設定頁改為主題畫廊，每套附色板預覽，切換即時換膚；相容舊版明暗設定。

### 修正
- **差異匯出**：修正未追蹤目錄（如 `uploads/`）被 git 折疊成單一項、進而誤報「讀取失敗」並略過的問題。改用 `git status --porcelain -uall` 展開目錄內實際檔案，並對目錄路徑加防呆。
- 修正頁尾版本號寫死為舊值，改用 `getVersion()` 動態顯示。
- 修正主題卡長名（Catppuccin…）在窄容器被裁切。

## v0.1.4

圖標小尺寸鋭化優化。

### 修正
- 小尺寸圖標改採「僅邊緣鋭化」（sharp `m1:0, m2:2`）：拿回 v0.1.3 缺乏的清晰度，同時不放大深色背景雜訊，工具列／開始選單圖標清晰又乾淨。

## v0.1.3

修正應用圖標小尺寸雜點。

### 修正
- 修正 `.ico` 小尺寸層（48px 等）的彩色雜點：改用 sharp 高品質縮放 + `png-to-ico` 組裝，取代會在小尺寸產生雜訊的 `png2icons`。Windows 桌面／開始選單常用的 48px 圖標不再破圖。

## v0.1.2

差異匯出功能、自動更新，以及多項體驗與安全強化。

### 功能
- **差異匯出**：列出專案異動檔（修改／新增／刪除／改名／未追蹤），勾選後產生可複製的 diff 文字，分「異動」與「新增刪除」兩框（JetBrains 風 patch 格式）。
- **自動更新**：整合 `tauri-plugin-updater`，啟動時靜默檢查、有新版顯示橫幅，設定頁亦可手動「檢查更新」；發版經簽章驗證。

### 體驗
- **單一實例**：再次啟動不再另開視窗，而是叫回既有（可能縮在系統匣的）視窗。
- **消除 CMD 黑窗**：呼叫 git／node 等子行程時不再閃出主控台視窗（Windows `CREATE_NO_WINDOW`）。
- **修正雙系統匣圖示**：移除設定檔重複的 tray 設定，右下角只剩一顆。

### 安全
- **敏感內容可檢視**：偵測到疑似機密時，不再「一次擋全部並自動忽略」；改為留一筆「待確認」項，列出觸發規則名與遮罩片段，由使用者判斷真偽（其餘正常候選照常產生）。
- 同步衝突卡控的理由附上命中片段，訊息更具體。

### 內部
- 清除全部編譯警告；新增差異匯出的單元與端對端測試。

## v0.1.1

品牌視覺更新：全新應用圖標與正式 README。

### 介面
- 全新應用圖標（深色圓角方塊、雙 A 標誌與青色發光核心、雙橢圓軌道），統一覆蓋安裝圖標、系統匣、視窗標題列與瀏覽器 favicon 全尺寸。
- 自動去除底圖黑色四角為透明圓角，並裁切透明邊距使圖標占滿畫布，與系統其他圖標大小一致。

### 文件
- README 由 Vite / Tauri 範本改寫為正式專案說明（簡介、主要功能、技術棧、開發與發版流程、圖標工具）。

### 內部
- 新增圖標產生工具：`scripts/gen-icons.mjs`（一鍵產生 Tauri 全尺寸 PNG + `.ico` / `.icns` + favicon）、`scripts/round-corners.mjs`（去除底圖黑色四角並裁切）。

## v0.1.0

首個內部版本。AI 記憶與技能同步管家：在日常使用 Codex / Claude 後，偵測 git 變更、
產生候選記憶與技能草稿，審核通過後同步至 Agent 設定檔。

### 功能
- **專案管理**：加入本機 Git 專案、初始化 `.amagi/` 工作目錄骨架（memory / pending / skills / history / artifacts / state）。
- **學習變更**：規則式掃描 git diff（README、依賴設定、CI/CD、Tauri 設定、Agent 規則），產生候選記憶。
- **審核佇列**：逐項接受／忽略／編輯候選記憶與技能草稿。
- **原生技能同步**：技能以原生 Skills 格式（含 `name` / `description` / `when_to_use` frontmatter）同步至 `.claude/skills/`、`.codex/skills/` 與 `.amagi/skills/`，支援自動觸發。
- **記憶同步**：審核通過的記憶寫入 `AGENTS.md`、`CLAUDE.md`（覆寫前自動備份 `.bak`）。
- **引導式執行（File Bridge）**：分步驟導引任務，逐步留下執行軌跡。

### 安全
- **安全過濾**：偵測疑似密碼／API key／token，封鎖含機密的候選，不允許自動保存。
- **衝突卡控**：同步前偵測違反規則的內容（如 `git config --local user.*`、`Co-Authored-By`、`git push --force` 等），預設擋下，需修正或明確放行。

### 介面
- Linear / Vercel 風格重設計：語意化設計 token、淺色／深色雙主題可切換、側欄分組、克制的靛藍強調色。

### 內部
- Rust 後端單元測試 + 端對端整合測試（對真實檔案系統驗證完整流水線）。
- 自動發版流程：`set-version.mjs` 版本三處同步、`extract-changelog.mjs` 產生 release 內容、GitHub Actions（`tauri-action`）自動打包並發佈安裝器與攜帶版。
