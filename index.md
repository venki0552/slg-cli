---
layout: home

hero:
  name: slg
  text: Semantic Lore for Git
  tagline: Transform your git history into a queryable knowledge base. Serve ground truth to AI agents in &lt;200ms — fully local, fully offline.
  actions:
    - theme: brand
      text: Get Started →
      link: /docs/getting-started
    - theme: alt
      text: Commands
      link: /docs/commands
    - theme: alt
      text: GitHub
      link: https://github.com/venki0552/slg

features:
  - icon: ⚡
    title: 95% fewer tokens
    details: Answers history questions with ~200 tokens instead of agents reading 20 files. Results in under 200ms, fully offline.
  - icon: 🤖
    title: MCP native
    details: 5 read-only tools for Claude Code, Cursor, Windsurf, and GitHub Copilot. Auto-registers on VS Code activation.
  - icon: 🔒
    title: Fully local & secure
    details: All data lives in ~/.slg/. Secrets redacted before indexing. CDATA-isolated output. No cloud. No data egress.
  - icon: 🔍
    title: Hybrid search
    details: Vector similarity + BM25 lexical ranking fused with Reciprocal Rank Fusion. Recency, exact-match, and security boosts.
  - icon: 🦀
    title: Single Rust binary
    details: Statically linked, no runtime dependencies. Cross-platform. Ships for Linux, macOS, and Windows.
  - icon: 🧩
    title: VS Code extension
    details: Auto-downloads the binary, indexes on activation, watches for branch changes, and shows live status in the status bar.
---

<div class="home-action-strip">

```bash
# one-time setup
slg init

# ask your repo anything
slg why "why was the retry limit set to 3?"
```

</div>
