# Jad's Brain

A local-first personal knowledge management desktop app built with **Tauri v2 + React + Rust**.  
Notes live on your machine as plain Markdown files. AI features (Claude, Ollama) are optional and never required for basic use.

![Tauri](https://img.shields.io/badge/Tauri-v2-blue) ![React](https://img.shields.io/badge/React-18-61dafb) ![Rust](https://img.shields.io/badge/Rust-2021-orange)

---

## Features

- **Vault** — Markdown notes organized by type (paper, idea, experiment, tangle, daily)
- **Graph** — Interactive D3 knowledge graph with semantic clustering (Ollama embeddings or TF-IDF fallback)
- **Claude AI** — Explore connections, Feynman questions, contradiction detection, PDF summaries, Tangle generation (requires Claude subscription or API key)
- **Zotero integration** — Import papers, extract annotations and references from local Zotero library
- **Multi-vault** — Switch between separate vaults (research project, personal, etc.)
- **MCP server** — Claude can read and write to your vault via Model Context Protocol tools

---

## Prerequisites

### Required

| Tool | Version | Install |
|------|---------|---------|
| **Node.js** | 18+ | [nodejs.org](https://nodejs.org) |
| **Rust + Cargo** | stable | [rustup.rs](https://rustup.rs) |
| **Tauri CLI v2** | latest | `npm install -g @tauri-apps/cli` |

On Windows, Tauri also requires the **WebView2 runtime** (pre-installed on Windows 11; download from Microsoft if on Windows 10).

### Optional (for AI features)

| Tool | Purpose | Install |
|------|---------|---------|
| **Ollama** | Local embeddings for semantic graph + tag suggestions | [ollama.com](https://ollama.com) |
| **nomic-embed-text** model | Semantic similarity | `ollama pull nomic-embed-text` |
| **Claude Code CLI** | AI features via Claude subscription (no API key needed) | `npm install -g @anthropic-ai/claude-code` then `claude /login` |
| **Anthropic API key** | Alternative to Claude CLI | Set in app Settings |

---

## Setup

### 1. Clone the repo

```bash
git clone https://github.com/your-username/jads-brain.git
cd jads-brain
```

### 2. Install frontend dependencies

```bash
npm install
```

### 3. Build the MCP server

The MCP server lets Claude read/write your vault during AI requests.

```bash
cd mcp-server
npm install
npm run build
cd ..
```

### 4. Run in development mode

```bash
npm run tauri dev
```

The app opens automatically. Your vault is created at `vault/` (relative to the project root) on first launch.

### 5. Build for production (optional)

```bash
npm run tauri build
```

The installer is placed in `src-tauri/target/release/bundle/`.

---

## Configuration

All settings are stored in `%LOCALAPPDATA%\jads-brain\` (Windows) — never in the repository.

### Claude AI

Open **Settings (⚙)** in the app:

- **CLI mode** (default, free with Claude subscription): requires `claude` CLI installed and authenticated (`claude /login`)
- **API key mode**: paste your Anthropic API key — billed per token

### Ollama (local embeddings)

1. Install Ollama: `ollama serve`
2. Pull the embedding model: `ollama pull nomic-embed-text`
3. The app detects Ollama automatically on `localhost:11434`

Without Ollama, the graph falls back to TF-IDF similarity (no extra setup needed).

### Zotero

The app auto-detects a local Zotero SQLite database. If not found, set the path manually in Settings (`zotero.sqlite` is usually at `%APPDATA%\Zotero\Zotero\zotero.sqlite`).

---

## Vault structure

```
vault/
├── daily/          # Daily notes (auto-created each day)
├── ideas/          # Idea notes
├── notes/          # General notes
├── papers/         # Paper / literature notes (importable from Zotero)
├── experiments/    # Experiment notes
├── tangle/         # Tangle synthesis notes (multi-note AI summaries)
└── templates/      # Note templates (checked into git)
```

Notes are plain `.md` files with YAML frontmatter:

```markdown
---
title: "My note"
note_type: idea
tags: [cognition, memory]
status: non-lu
interest: 3
created: 2026-01-15
---

Note body here. Link to other notes with [[double brackets]].
```

---

## Multi-vault

Click the vault name in the header to switch between vaults or add a new one.  
Each vault is a fully independent directory. Settings (API key, etc.) are shared globally.

---

## Tech stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri v2 (Rust backend) |
| Frontend | React 18 + TypeScript + Vite |
| State | Zustand |
| Graph | D3 v7 |
| Note rendering | react-markdown + remark-gfm |
| Embeddings | Ollama (nomic-embed-text) + TF-IDF fallback |
| AI | Claude Code CLI or Anthropic REST API |
| MCP | @modelcontextprotocol/sdk |
| Zotero | Local SQLite (rusqlite) |
| PDF indexing | pdf-extract (Rust) |

---

## License

MIT
