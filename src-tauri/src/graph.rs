use crate::NoteMeta;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// Maximum number of non-explicit (tag / tfidf) edges per node.
// Explicit wikilinks are always kept. This prevents dense hubs from drowning the graph.
const MAX_NON_EXPLICIT_PER_NODE: usize = 5;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub note_type: String,
    pub tags: Vec<String>,
    pub weight: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub edge_type: String, // "explicit" | "tag"
    pub weight: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

// Always normalize edge keys so (A,B) and (B,A) are treated as the same edge.
fn edge_key(a: &str, b: &str) -> (String, String) {
    if a <= b { (a.to_string(), b.to_string()) } else { (b.to_string(), a.to_string()) }
}

// Root-level aggregate log files that would otherwise become graph hubs.
// insights.md accumulates every "explore" output and links to almost everything.
const GRAPH_EXCLUDED: &[&str] = &["insights.md"];

pub fn build_graph(notes: &[NoteMeta], tfidf_edges: &[(String, String, f32)]) -> GraphData {
    let notes: Vec<NoteMeta> = notes.iter()
        .filter(|n| {
            let fname = std::path::Path::new(&n.path)
                .file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_default();
            !GRAPH_EXCLUDED.iter().any(|ex| fname.eq_ignore_ascii_case(ex))
        })
        .cloned()
        .collect();

    let title_map: HashMap<String, &str> = notes
        .iter()
        .map(|n| (n.title.to_lowercase(), n.id.as_str()))
        .collect();

    let mut backlinks: HashMap<String, usize> = HashMap::new();
    for note in &notes {
        for link in &note.links {
            if let Some(&target) = title_map.get(&link.to_lowercase()) {
                *backlinks.entry(target.to_string()).or_insert(0) += 1;
            }
        }
    }

    let nodes: Vec<GraphNode> = notes
        .iter()
        .map(|n| GraphNode {
            id: n.id.clone(),
            title: n.title.clone(),
            note_type: n.note_type.clone(),
            tags: n.tags.clone(),
            weight: 1.0 + *backlinks.get(&n.id).unwrap_or(&0) as f32 * 0.8,
        })
        .collect();

    let mut edges: Vec<GraphEdge> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    // Explicit wikilink edges (highest priority — use normalised key so tag/tfidf can't overlap)
    for note in &notes {
        for link in &note.links {
            if let Some(&target_id) = title_map.get(&link.to_lowercase()) {
                if target_id != note.id {
                    let key = edge_key(&note.id, target_id);
                    if seen.insert(key) {
                        edges.push(GraphEdge {
                            source: note.id.clone(),
                            target: target_id.to_string(),
                            edge_type: "explicit".to_string(),
                            weight: 1.0,
                        });
                    }
                }
            }
        }
    }

    // Tag co-occurrence edges
    for i in 0..notes.len() {
        for j in (i + 1)..notes.len() {
            let shared: Vec<_> = notes[i].tags.iter()
                .filter(|t| notes[j].tags.contains(t))
                .collect();
            if shared.len() >= 2 {
                let key = edge_key(&notes[i].id, &notes[j].id);
                if seen.insert(key) {
                    edges.push(GraphEdge {
                        source: notes[i].id.clone(),
                        target: notes[j].id.clone(),
                        edge_type: "tag".to_string(),
                        weight: shared.len() as f32 * 0.4,
                    });
                }
            }
        }
    }

    // Build set of included node IDs so we can filter tfidf edges.
    // Edges referencing excluded notes (e.g. insights.md) would leave D3 with
    // undefined source/target → NaN positions → invisible graph.
    let included_ids: HashSet<&str> = notes.iter().map(|n| n.id.as_str()).collect();

    // TF-IDF semantic similarity edges (weakest — only if no stronger edge exists)
    for (src, tgt, sim) in tfidf_edges {
        if !included_ids.contains(src.as_str()) || !included_ids.contains(tgt.as_str()) {
            continue;
        }
        let key = edge_key(src, tgt);
        if seen.insert(key) {
            edges.push(GraphEdge {
                source: src.clone(),
                target: tgt.clone(),
                edge_type: "tfidf".to_string(),
                weight: *sim,
            });
        }
    }

    GraphData { nodes, edges: prune_edges(edges) }
}

/// Keep all explicit edges; for tag/tfidf edges keep only the strongest N per node.
fn prune_edges(edges: Vec<GraphEdge>) -> Vec<GraphEdge> {
    let mut result: Vec<GraphEdge> = Vec::new();
    let mut non_explicit: Vec<GraphEdge> = Vec::new();

    for edge in edges {
        if edge.edge_type == "explicit" {
            result.push(edge);
        } else {
            non_explicit.push(edge);
        }
    }

    // Strongest non-explicit edges first
    non_explicit.sort_by(|a, b| {
        b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut non_explicit_degree: HashMap<String, usize> = HashMap::new();
    for edge in non_explicit {
        let sd = *non_explicit_degree.get(&edge.source).unwrap_or(&0);
        let td = *non_explicit_degree.get(&edge.target).unwrap_or(&0);
        if sd >= MAX_NON_EXPLICIT_PER_NODE || td >= MAX_NON_EXPLICIT_PER_NODE { continue; }
        *non_explicit_degree.entry(edge.source.clone()).or_insert(0) += 1;
        *non_explicit_degree.entry(edge.target.clone()).or_insert(0) += 1;
        result.push(edge);
    }

    result
}
