const githubUrl = "https://github.com/Racious/Amagi-Core";
const releasesUrl = `${githubUrl}/releases/latest`;

const capabilities = [
  {
    index: "01",
    title: "從變更中學習",
    description:
      "掃描 Git diff 與專案設定，整理出值得保存的架構決策、開發慣例與技能草稿。",
  },
  {
    index: "02",
    title: "先審核，再寫入",
    description:
      "每一筆候選內容都進入審核佇列。接受、編輯或忽略，由你保有最後決定權。",
  },
  {
    index: "03",
    title: "Vault 單一真實來源",
    description:
      "記憶、技能與規範集中到可版本控制的 Vault，換機後仍能完整恢復工作脈絡。",
  },
  {
    index: "04",
    title: "Claude × Codex 同步",
    description:
      "自動維護 CLAUDE.md、AGENTS.md 與原生 Skills，讓兩套 Agent 讀到一致的專案知識。",
  },
  {
    index: "05",
    title: "安全閘與灰名單",
    description:
      "敏感資訊與危險操作在落地前即被攔截；誤判可精準靜音，不犧牲其他安全命中。",
  },
  {
    index: "06",
    title: "引導式任務軌跡",
    description:
      "複雜任務拆成可驗證步驟，保留計畫、結果與證據，讓完成不只是一個勾選。",
  },
];

const workflow = [
  {
    step: "01",
    label: "Observe",
    title: "觀察專案變更",
    text: "讀取真實 diff、設定與文件，而不是要求你重新解釋整個專案。",
  },
  {
    step: "02",
    label: "Propose",
    title: "提出記憶與技能",
    text: "以候選項呈現可保存的知識，內容與來源清楚可追溯。",
  },
  {
    step: "03",
    label: "Review",
    title: "由你審核定案",
    text: "接受、編輯、忽略或解除灰名單；不讓 AI 越過人的判斷。",
  },
  {
    step: "04",
    label: "Sync",
    title: "同步到每個 Agent",
    text: "以 Vault 為核心寫回 Claude Code 與 Codex 的原生入口與技能。",
  },
];

function GithubMark() {
  return <span aria-hidden="true" className="github-mark">GH</span>;
}

function Arrow() {
  return <span aria-hidden="true" className="button-arrow">↗</span>;
}

