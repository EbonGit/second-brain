use crate::NoteMeta;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatMessage {
    pub role: String,   // "user" | "assistant"
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeReply {
    pub text: String,
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

// ── Claude Code CLI (uses subscription) ──────────────────────────────────────

/// Locate the `claude` CLI — returns the full path to the executable, or None.
fn locate_claude() -> Option<String> {
    let appdata   = std::env::var("APPDATA").unwrap_or_default();
    let userprof  = std::env::var("USERPROFILE").unwrap_or_default();
    let localdata = std::env::var("LOCALAPPDATA").unwrap_or_default();
    eprintln!("[locate_claude] APPDATA={:?} UP={:?}", appdata, userprof);

    // Explicit owned Vec — avoids any lifetime issue with &[&str] + temporaries
    let candidates: Vec<String> = vec![
        format!("{}\\npm\\claude.cmd", appdata),
        format!("{}\\AppData\\Roaming\\npm\\claude.cmd", userprof),
        format!("{}\\scoop\\shims\\claude.cmd", userprof),
        format!("{}\\Programs\\claude-code\\claude.exe", localdata),
    ];
    for path in &candidates {
        let exists = std::path::Path::new(path).exists();
        eprintln!("[locate_claude] {:?} → {}", path, exists);
        if exists { return Some(path.clone()); }
    }

    // where.exe fallback (works if claude is in the shell's PATH)
    if let Ok(out) = std::process::Command::new("where.exe").arg("claude").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout);
            for line in s.lines() {
                let t = line.trim();
                if !t.is_empty() {
                    eprintln!("[locate_claude] where.exe → {:?}", t);
                    return Some(t.to_string());
                }
            }
        }
    }

    // Claude Code (desktop app) is an MSIX package on Windows — npm global installs go
    // to AppData\Local\Packages\Claude_*\LocalCache\Roaming\npm\ which is only visible
    // via filesystem virtualisation inside the Claude process, not from the user's shell.
    // We scan the Packages directory directly so we can find it from any process.
    if !localdata.is_empty() {
        let packages = format!("{}\\Packages", localdata);
        if let Ok(entries) = std::fs::read_dir(&packages) {
            for entry in entries.filter_map(|e| e.ok()) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_lowercase().starts_with("claude") {
                    let candidate = format!("{}\\LocalCache\\Roaming\\npm\\claude.cmd",
                                           entry.path().to_string_lossy());
                    eprintln!("[locate_claude] msix check {:?}", candidate);
                    if std::path::Path::new(&candidate).exists() {
                        return Some(candidate);
                    }
                }
            }
        }
    }

    eprintln!("[locate_claude] → None (APPDATA empty={})", appdata.is_empty());
    None
}

/// Write a temporary MCP config file so the `claude --print` subprocess can start
/// jads-brain without relying on a pre-existing interactive-session configuration.
fn write_mcp_config(vault_path: &str) -> Option<std::path::PathBuf> {
    let mcp_server = std::path::Path::new(vault_path)
        .parent()?
        .join("mcp-server")
        .join("dist")
        .join("index.js");
    if !mcp_server.exists() { return None; }

    let config = serde_json::json!({
        "mcpServers": {
            "jads-brain": {
                "command": "node",
                "args": [mcp_server.to_string_lossy()],
                "env": { "JADS_VAULT_PATH": vault_path }
            }
        }
    });
    let tmp = std::env::temp_dir().join("jads-brain-mcp.json");
    if std::fs::write(&tmp, config.to_string()).is_ok() {
        Some(tmp)
    } else {
        None
    }
}

