#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// ── Configuration ─────────────────────────────────────────────────────────────

function getVaultPath(): string {
  const argIdx = process.argv.indexOf("--vault-path");
  if (argIdx !== -1 && process.argv[argIdx + 1]) return process.argv[argIdx + 1];
  if (process.env.JADS_VAULT_PATH) return process.env.JADS_VAULT_PATH;
  // Dev default: vault/ two levels up from mcp-server/src/
  const __dirname = path.dirname(fileURLToPath(import.meta.url));
  return path.join(__dirname, "..", "..", "vault");
}

const VAULT_PATH = getVaultPath();

// ── Types ─────────────────────────────────────────────────────────────────────

interface NoteMeta {
  path: string;
  title: string;
  note_type: string;
  tags: string[];
  status?: string;
  created: string;
  modified: string;
  links: string[];
}

// ── Vault helpers ─────────────────────────────────────────────────────────────

function parseFrontmatter(raw: string): { meta: Record<string, unknown>; body: string } {
  if (!raw.startsWith("---")) return { meta: {}, body: raw };
  const rest = raw.slice(3);
  const endIdx = rest.indexOf("\n---");
  if (endIdx === -1) return { meta: {}, body: raw };
  const fmText = rest.slice(0, endIdx);
  const body = rest.slice(endIdx + 4).trimStart();
  const meta: Record<string, unknown> = {};
  let blockKey: string | null = null;
  const blockItems: string[] = [];

  for (const line of fmText.split("\n")) {
    // Block-style list item: "  - value"
    const blockMatch = line.match(/^\s+-\s+(.+)$/);
    if (blockMatch) {
      if (blockKey) blockItems.push(blockMatch[1].trim().replace(/^["']|["']$/g, ""));
      continue;
    }
    // Flush any pending block list
    if (blockKey && blockItems.length > 0) {
      meta[blockKey] = [...blockItems];
      blockItems.length = 0;
      blockKey = null;
    } else if (blockKey) {
      blockKey = null;
    }
    const colon = line.indexOf(":");
    if (colon === -1) continue;
    const key = line.slice(0, colon).trim();
    const val = line.slice(colon + 1).trim();
    if (!key) continue;
    if (val === "") {
      // Might be the start of a block list
      blockKey = key;
    } else if (val.startsWith("[") && val.endsWith("]")) {
      // Inline array: tags: [a, b, c]
      meta[key] = val.slice(1, -1)
        .split(",")
        .map(s => s.trim().replace(/^["']|["']$/g, ""))
        .filter(Boolean);
    } else {
      meta[key] = val.replace(/^["']|["']$/g, "");
    }
  }
  if (blockKey && blockItems.length > 0) meta[blockKey] = [...blockItems];
  return { meta, body };
}

function extractWikilinks(text: string): string[] {
  return [...new Set([...text.matchAll(/\[\[([^\]]+)\]\]/g)].map(m => m[1].trim()))];
}

function readNoteMeta(filePath: string): NoteMeta | null {
  try {
    const raw = fs.readFileSync(filePath, "utf-8");
    const stat = fs.statSync(filePath);
    const { meta } = parseFrontmatter(raw);
    const title = (meta.title as string) || path.basename(filePath, ".md");
    const note_type = (meta.note_type as string) || "note";
    const tagsRaw = meta.tags;
    const tags: string[] = Array.isArray(tagsRaw)
      ? tagsRaw.map(t => String(t).toLowerCase())
      : [];
    return {
      path: filePath,
      title,
      note_type,
      tags,
      status: meta.status as string | undefined,
      created: (meta.created as string) || "",
      modified: stat.mtime.toISOString().slice(0, 10),
      links: extractWikilinks(raw),
    };
  } catch {
    return null;
  }
}

function scanVault(vaultPath: string): NoteMeta[] {
  const notes: NoteMeta[] = [];
  function walk(dir: string) {
    try {
      for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
        if (entry.isDirectory()) {
          if (entry.name !== "templates") walk(path.join(dir, entry.name));
        } else if (entry.name.endsWith(".md")) {
          const m = readNoteMeta(path.join(dir, entry.name));
          if (m) notes.push(m);
        }
      }
    } catch { /* skip unreadable dirs */ }
  }
  walk(vaultPath);
  return notes.sort((a, b) => b.modified.localeCompare(a.modified));
}

// ── Ollama semantic search ─────────────────────────────────────────────────────

async function ollamaEmbed(text: string): Promise<number[] | null> {
  try {
    const res = await fetch("http://localhost:11434/api/embeddings", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: "nomic-embed-text", prompt: text }),
    });
    if (!res.ok) return null;
    const data = (await res.json()) as { embedding?: number[] };
    return data.embedding ?? null;
  } catch {
    return null;
  }
}

function cosine(a: number[], b: number[]): number {
  let dot = 0, na = 0, nb = 0;
  for (let i = 0; i < a.length; i++) { dot += a[i] * b[i]; na += a[i] ** 2; nb += b[i] ** 2; }
  return dot / (Math.sqrt(na) * Math.sqrt(nb) + 1e-9);
}

async function semanticSearch(
  notes: NoteMeta[],
  query: string,
  topK: number
): Promise<Array<{ path: string; title: string; score: number }>> {
  const qEmb = await ollamaEmbed(query);
  if (!qEmb) {
    // Fallback: title/tag keyword search
    const q = query.toLowerCase();
    return notes
      .filter(n => n.title.toLowerCase().includes(q) || n.tags.some(t => t.includes(q)))
      .slice(0, topK)
      .map(n => ({ path: n.path, title: n.title, score: 0 }));
  }
  // Load embedding cache produced by the Tauri app
  const cachePath = path.join(path.dirname(VAULT_PATH), ".jb_embed_cache.json");
  let cache: Record<string, number[]> = {};
  try { cache = JSON.parse(fs.readFileSync(cachePath, "utf-8")); } catch { /* no cache yet */ }

  const results: Array<{ path: string; title: string; score: number }> = [];
  for (const note of notes) {
    let emb: number[] | null = cache[note.path] ?? null;
    if (!emb) {
      try {
        const snippet = fs.readFileSync(note.path, "utf-8").slice(0, 2000);
        emb = await ollamaEmbed(snippet);
      } catch { /* skip */ }
    }
    if (!emb) continue;
    results.push({ path: note.path, title: note.title, score: cosine(qEmb, emb) });
  }
  return results.sort((a, b) => b.score - a.score).slice(0, topK);
}

// ── MCP Server ────────────────────────────────────────────────────────────────

const server = new Server(
  { name: "jads-brain", version: "1.0.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "list_notes",
      description:
        "List all notes in the vault with metadata (title, type, tags, path, wikilinks). " +
        "Use first to understand what's in the vault. Optionally filter by type or tag.",
      inputSchema: {
        type: "object",
        properties: {
          note_type: {
            type: "string",
            description: "Filter by note type: paper, idea, experiment, tangle, daily, note",
          },
          tag: {
            type: "string",
            description: "Filter by tag (case-insensitive substring)",
          },
        },
      },
    },
    {
      name: "read_note",
      description: "Read the full content and metadata of a note by its file path.",
      inputSchema: {
        type: "object",
        required: ["path"],
        properties: {
          path: { type: "string", description: "Absolute path to the .md file" },
        },
      },
    },
    {
      name: "search_text",
      description: "Full-text search across vault notes. Returns matching notes with a surrounding excerpt.",
      inputSchema: {
        type: "object",
        required: ["query"],
        properties: {
          query: { type: "string" },
          limit: { type: "number", description: "Max results (default 10)" },
        },
      },
    },
    {
      name: "search_semantic",
      description:
        "Semantic similarity search using Ollama embeddings (nomic-embed-text). " +
        "Falls back to keyword search if Ollama is offline.",
      inputSchema: {
        type: "object",
        required: ["query"],
        properties: {
          query: { type: "string" },
          limit: { type: "number", description: "Max results (default 10)" },
        },
      },
    },
    {
      name: "create_note",
      description: "Create a new note in the vault. Returns the path of the created file.",
      inputSchema: {
        type: "object",
        required: ["title", "note_type", "body"],
        properties: {
          title: { type: "string" },
          note_type: {
            type: "string",
            enum: ["idea", "paper", "experiment", "tangle", "note"],
          },
          tags: {
            type: "array",
            items: { type: "string" },
            description: "Tags to apply (lowercased automatically)",
          },
          body: {
            type: "string",
            description: "Markdown body content (without frontmatter)",
          },
        },
      },
    },
    {
      name: "update_note",
      description:
        "Overwrite the full content of an existing note (frontmatter + body). " +
        "Call read_note first to get the current content, modify it, then call update_note.",
      inputSchema: {
        type: "object",
        required: ["path", "content"],
        properties: {
          path: { type: "string" },
          content: {
            type: "string",
            description: "Complete file content including frontmatter",
          },
        },
      },
    },
    {
      name: "add_wikilink",
      description:
        "Append a [[wikilink]] to an existing note (in the ## Liens section or at the end). " +
        "Skips silently if the link is already present.",
      inputSchema: {
        type: "object",
        required: ["path", "link_title"],
        properties: {
          path: { type: "string" },
          link_title: {
            type: "string",
            description: "The title to link to — without brackets",
          },
        },
      },
    },
    {
      name: "read_pdf",
      description:
        "Read the extracted text of a PDF from the Zotero library. " +
        "Find by Zotero item key (e.g. 'NREKA9I6') or by title substring. " +
        "Use list_notes(note_type='paper') to find available paper notes with their Zotero keys.",
      inputSchema: {
        type: "object",
        required: ["query"],
        properties: {
          query: {
            type: "string",
            description: "Zotero item key or title substring",
          },
          max_chars: {
            type: "number",
            description: "Max characters to return (default 6000)",
          },
        },
      },
    },
    {
      name: "search_pdf",
      description:
        "Full-text search across all indexed PDFs. Returns excerpts from matching papers. " +
        "Useful for finding specific methods, results, or citations across the literature.",
      inputSchema: {
        type: "object",
        required: ["query"],
        properties: {
          query: { type: "string" },
          limit: { type: "number", description: "Max results (default 5)" },
        },
      },
    },
  ],
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  const a = (args ?? {}) as Record<string, unknown>;

  switch (name) {

    case "list_notes": {
      let notes = scanVault(VAULT_PATH);
      if (a.note_type) notes = notes.filter(n => n.note_type === String(a.note_type));
      if (a.tag) {
        const q = String(a.tag).toLowerCase();
        notes = notes.filter(n => n.tags.some(t => t.includes(q)));
      }
      return { content: [{ type: "text" as const, text: JSON.stringify(notes, null, 2) }] };
    }

    case "read_note": {
      const filePath = String(a.path);
      if (!fs.existsSync(filePath)) {
        return {
          content: [{ type: "text" as const, text: `Error: not found: ${filePath}` }],
          isError: true,
        };
      }
      const raw = fs.readFileSync(filePath, "utf-8");
      const { meta, body } = parseFrontmatter(raw);
      return {
        content: [{
          type: "text" as const,
          text: JSON.stringify({ path: filePath, meta, body }, null, 2),
        }],
      };
    }

    case "search_text": {
      const query = String(a.query).toLowerCase();
      const limit = Number(a.limit ?? 10);
      const notes = scanVault(VAULT_PATH);
      const results: Array<{ path: string; title: string; excerpt: string }> = [];
      for (const note of notes) {
        if (results.length >= limit) break;
        try {
          const raw = fs.readFileSync(note.path, "utf-8");
          const lower = raw.toLowerCase();
          const idx = lower.indexOf(query);
          if (idx !== -1) {
            const start = Math.max(0, idx - 80);
            const excerpt = raw.slice(start, idx + query.length + 80).replace(/\n/g, " ");
            results.push({ path: note.path, title: note.title, excerpt: `…${excerpt}…` });
          }
        } catch { /* skip */ }
      }
      return { content: [{ type: "text" as const, text: JSON.stringify(results, null, 2) }] };
    }

    case "search_semantic": {
      const query = String(a.query);
      const limit = Number(a.limit ?? 10);
      const notes = scanVault(VAULT_PATH);
      const results = await semanticSearch(notes, query, limit);
      return { content: [{ type: "text" as const, text: JSON.stringify(results, null, 2) }] };
    }

    case "create_note": {
      const title = String(a.title);
      const note_type = String(a.note_type ?? "note");
      const tags: string[] = Array.isArray(a.tags)
        ? (a.tags as unknown[]).map(t => String(t).toLowerCase())
        : [];
      const body = String(a.body ?? "");
      const date = new Date().toISOString().slice(0, 10);
      const slug = title.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
      const dir = path.join(VAULT_PATH, note_type + "s");
      fs.mkdirSync(dir, { recursive: true });
      const filePath = path.join(dir, `${date}-${slug}.md`);
      const tagsLine = tags.length > 0 ? `tags: [${tags.join(", ")}]` : "tags: []";
      const content =
        `---\ntitle: "${title}"\nnote_type: ${note_type}\n${tagsLine}\ncreated: ${date}\n---\n\n${body}`;
      fs.writeFileSync(filePath, content, "utf-8");
      return { content: [{ type: "text" as const, text: `Created: ${filePath}` }] };
    }

    case "update_note": {
      const filePath = String(a.path);
      if (!fs.existsSync(filePath)) {
        return {
          content: [{ type: "text" as const, text: `Error: not found: ${filePath}` }],
          isError: true,
        };
      }
      fs.writeFileSync(filePath, String(a.content), "utf-8");
      return { content: [{ type: "text" as const, text: `Updated: ${filePath}` }] };
    }

    case "add_wikilink": {
      const filePath = String(a.path);
      const linkTitle = String(a.link_title);
      if (!fs.existsSync(filePath)) {
        return {
          content: [{ type: "text" as const, text: `Error: not found: ${filePath}` }],
          isError: true,
        };
      }
      const raw = fs.readFileSync(filePath, "utf-8");
      if (raw.includes(`[[${linkTitle}]]`)) {
        return {
          content: [{ type: "text" as const, text: `[[${linkTitle}]] already present in ${filePath}` }],
        };
      }
      let updated: string;
      const liensIdx = raw.search(/^## Liens\s*$/m);
      if (liensIdx !== -1) {
        // Insert after the ## Liens header line
        const insertAt = raw.indexOf("\n", liensIdx) + 1;
        updated = raw.slice(0, insertAt) + `\n- [[${linkTitle}]]\n` + raw.slice(insertAt);
      } else {
        updated = raw.trimEnd() + `\n\n## Liens\n\n- [[${linkTitle}]]\n`;
      }
      fs.writeFileSync(filePath, updated, "utf-8");
      return {
        content: [{ type: "text" as const, text: `Added [[${linkTitle}]] to ${path.basename(filePath)}` }],
      };
    }

    case "read_pdf": {
      const query = String(a.query);
      const maxChars = Number(a.max_chars ?? 6000);
      const indexPath = path.join(path.dirname(VAULT_PATH), ".jb_pdf_index.json");
      if (!fs.existsSync(indexPath)) {
        return { content: [{ type: "text" as const, text: "PDF index not found. Build it first via Zotero panel → Index PDFs." }] };
      }
      let entries: Record<string, [string, string]> = {};
      try {
        entries = JSON.parse(fs.readFileSync(indexPath, "utf-8")).entries ?? {};
      } catch {
        return { content: [{ type: "text" as const, text: "Error reading PDF index." }], isError: true };
      }
      // Exact key match
      if (entries[query]) {
        const [title, text] = entries[query];
        const out = `**${title}**\n\n${text.slice(0, maxChars)}${text.length > maxChars ? "\n\n[…tronqué]" : ""}`;
        return { content: [{ type: "text" as const, text: out }] };
      }
      // Title substring match
      const ql = query.toLowerCase();
      const match = Object.entries(entries).find(([, [t]]) => t.toLowerCase().includes(ql));
      if (match) {
        const [, [title, text]] = match;
        const out = `**${title}**\n\n${text.slice(0, maxChars)}${text.length > maxChars ? "\n\n[…tronqué]" : ""}`;
        return { content: [{ type: "text" as const, text: out }] };
      }
      return { content: [{ type: "text" as const, text: `Aucun PDF trouvé pour "${query}". Essaie list_notes(note_type='paper') pour voir les clés Zotero.` }] };
    }

    case "search_pdf": {
      const query = String(a.query).toLowerCase();
      const limit = Number(a.limit ?? 5);
      const indexPath = path.join(path.dirname(VAULT_PATH), ".jb_pdf_index.json");
      if (!fs.existsSync(indexPath)) {
        return { content: [{ type: "text" as const, text: "PDF index not found." }] };
      }
      let entries: Record<string, [string, string]> = {};
      try {
        entries = JSON.parse(fs.readFileSync(indexPath, "utf-8")).entries ?? {};
      } catch {
        return { content: [{ type: "text" as const, text: "Error reading PDF index." }], isError: true };
      }
      const results: Array<{ title: string; key: string; excerpt: string }> = [];
      for (const [key, [title, text]] of Object.entries(entries)) {
        if (results.length >= limit) break;
        const lower = text.toLowerCase();
        const idx = lower.indexOf(query);
        if (idx !== -1) {
          const start = Math.max(0, idx - 80);
          const excerpt = text.slice(start, idx + query.length + 200).replace(/\n+/g, " ");
          results.push({ title, key, excerpt: `…${excerpt}…` });
        }
      }
      if (results.length === 0) {
        return { content: [{ type: "text" as const, text: `Aucun PDF ne contient "${query}".` }] };
      }
      const out = results.map(r => `**${r.title}** (clé: ${r.key})\n${r.excerpt}`).join("\n\n---\n\n");
      return { content: [{ type: "text" as const, text: out }] };
    }

    default:
      return {
        content: [{ type: "text" as const, text: `Unknown tool: ${name}` }],
        isError: true,
      };
  }
});

const transport = new StdioServerTransport();
await server.connect(transport);
