# Étude ×10 — Rendre codescope dix fois plus utile et plus simple

> **Date** : 2026-07-03 · Étude **produit/UX**, complémentaire de l'[audit technique](./audit-architecture.md) et de l'[étude vector store](./etude-vector-store.md).
> **Question posée** : qu'est-ce qui rendrait codescope ×10 plus utile et plus simple, pour son utilisateur principal (les agents IA) et pour les humains qui l'installent ?

---

## 1. Constat : le produit est bon, le parcours le freine

La proposition de valeur est réelle et mesurée (cf. `tests/comparison/` : ~5-10× moins de tokens qu'un workflow grep/cat pour une même tâche). Mais la valeur est enfermée derrière un parcours qui la dilue :

### Le funnel actuel et ses pertes

| Étape | Aujourd'hui | Friction |
|---|---|---|
| 1. Installer | Télécharger un binaire depuis la page Releases, l'extraire, le mettre dans le PATH | Pas de one-liner, pas de brew/scoop/winget, pas de `npx` |
| 2. Initialiser | `codescope init` | Une commande de plus qui n'apporte rien à l'utilisateur |
| 3. Indexer | `codescope index` → téléchargement du modèle (~100-500 Mo) **puis** embedding de tout le repo **avant** la première recherche | Time-to-first-result de plusieurs minutes sur un vrai repo ; l'utilisateur part |
| 4. Brancher l'agent | `codescope agent-setup` = injection de ~120 lignes d'instructions dans CLAUDE.md/.cursorrules | **Le maillon faible** : dépend de l'obéissance du LLM, consomme du contexte en permanence, se périme, ne marche pas pour les agents sans fichier d'instructions |
| 5. Rester à jour | Penser à relancer `codescope index` | Les résultats deviennent stales **silencieusement** — pire qu'une erreur |

Chaque étape perd des utilisateurs ou de la valeur. Le ×10 n'est pas dans une feature de recherche de plus : il est dans **la suppression de ce funnel** et dans **l'élévation du niveau des réponses**.

### Ce que veut réellement l'utilisateur primaire (un agent IA)

Un agent ne veut pas « un moteur de recherche » ; il veut, au milieu d'une tâche : *« donne-moi le bon contexte de ce repo, dans mon budget de tokens, sans que j'aie à orchestrer init/index/modèle/fraîcheur »*. Tout ce qui l'éloigne de ça (états d'erreur, index périmé, discipline de flags) est de la charge cognitive facturée en tokens et en échecs.

---

## 2. Les dix leviers, classés par impact

### 🥇 L1 — Serveur MCP intégré : `codescope mcp` (impact ×10 à lui seul)

**Constat** : l'intégration actuelle par prompt (`agent-setup`) est la partie la plus fragile du produit. En 2026, le standard d'outillage des agents (Claude Code, Cursor, Windsurf, Copilot, agents SDK) est **MCP** — un outil exposé nativement est *toujours* disponible, typé, découvrable, sans consommer le contexte système.

**Proposition** : un sous-commande `codescope mcp` (transport stdio) exposant des **tools** :
- `search_code(query, top_k?, type?, path_filter?, max_tokens?)` → résultats compacts
- `trace_calls(symbol, direction, depth?)` → callers/callees
- `get_symbol(symbol)` / `get_outline(path)` → définitions et cartes (cf. L4)
- `index_status()` → fraîcheur, stats
- L'indexation est **implicite** (cf. L2) : le tool ne renvoie jamais « lance d'abord codescope index », il le fait.

`agent-setup` pivote : au lieu de coller 120 lignes de prompt, il **enregistre le serveur MCP** (`.mcp.json`, `claude mcp add`, config Cursor/Windsurf) + 5 lignes d'instructions max. Le prompt actuel reste en fallback pour les environnements sans MCP.

**Pré-requis techniques** : quasi tous déjà planifiés — `IndexPipeline` en core (epic 17.3), moteur réutilisable. Le process MCP étant long-vécu, il garde le modèle ONNX chargé → les recherches sémantiques passent de ~1-2 s (démarrage à froid du CLI) à ~10 ms. Crate `rmcp` (SDK officiel Rust).
**Effort** : ~1,5 semaine. **Dépend de** : epic 17 phase 0.

### 🥈 L2 — Zéro-config : la première recherche fait tout (« it just works »)

**Constat** : init → index → search, trois commandes pour obtenir la première valeur ; chaque état intermédiaire a ses erreurs (« Not in a codescope project », « model not found », « dimension mismatch »).