/// Call `claude --print --output-format json` with the prompt piped via stdin.
/// Returns a ClaudeReply with text + token counts (when the JSON output includes them).
/// Always runs via `cmd /C` so .cmd shims and user PATH entries are resolved correctly.
pub async fn call_via_cli(prompt: &str, api_key: Option<&str>, vault_path: Option<&str>) -> anyhow::Result<ClaudeReply> {
    let claude_found = locate_claude();
    let claude_cmd = claude_found.as_deref().unwrap_or("claude");
    eprintln!("[call_via_cli] locate_claude={:?}", claude_found);

    let mcp_config = vault_path.and_then(write_mcp_config);

    let mut args: Vec<&str> = vec!["/C", claude_cmd, "--print", "--output-format", "json",
        "--allowedTools", "mcp__jads-brain__*"];
    let mcp_config_str;
    if let Some(ref path) = mcp_config {
        mcp_config_str = path.to_string_lossy().into_owned();
        args.push("--mcp-config");
        args.push(&mcp_config_str);
    }

    let mut cmd = tokio::process::Command::new("cmd");
    cmd.args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    if let Some(key) = api_key {
        cmd.env("ANTHROPIC_API_KEY", key);
    }

    let mut child = cmd.spawn()
        .map_err(|e| anyhow::anyhow!("Impossible de démarrer cmd.exe: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes()).await?;
    }

    let out = child.wait_with_output().await?;
    let stderr_raw = String::from_utf8_lossy(&out.stderr);
    let stdout_raw = String::from_utf8_lossy(&out.stdout);
    eprintln!("[call_via_cli] exit={} stdout_len={} stderr_len={}",
              out.status, stdout_raw.len(), stderr_raw.len());

    if !out.status.success() {
        // Claude CLI often writes errors to stdout, not stderr
        let error_text = if !stderr_raw.trim().is_empty() {
            stderr_raw.trim().to_string()
        } else {
            stdout_raw.trim().to_string()
        };
        let error_lc = error_text.to_lowercase();
        eprintln!("[call_via_cli] error_text={}", &error_text[..error_text.len().min(300)]);

        if error_lc.contains("not logged in") || error_lc.contains("please run /login")
            || error_lc.contains("login") || error_lc.contains("auth")
        {
            let cli_path = claude_found
                .unwrap_or_else(|| {
                    // Generic fallback: npm global install path for current user
                    let roaming = std::env::var("APPDATA").unwrap_or_default();
                    format!("{roaming}\\npm\\claude.cmd")
                });
            return Err(anyhow::anyhow!(
                "Claude CLI non authentifié.\n\
                 Ouvre cmd.exe et lance :\n\
                   {cli_path}\n\
                 Puis tape `/login` pour connecter ton compte Claude.ai.\n\
                 Relance ensuite Jad's Brain."
            ));
        }
        if error_lc.contains("not recognized") || error_lc.contains("not found")
            || error_lc.contains("cannot find")
        {
            return Err(anyhow::anyhow!(
                "Claude Code CLI introuvable.\n\
                 Installe-le : npm install -g @anthropic-ai/claude-code\n\
                 Ou configure une clé API dans Réglages → Claude (mode Clé API)."
            ));
        }
        if error_text.is_empty() {
            return Err(anyhow::anyhow!(
                "Claude CLI a échoué (code {}). Lance `claude` dans cmd.exe pour vérifier l'auth.",
                out.status
            ));
        }
        return Err(anyhow::anyhow!("Erreur Claude CLI: {}", error_text));
    }

    let trimmed = stdout_raw.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("Claude CLI n'a retourné aucune réponse — vérifie l'authentification."));
    }

    // Try to parse JSON output (--output-format json)
    // The CLI returns { "type": "result", "result": "...", "usage": { ... } }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(text) = json["result"].as_str() {
            if !text.is_empty() {
                let u = &json["usage"];
                // Sum all input token buckets (new + cache creation + cache read)
                let input_tokens = Some(
                    u["input_tokens"].as_u64().unwrap_or(0) as u32
                    + u["cache_creation_input_tokens"].as_u64().unwrap_or(0) as u32
                    + u["cache_read_input_tokens"].as_u64().unwrap_or(0) as u32
                );
                let output_tokens = u["output_tokens"].as_u64().map(|v| v as u32);
                eprintln!("[call_via_cli] parsed JSON: in={:?} out={:?}", input_tokens, output_tokens);
                return Ok(ClaudeReply { text: text.to_string(), input_tokens, output_tokens });
            }
        }
    }

    // Fallback: plain text (in case the CLI version doesn't support --output-format json)
    Ok(ClaudeReply { text: trimmed.to_string(), input_tokens: None, output_tokens: None })
}