export default function Home() {
  return (
    <main>
      <nav className="site-nav" aria-label="主要導覽">
        <a className="brand" href="#top" aria-label="AMAGI Core 首頁">
          <img src="/amagi-core-ui.png" alt="" width="36" height="36" />
          <span>
            <strong>AMAGI</strong>
            <small>CORE</small>
          </span>
        </a>

        <div className="nav-links" aria-label="頁面章節">
          <a href="#capabilities">核心能力</a>
          <a href="#workflow">運作方式</a>
          <a href="#safety">安全設計</a>
        </div>

        <a className="nav-github" href={githubUrl} target="_blank" rel="noreferrer">
          <GithubMark /> GitHub <Arrow />
        </a>
      </nav>

      <section className="hero" id="top">
        <div className="hero-glow" aria-hidden="true" />
        <div className="hero-copy">
          <p className="eyebrow"><span /> AI MEMORY &amp; SKILL ORCHESTRATOR</p>
          <h1>
            讓 AI 真正記得
            <br />
            <em>你的專案。</em>
          </h1>
          <p className="hero-lead">
            AMAGI Core 把每次開發留下的變更，轉成可審核、可同步、可跨機延續的專案記憶與技能。
          </p>
          <div className="hero-actions">
            <a className="button button-primary" href={releasesUrl} target="_blank" rel="noreferrer">
              下載 Windows 最新版 <Arrow />
            </a>
            <a className="button button-secondary" href={githubUrl} target="_blank" rel="noreferrer">
              <GithubMark /> 查看原始碼
            </a>
          </div>
          <div className="platform-note">
            <span className="status-dot" /> Windows 10 / 11
            <span className="divider" /> MSI · NSIS · Portable
          </div>
        </div>

        <div className="hero-console" aria-label="AMAGI Core 工作流程介面示意">
          <div className="console-frame">
            <div className="console-titlebar">
              <div className="window-dots" aria-hidden="true"><i /><i /><i /></div>
              <span>AMAGI Core</span>
              <span className="console-status"><i /> Vault connected</span>
            </div>
            <div className="console-body">
              <aside className="console-sidebar">
                <div className="mini-brand"><img src="/amagi-core-ui.png" alt="" /> A</div>
                <span className="side-label">WORKSPACE</span>
                <span className="side-item active"><b>⌂</b> 總覽</span>
                <span className="side-item"><b>□</b> 專案管理</span>
                <span className="side-label">TASKS</span>
                <span className="side-item"><b>◇</b> 學習變更</span>
                <span className="side-item"><b>✓</b> 審核佇列 <i>3</i></span>
                <span className="side-item"><b>◎</b> 記憶庫</span>
                <span className="side-item"><b>⚡</b> 技能管理</span>
              </aside>
              <div className="console-content">
                <div className="content-heading">
                  <div><small>PROJECT MEMORY</small><strong>審核佇列</strong></div>
                  <span className="console-action">全部接受</span>
                </div>
                <div className="review-card featured">
                  <div className="review-meta"><span>MEMORY</span><time>剛剛</time></div>
                  <strong>Vault-first 同步策略</strong>
                  <p>以 Vault 檔案集合為唯一權威，避免跨機同步後已刪除的記憶再次出現。</p>
                  <div className="review-actions"><span>忽略</span><b>接受並同步</b></div>
                </div>
                <div className="review-card">
                  <div className="review-meta"><span>SKILL</span><time>2 分鐘前</time></div>
                  <strong>guided-smoke-test</strong>
                  <p>逐步陪同完成實機驗證，依客觀證據判斷每個步驟。</p>
                </div>
                <div className="sync-strip">
                  <span><i /> Vault</span><b>→</b><span>CLAUDE.md</span><b>→</b><span>AGENTS.md</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="trust-strip" aria-label="技術與產品特性">
        <span>TAURI 2</span><i />
        <span>RUST</span><i />
        <span>VUE 3</span><i />
        <span>LOCAL FIRST</span><i />
        <span>HUMAN REVIEWED</span>
      </section>

      <section className="section capabilities" id="capabilities">
        <div className="section-heading">
          <p className="eyebrow"><span /> CORE CAPABILITIES</p>
          <h2>把散落的脈絡，<br />變成可持續的系統。</h2>
          <p>不是另一個聊天紀錄。AMAGI Core 管理的是能被 Agent 實際讀取、並可由你治理的長期知識。</p>
        </div>
        <div className="capability-grid">
          {capabilities.map((item) => (
            <article className="capability-card" key={item.index}>
              <span className="card-index">{item.index}</span>
              <div className="card-symbol" aria-hidden="true"><i /></div>
              <h3>{item.title}</h3>
              <p>{item.description}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section workflow" id="workflow">
        <div className="workflow-head">
          <div>
            <p className="eyebrow"><span /> HOW IT WORKS</p>
            <h2>從變更到長期記憶，<br />每一步都有跡可循。</h2>
          </div>
          <p>AMAGI Core 不會在背景偷偷改寫你的規範。每筆知識都經過明確流程，重要操作必須由你確認。</p>
        </div>
        <div className="workflow-track">
          {workflow.map((item) => (
            <article key={item.step}>
              <div className="step-top"><b>{item.step}</b><span>{item.label}</span></div>
              <div className="track-line"><i /></div>
              <h3>{item.title}</h3>
              <p>{item.text}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="section safety" id="safety">
        <div className="safety-copy">
          <p className="eyebrow"><span /> SAFETY BY DESIGN</p>
          <h2>記得更多，<br />不代表放鬆邊界。</h2>
          <p>同步之前，內容必須通過安全過濾、路徑驗證與人工審核。敏感資訊不該成為永久記憶，危險指令也不該被學成習慣。</p>
          <a href={githubUrl} target="_blank" rel="noreferrer">查看專案設計與變更紀錄 <Arrow /></a>
        </div>
        <div className="safety-panel">
          <div className="shield-orbit" aria-hidden="true"><span>✓</span></div>
          <div className="safety-list">
            <div><i>01</i><span><strong>敏感資訊掃描</strong><small>密碼、API Key 與 Token 落地前攔截</small></span><b>ACTIVE</b></div>
            <div><i>02</i><span><strong>路徑防護</strong><small>拒絕越界寫入與 Vault 自我覆寫</small></span><b>ACTIVE</b></div>
            <div><i>03</i><span><strong>人工審核閘</strong><small>候選內容需接受後才進入權威來源</small></span><b>ACTIVE</b></div>
            <div><i>04</i><span><strong>原子寫入與備份</strong><small>關鍵檔案保留復原與失敗收斂能力</small></span><b>ACTIVE</b></div>
          </div>
        </div>
      </section>

      <section className="download-section">
        <div className="download-glow" aria-hidden="true" />
        <div>
          <p className="eyebrow"><span /> READY WHEN YOU ARE</p>
          <h2>讓下一次對話，<br />從理解開始。</h2>
          <p>下載 AMAGI Core，讓 Claude Code 與 Codex 延續同一份專案脈絡。</p>
        </div>
        <div className="download-actions">
          <a className="button button-light" href={releasesUrl} target="_blank" rel="noreferrer">
            前往 GitHub Releases <Arrow />
          </a>
          <small>Windows 10 / 11 · 安裝版與攜帶版</small>
        </div>
      </section>

      <footer>
        <a className="brand" href="#top">
          <img src="/amagi-core-ui.png" alt="" width="32" height="32" />
          <span><strong>AMAGI</strong><small>CORE</small></span>
        </a>
        <p>AI 記憶與技能同步管家</p>
        <div>
          <a href={githubUrl} target="_blank" rel="noreferrer">GitHub</a>
          <a href={releasesUrl} target="_blank" rel="noreferrer">Releases</a>
          <span>© 2026 AMAGI Core</span>
        </div>
      </footer>
    </main>
  );
}