**Proposition** :
- `codescope search "..."` dans un repo vierge → auto-init (profil auto-détecté) + indexation lexicale à la volée + réponse. Zéro configuration, une commande.
- Avant chaque recherche : contrôle de fraîcheur éclair (scan mtime — la mécanique `ChangeDetector` existe) ; si stale → ré-index incrémental automatique (rapide) ; opt-out `--no-refresh`.
- `init`/`index` restent pour le contrôle fin, mais deviennent optionnels dans le parcours nominal.

**Effort** : ~1 semaine (surtout de l'orchestration — `IndexPipeline` la rend triviale). **Impact** : le README passe de 3 commandes à 1 ; plus aucun état d'erreur « pas initialisé/pas indexé ».

### 🥉 L3 — Time-to-first-result : indexation progressive

**Constat** : aujourd'hui, la première recherche attend le téléchargement du modèle **et** l'embedding complet. Or l'index lexical (Tantivy) est prêt en secondes et répond déjà à 70 % des requêtes d'agents.

**Proposition** :
- `index` publie le lexical dès qu'il est prêt et **continue les embeddings en arrière-plan** (ou à la prochaine invocation) ; le téléchargement du modèle devient non-bloquant.
- `search --type hybrid` **dégrade proprement en lexical** tant que les vecteurs manquent, avec un champ JSONL explicite (`"semantic": "pending (43%)"`) au lieu d'une erreur.
- Objectif mesurable : **première recherche < 30 s** après installation sur un repo de 10k fichiers (vs plusieurs minutes aujourd'hui).

**Effort** : ~1 semaine (le découplage embed/pipeline est déjà l'action A4 de l'audit — même chantier).

### L4 — Monter d'un niveau d'abstraction : `map`, `def`, `refs`, `context`

**Constat** : `search` répond à « où est-ce que ça parle de X ? ». Les agents posent trois autres questions constamment, qu'ils bricolent aujourd'hui en plusieurs allers-retours : *comment ce repo est-il organisé ? où est défini ce symbole ? donne-moi tout le contexte utile autour de ce point.*

**Proposition** — quatre commandes/tools au-dessus des données **déjà en base** (chunks, symbols, kinds, imports, call_sites) :
- `codescope map [path] [--depth N] [--budget 1500]` : carte du repo en symboles (fichiers → fonctions/classes signatures), compacte, bornée en tokens — l'équivalent du « repo map » d'aider. C'est LE premier appel d'un agent qui découvre un repo.
- `codescope def <symbol>` : la ou les définitions exactes (déjà quasi gratuit : `find_chunks_by_symbol` existe).
- `codescope refs <symbol>` : les usages (call_sites + lexical fallback).
- `codescope context <symbol|file:line> --budget 2000` : **bundle prêt-à-prompter** = définition + appelants directs + appelés + imports du fichier, assemblé et tronqué pour tenir le budget. Remplace 4-6 allers-retours d'agent par un seul appel.

**Effort** : ~1,5 semaine pour les quatre (la donnée existe, c'est de l'assemblage + format). **Impact** : c'est ce qui différencie « un grep sémantique » d'« un outil de compréhension de code ».

### L5 — Budget de tokens de bout en bout

**Constat** : `--compact`/`--excerpt-lines` existent (epic 15) mais l'agent doit encore choisir les bons flags et deviner combien de résultats tiennent dans son budget.

**Proposition** : `--max-tokens N` sur `search`/`map`/`context` : codescope compose la réponse (nombre de résultats, longueur des snippets) pour tenir sous N tokens (estimation ~4 chars/token), et l'annonce (`"est_tokens": 1850`). Défaut raisonnable en mode MCP.

**Effort** : 2-3 j. **Impact** : supprime la dernière décision que l'agent doit prendre.

### L6 — Installation en une ligne

**Constat** : le téléchargement manuel depuis Releases est la première marche, et elle est haute — surtout pour l'utilisateur qui veut juste essayer.

**Proposition** :
- `curl -fsSL https://…/install.sh | sh` (Linux/macOS) + `irm https://…/install.ps1 | iex` (Windows) — détection OS/arch, checksum.
- `cargo binstall codescope`, tap Homebrew, bucket Scoop/winget.
- Wrapper npm `npx codescope@latest` (post-install télécharge le binaire) — l'écosystème agents est massivement JS ; `npx` est le chemin de moindre friction pour un premier essai.

**Effort** : ~1 semaine (scripts + CI release). **Impact** : le taux de conversion « curieux → utilisateur » se joue là.

### L7 — Fraîcheur continue : `codescope watch` (epic 10, à prioriser)

Le mode watch (déjà spécifié dans l'epic 10) est la réponse de fond au problème de staleness, et le compagnon naturel du serveur MCP (le process long-vécu **est** le watcher idéal : `codescope mcp --watch`). À re-prioriser juste après L1-L3 plutôt que « pending » en fin de backlog.
**Effort** : déjà chiffré epic 10.

### L8 — Qualité des résultats (le ×10 silencieux)

Améliorations de ranking à fort ratio valeur/effort, toutes sur des données déjà présentes :
- **Filtres** : `--path src/`, `--lang rust`, `--kind function` (les colonnes existent en base ; aucun filtre n'est exposé aujourd'hui).
- **Boosts** : match exact de symbole (déjà partiel dans rerank.rs), match sur le nom de fichier, malus configurable pour `tests/`/`vendor/`.
- **Récence git** : booster légèrement les fichiers récemment modifiés (`git log` au moment de l'index) — les agents cherchent surtout du code vivant.
- **Modèle code-specific en option** : le défaut actuel (paraphrase-multilingual, 384d) est généraliste ; proposer un modèle entraîné sur du code (ex. jina-embeddings-code, 768d) via le registry ouvert (A20) pour les repos où la qualité prime.

**Effort** : 1-2 j par item, indépendants.

### L9 — Diagnostics : `codescope doctor` et erreurs actionnables

- `codescope doctor` : vérifie binaire, modèle (présence/checksums), index (cohérence — le check de `status` existe depuis la PR #21), config, et **propose la commande de réparation** pour chaque problème.
- Erreurs machine-lisibles sur stderr en JSONL (`{"error": "...", "fix": "codescope index --all"}`) + exit codes documentés et stables — un agent peut alors s'auto-réparer sans intervention humaine.

**Effort** : 2-3 j.

### L10 — Multi-repo / workspace

`codescope search --repos ../lib,../api "..."` ou un fichier de workspace global (`~/.codescope/workspaces.toml`) : les agents travaillent de plus en plus en multi-repos (app + lib + infra). Fusion des résultats avec préfixe de repo.
**Effort** : ~1 semaine. À faire après l'epic 17 (l'abstraction stockage aide).

---

## 3. Matrice impact / effort

```text
Impact ▲
  ×10  │  L1 MCP ◉            L2 zéro-config ◉
       │            L3 progressif ◉
   ×5  │  L4 map/context ◉        L7 watch ◉
       │  L5 budget ◉   L6 install ◉
   ×2  │  L8 ranking ◉  L9 doctor ◉      L10 multi-repo ◉
       └──────────────────────────────────────────────▶ Effort
          2-3 j        ~1 sem           ~1,5 sem +
```

## 4. Feuille de route proposée

| Vague | Contenu | Effort | Résultat mesurable |
|---|---|---|---|
| **A — « It just works »** | L2 zéro-config + L3 progressif + L9 doctor | ~2,5 sem | 1 commande au lieu de 3 ; première recherche < 30 s ; zéro erreur d'état |
| **B — « Natif agents »** | L1 MCP + L5 budget + refonte `agent-setup` | ~2 sem | codescope = tools natifs dans Claude Code/Cursor ; recherches ~10 ms à chaud |
| **C — « Comprendre, pas chercher »** | L4 map/def/refs/context + L8 filtres/boosts | ~2 sem | un appel remplace 4-6 allers-retours d'agent |
| **D — « Partout, toujours frais »** | L6 install one-liner + L7 watch (epic 10) + L10 multi-repo | ~2,5 sem | `npx codescope` fonctionne ; index jamais stale |

**Articulation avec l'existant** : la vague A partage ses fondations avec l'epic 17 phase 0 (`IndexPipeline`) et l'action A4 de l'audit (embeddings découplés) — les faire dans cet ordre évite tout travail jeté. L'epic 17 (backends) peut avancer en parallèle des vagues B/C.

## 5. Mesures de succès (avant/après)

| KPI | Aujourd'hui | Cible |
|---|---|---|
| Commandes avant la première réponse | 3 (+ install manuel) | **1** |
| Time-to-first-result (repo 10k fichiers) | minutes (modèle + embed) | **< 30 s** (lexical immédiat) |
| Latence search à chaud (agent MCP) | ~1-2 s (cold start CLI + modèle) | **~10-50 ms** |
| Intégration agent | ~120 lignes de prompt à maintenir | enregistrement MCP en 1 commande |
| Allers-retours agent pour « comprendre X » | 4-6 (search + reads) | **1-2** (`context`, `map`) |
| Résultats périmés | silencieux | impossibles (auto-refresh / watch) |

## 6. Ce que cette étude ne recommande PAS

Par honnêteté sur le « simple d'utilisation » :
- **Pas d'interface web/TUI** : le consommateur est un agent ou un terminal ; toute UI serait de la maintenance sans servir la cible.
- **Pas de cloud/télémétrie** : l'offline-first est un avantage compétitif différenciant, pas une dette.
- **Pas de nouveau langage de requête** : le langage naturel + 3 filtres suffisent ; un DSL ajouterait de la charge cognitive — exactement l'inverse du but.