// ── Vault tools (tool-use loop for API path) ─────────────────────────────────

fn vault_tools_schema() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "read_note",
            "description": "Read the full content and frontmatter of a vault note. \
                            Use the note's path from the vault index.",
            "input_schema": {
                "type": "object",
                "required": ["path"],
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to the .md file" }
                }
            }
        },
        {
            "name": "search_notes",
            "description": "Full-text search across vault notes. Returns matching notes with surrounding excerpts.",
            "input_schema": {
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "integer", "description": "Max results (default 5)" }
                }
            }
        }
    ])
}

fn execute_vault_tool(name: &str, input: &serde_json::Value, vault_path: &str) -> String {
    match name {
        "read_note" => {
            let path = input["path"].as_str().unwrap_or("");
            std::fs::read_to_string(path)
                .unwrap_or_else(|e| format!("Error reading {path}: {e}"))
        }
        "search_notes" => {
            let query = input["query"].as_str().unwrap_or("").to_lowercase();
            let limit = input["limit"].as_u64().unwrap_or(5) as usize;
            let mut results = Vec::new();
            if let Ok(notes) = crate::vault::scan_vault(vault_path) {
                for note in notes {
                    if results.len() >= limit { break; }
                    if let Ok(content) = std::fs::read_to_string(&note.path) {
                        let lower = content.to_lowercase();
                        if let Some(idx) = lower.find(&query) {
                            let start = idx.saturating_sub(100);
                            let end = (idx + query.len() + 100).min(content.len());
                            let excerpt = content[start..end].replace('\n', " ");
                            results.push(format!("**{}** ({})\n…{}…",
                                note.title, note.path, excerpt));
                        }
                    }
                }
            }
            if results.is_empty() {
                format!("Aucun résultat pour «{query}»")
            } else {
                results.join("\n\n---\n\n")
            }
        }
        _ => format!("Outil inconnu: {name}"),
    }
}

