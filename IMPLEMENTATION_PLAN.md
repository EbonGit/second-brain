# Jad's Brain — Plan d'implémentation des 17 features

> Règle générale : **Ollama** tourne librement ; **Claude** = toujours 1 clic explicite.

---

## Phase A — Fondations (algo pur, sans IA)

### A1. Score de maturité automatique (#4)
- Rust : `maturity_score: u8` (0–100) dans `scan_vault` — liens entrants/sortants + longueur + tags + interest
- Seuils : seed < 30, developing < 65, mature ≥ 65
- Affichage : badge coloré dans NoteListPanel + footer éditeur

### A2. Wikilinks fantômes (#3, partie algo)
- Rust : `get_ghost_links()` — scan `[[Titre]]`, retourne ceux sans note correspondante
- Graphe : nœuds fantômes en pointillés gris avec compteur "cité par N notes"
- **Claude optionnel** : bouton "Générer avec Claude" sur le modal fantôme (Phase D)

### A3. Daily note automatique (#17)
- Rust : à `init_vault`, créer `daily/YYYY-MM-DD.md` si elle n'existe pas
- `AppState.modified_today: Mutex<Vec<String>>` — `save_note` enregistre chaque path
- Fermeture ou bouton manuel → append liste dans la daily note

### A4. Full-text search PDFs (#13)
- Rust : crate `pdf-extract` pour extraire le texte des PDFs Zotero
- Cache `.jb_pdf_index.json` (zotero_key → texte brut)
- Commande `search_pdf_fulltext(query)` → excerpts ±200 chars
- UI : onglet "PDF" dans la barre de recherche

---

## Phase B — Graphe et découverte (Ollama, automatique)

### B1. Clustering automatique (#1)
- Rust : k-means simplifié sur embeddings Ollama (k=3–8 auto)
- `cluster_id` dans `GraphNode`
- Graphe : contour coloré par cluster + légende

### B2. Vue "focus" (#5)
- Frontend D3 : double-clic → filtre 2-hop neighbors, nœuds hors périmètre à opacity 0.1
- Bouton "↩ Vue complète" pour sortir

### B3. Recherche sémantique (#6)
- Rust : `semantic_search(query, top_k)` — embed via Ollama + cosine similarity sur cache
- UI : icône "≈" dans barre de recherche pour switcher mode texte/sémantique

### B4. Auto-tagging (#19)
- Déclenché après `save_note` si 0 tags
- Rust : K voisins par embedding, agrège tags pondérés → top 3 suggestions
- UI : bandeau discret + bouton "Accepter"

---

## Phase C — Intelligence de fond (Ollama, automatique)

### C1. Incubateur de connexions (#7)
- Après `save_note`, re-embedding + comparaison avec notes des 30 derniers jours
- Si cosine > 0.72 et connexion nouvelle → `AppState.incubator_notifs`
- UI : badge orange sur bouton graphe

### C2. Résumé vault compressé (#20)
- `.jb_vault_summary.json` : première phrase non-vide par note
- `vault_context()` : mode compact quand vault > 150 notes (8-12 tokens/note)
- Au-delà 300 notes : pre-filtre TF-IDF/embedding avant d'envoyer à Claude

### C3. Journal d'insights (#21)
- Append dans `insights.md` à chaque résultat Claude "Explorer les liens"
- Format : `## 2026-09-26\n**[Source]** → [[Lien]] : explication\n`

---

## Phase D — Claude 1-clic

### D1. Résumé profond de PDF (#11)
- Rust : `extract_pdf_text(pdf_path)` → texte (max 80k chars)
- Bouton "Résumer PDF avec Claude" dans ZoteroPanel
- Claude : titre + auteurs + texte → résumé structuré (contribution / méthode / résultats / limites)

### D2. Extraction de références (#12, algo pur)
- Rust : regex sur section "References"/"Bibliography" du texte PDF
- Parser : auteurs + titre + année → `Vec<ParsedRef>`
- Matching fuzzy avec vault (Levenshtein) et Zotero

### D3. Questions de révision (#8)
- Bouton "◎ Questions" dans toolbar éditeur
- Claude → 4-5 questions Feynman dans ExploreDrawer

### D4. Détection de contradictions (#9)
- Bouton "⚡ Analyser tensions" dans header ou ChatPanel
- Claude reçoit vault context + Tangle Nodes existants

### D5. Génération de Tangle (#15)
- Mode sélection multi-notes dans NoteListPanel
- Bouton "◈ Créer Tangle" quand ≥ 2 notes cochées

### D6. Création d'objets depuis le chat (#16)
- Claude retourne blocs ```create-note``` / ```create-tangle``` → bouton "Créer" dans la bulle
- Couvre : note, tangle, experiment, manuscrit

### D7. Génération de note depuis fantôme (#3, partie Claude)
- Modal ghost link → bouton "Générer avec Claude" (titre pré-rempli + context notes)

---

## Ordre d'exécution

```
Phase A (fondations algo)     → session 1-2
Phase B (graphe Ollama)       → session 3-4
Phase C (intelligence fond)   → session 5
Phase D (Claude 1-clic)       → session 6-8
```