/// Agentic API loop: sends messages + vault tools, executes tool calls until
/// the model responds with `end_turn` (or reaches the iteration limit).
async fn call_api_agentic(
    api_key: &str,
    messages: &[ChatMessage],
    system: Option<&str>,
    vault_path: &str,
) -> anyhow::Result<ClaudeReply> {
    let client = reqwest::Client::new();
    let tools = vault_tools_schema();
    let mut api_msgs: Vec<serde_json::Value> = messages.iter().map(|m| {
        serde_json::json!({"role": m.role, "content": m.content})
    }).collect();
    let mut total_in = 0u32;
    let mut total_out = 0u32;

    for _ in 0..8 {
        let mut body = serde_json::json!({
            "model": "claude-sonnet-4-6",
            "max_tokens": 4096,
            "messages": api_msgs,
            "tools": tools,
        });
        if let Some(s) = system {
            body["system"] = serde_json::Value::String(s.to_string());
        }
        let resp: serde_json::Value = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send().await?.json().await?;

        if let Some(err) = resp.get("error") {
            return Err(anyhow::anyhow!("Claude API: {}",
                err["message"].as_str().unwrap_or("unknown error")));
        }
        total_in  += resp["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        total_out += resp["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;

        let stop = resp["stop_reason"].as_str().unwrap_or("end_turn");
        let content = resp["content"].as_array()
            .ok_or_else(|| anyhow::anyhow!("Réponse inattendue de l'API"))?.clone();

        // Push assistant turn
        api_msgs.push(serde_json::json!({"role": "assistant", "content": content}));

        if stop != "tool_use" {
            let text = content.iter()
                .filter_map(|b| (b["type"] == "text").then(|| b["text"].as_str()).flatten())
                .collect::<Vec<_>>()
                .join("\n");
            return Ok(ClaudeReply {
                text,
                input_tokens: Some(total_in),
                output_tokens: Some(total_out),
            });
        }

        // Execute all tool calls and return results in one user turn
        let results: Vec<serde_json::Value> = content.iter()
            .filter(|b| b["type"] == "tool_use")
            .map(|b| {
                let id   = b["id"].as_str().unwrap_or("").to_string();
                let name = b["name"].as_str().unwrap_or("");
                let out  = execute_vault_tool(name, &b["input"], vault_path);
                serde_json::json!({"type": "tool_result", "tool_use_id": id, "content": out})
            })
            .collect();
        api_msgs.push(serde_json::json!({"role": "user", "content": results}));
    }
    Err(anyhow::anyhow!("Trop de rounds d'outils sans réponse finale"))
}

/// Compact vault index for Claude context.
/// When a summary map is provided, appends the first sentence of each note
/// so Claude has semantic content — not just titles — to reason over.
pub fn vault_context(notes: &[NoteMeta], summary: &HashMap<String, String>) -> String {
    notes.iter().map(|n| {
        let tags = if n.tags.is_empty() {
            String::new()
        } else {
            format!(" [{}]", n.tags.join(", "))
        };
        let snippet = summary.get(&n.path)
            .filter(|s| !s.is_empty())
            .map(|s| {
                let t: String = s.chars().take(150).collect();
                format!(" — {}", t)
            })
            .unwrap_or_default();
        format!("• [{}] {}{}{}", n.note_type, n.title, tags, snippet)
    }).collect::<Vec<_>>().join("\n")
}

/// `api_key` = None → use CLI subscription mode, Some(key) → use REST API directly.
/// In CLI mode the key is also forwarded as ANTHROPIC_API_KEY so an API key works either way.
pub async fn explore_links(
    api_key: Option<&str>,
    use_cli: bool,
    note_title: &str,
    note_body: &str,
    vault_ctx: &str,
) -> anyhow::Result<ClaudeReply> {
    let prompt = format!(
        "Vault index:\n{vault_ctx}\n\n\
        Note actuelle — **{note_title}**:\n{note_body}\n\n\
        Identifie des connexions intellectuelles non-évidentes entre cette note et le vault.\n\
        Organise ta réponse en DEUX sections avec exactement ces titres Markdown :\n\n\
        ## Liens existants\n\
        2-4 notes déjà présentes dans le vault avec lesquelles cette note a une connexion profonde. \
        Pour chacune : cite le titre exact entre [[doubles crochets]] + 1 phrase expliquant le lien.\n\n\
        ## Nouvelles pistes\n\
        1-3 concepts non couverts dans le vault qui émergent de cette note. \
        Formule chaque concept comme titre de note EN ANGLAIS entre [[doubles crochets]] \
        (ex: [[Embodied Cognition]], [[Predictive Processing]]) + 1 phrase de pertinence.\n\n\
        Sois concis, évite les liens triviaux, privilégie les connexions non-évidentes."
    );
    if use_cli {
        call_via_cli(&prompt, api_key, None).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée. Va dans Réglages → Claude."))?;
        call_api(key, &[ChatMessage { role: "user".into(), content: prompt }], None).await
    }
}

/// `api_key` = None → use CLI subscription mode, Some(key) → use REST API directly.
/// In API mode, a tool-use loop lets Claude call `read_note` and `search_notes` autonomously.
pub async fn chat(
    api_key: Option<&str>,
    use_cli: bool,
    history: Vec<ChatMessage>,
    vault_ctx: &str,
    vault_path: &str,
) -> anyhow::Result<ClaudeReply> {
    let system = format!(
        "Tu es un assistant de recherche scientifique intégré dans un second cerveau local. \
        Tu as accès à l'index du vault de l'utilisateur:\n\n{vault_ctx}\n\n\
        Réponds de façon concise et précise. Quand tu fais référence à une note du vault, \
        cite son titre entre [[doubles crochets]]. Si l'utilisateur demande des connexions, \
        cherche des liens non-évidents entre les concepts. \
        Tu disposes des outils MCP jads-brain : read_note(path), search_text(query), \
        search_semantic(query), list_notes(note_type?, tag?), create_note(title, note_type, body), \
        add_wikilink(path, link_title). Utilise-les pour lire le contenu complet d'une note \
        ou chercher quelque chose de précis dans le vault avant de répondre."
    );
    if use_cli {
        let mut prompt = format!("Contexte système:\n{system}\n\n---\n\n");
        for msg in &history {
            let role = if msg.role == "user" { "Utilisateur" } else { "Assistant" };
            prompt.push_str(&format!("{role}: {}\n\n", msg.content));
        }
        prompt.push_str("Réponds à la dernière question de l'utilisateur.");
        call_via_cli(&prompt, api_key, Some(vault_path)).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée. Va dans Réglages → Claude."))?;
        call_api_agentic(key, &history, Some(&system), vault_path).await
    }
}

async fn call_api(api_key: &str, messages: &[ChatMessage], system: Option<&str>) -> anyhow::Result<ClaudeReply> {
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "max_tokens": 1024,
        "messages": messages,
    });
    if let Some(s) = system {
        body["system"] = serde_json::Value::String(s.to_string());
    }
    let resp: serde_json::Value = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send().await?.json().await?;
    extract_reply(&resp)
}

pub async fn feynman(
    api_key: Option<&str>,
    use_cli: bool,
    note_title: &str,
    note_body: &str,
) -> anyhow::Result<ClaudeReply> {
    let prompt = format!(
        "Note : **{note_title}**\n\n{note_body}\n\n\
        Génère exactement 5 questions de Feynman pour tester la compréhension profonde de ce sujet.\n\
        Ces questions doivent :\n\
        - Forcer à expliquer les mécanismes fondamentaux (pas juste les faits)\n\
        - Mettre en lumière les présupposés implicites\n\
        - Relier le concept à des exemples concrets ou analogies\n\n\
        Format : liste numérotée 1-5. Questions courtes et directes.\n\
        Réponds en français."
    );
    if use_cli {
        call_via_cli(&prompt, api_key, None).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée."))?;
        call_api(key, &[ChatMessage { role: "user".into(), content: prompt }], None).await
    }
}

pub async fn contradictions(
    api_key: Option<&str>,
    use_cli: bool,
    note_title: &str,
    note_body: &str,
    vault_ctx: &str,
) -> anyhow::Result<ClaudeReply> {
    let prompt = format!(
        "Index vault:\n{vault_ctx}\n\n\
        Note : **{note_title}**\n\n{note_body}\n\n\
        Identifie des tensions intellectuelles ou contradictions :\n\
        1. Entre cette note et d'autres notes du vault\n\
        2. En interne dans la note elle-même\n\n\
        Format : liste à puces, 3-5 tensions maximum. Pour chaque tension :\n\
        « **Thèse A** (cite [[titre note]]) vs **Thèse B** : explication en 1 phrase. »\n\n\
        Si aucune contradiction n'existe, dis-le explicitement.\n\
        Réponds en français."
    );
    if use_cli {
        call_via_cli(&prompt, api_key, None).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée."))?;
        call_api(key, &[ChatMessage { role: "user".into(), content: prompt }], None).await
    }
}

pub async fn tangle(
    api_key: Option<&str>,
    use_cli: bool,
    notes_content: &str,
    vault_path: Option<&str>,
) -> anyhow::Result<ClaudeReply> {
    let prompt = format!(
        "Synthétise ces notes en un Tangle — une note de synthèse créative qui fait émerger \
        des idées nouvelles de la tension entre ces sources.\n\n\
        Notes sélectionnées :\n{notes_content}\n\n\
        Rédige un Tangle (~300 mots) qui :\n\
        1. Identifie le fil conducteur entre ces notes\n\
        2. Met en tension leurs idées divergentes\n\
        3. Fait émerger 1-2 idées ou hypothèses nouvelles\n\
        4. Cite les notes sources entre [[doubles crochets]]\n\n\
        Commence directement par le contenu. Pas de titre ni de préambule.\n\
        Réponds en français."
    );
    if use_cli {
        call_via_cli(&prompt, api_key, vault_path).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée."))?;
        call_api(key, &[ChatMessage { role: "user".into(), content: prompt }], None).await
    }
}

pub async fn generate_note_stub(
    api_key: Option<&str>,
    use_cli: bool,
    title: &str,
    cited_by: &[String],
) -> anyhow::Result<ClaudeReply> {
    let cited_str = if cited_by.is_empty() {
        "notes du vault".to_string()
    } else {
        cited_by.iter().map(|s| format!("[[{s}]]")).collect::<Vec<_>>().join(", ")
    };
    let prompt = format!(
        "Génère une note stub pour le concept «{title}».\n\
        Ce concept est cité dans : {cited_str}.\n\n\
        Rédige une note courte (~150 mots) :\n\
        1. Définition concise du concept\n\
        2. Pertinence dans ce contexte de recherche\n\
        3. 2-3 pistes d'exploration ou sous-questions\n\n\
        Commence directement par le contenu. Pas de titre ni de frontmatter.\n\
        Réponds en français."
    );
    if use_cli {
        call_via_cli(&prompt, api_key, None).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée."))?;
        call_api(key, &[ChatMessage { role: "user".into(), content: prompt }], None).await
    }
}

pub async fn pdf_summary(
    api_key: Option<&str>,
    use_cli: bool,
    pdf_title: &str,
    pdf_text: &str,
) -> anyhow::Result<ClaudeReply> {
    let truncated = &pdf_text[..pdf_text.len().min(8000)];
    let prompt = format!(
        "Article : **{pdf_title}**\n\n{truncated}\n\n\
        Génère un résumé approfondi de cet article scientifique avec ces sections :\n\n\
        ## Résumé\n\
        2-3 phrases capturant l'essence de l'article.\n\n\
        ## Contributions principales\n\
        3-5 points clés. Méthodes, résultats, mécanismes découverts.\n\n\
        ## Méthodologie\n\
        1-2 phrases sur l'approche expérimentale ou théorique.\n\n\
        ## Implications & Connexions\n\
        Liens avec d'autres domaines, questions ouvertes, implications pratiques.\n\n\
        Réponds en français."
    );
    if use_cli {
        call_via_cli(&prompt, api_key, None).await
    } else {
        let key = api_key.ok_or_else(|| anyhow::anyhow!("Clé Anthropic non configurée."))?;
        call_api(key, &[ChatMessage { role: "user".into(), content: prompt }], None).await
    }
}

fn extract_reply(resp: &serde_json::Value) -> anyhow::Result<ClaudeReply> {
    if let Some(err) = resp.get("error") {
        let msg = err["message"].as_str().unwrap_or("Unknown error");
        return Err(anyhow::anyhow!("Claude API error: {}", msg));
    }
    let text = resp["content"][0]["text"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow::anyhow!("Unexpected response shape: {}", resp))?;
    let input_tokens  = resp["usage"]["input_tokens"].as_u64().map(|v| v as u32);
    let output_tokens = resp["usage"]["output_tokens"].as_u64().map(|v| v as u32);
    Ok(ClaudeReply { text, input_tokens, output_tokens })
}
