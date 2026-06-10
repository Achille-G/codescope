# Audit d'architecture et de scalabilité — codescope

> **Date** : 2026-06-10 · **Version auditée** : 0.2.0 (`main` @ `9d4fd10`)
> **Périmètre** : les 5 crates du workspace (~12 900 lignes de Rust hors tests/benches), CI, tests, documentation.
> Les améliorations recommandées sont détaillées dans [`amelioration.md`](./amelioration.md).

---

## 1. Vue d'ensemble

**codescope** est un CLI de recherche de code structurelle et sémantique, offline et multi-OS, dont le consommateur principal est un agent IA (sortie JSONL). Il combine trois index : SQLite (métadonnées), Tantivy (BM25 lexical) et usearch/HNSW (ANN sémantique via embeddings ONNX), fusionnés par Reciprocal Rank Fusion.

### Verdict global

| Axe | Note | Commentaire |
|---|---|---|
| Architecture / découpage | ★★★★☆ | Couches claires, dépendances unidirectionnelles, API publiques minimales |
| Design patterns | ★★★★☆ | Patterns idiomatiques et justifiés, peu de sur-ingénierie |
| Bonnes pratiques | ★★★★☆ | thiserror/anyhow, tracing, CI 3 OS avec clippy `-D warnings`, 0 `unsafe` |
| Tests | ★★★☆☆ | ~310 tests + intégration CLI, mais peu de cas limites et aucun test de charge |
| Scalabilité | ★★☆☆☆ | Solide jusqu'à ~100k fichiers ; au-delà, plusieurs goulets identifiés (§6) |
| Robustesse opérationnelle | ★★★☆☆ | Pas de cohérence transactionnelle inter-index, pas de retry réseau, checksums inactifs |

**Synthèse** : projet bien structuré et idiomatique, au-dessus de la moyenne pour un projet de cette taille. Les faiblesses sont concentrées sur la **scalabilité des structures en mémoire** (HNSW, tombstones), la **cohérence entre les trois index** et quelques **inefficacités du pipeline d'indexation**. Aucun refactor majeur n'est nécessaire : les corrections sont localisées.

---

## 2. Architecture

### 2.1 Découpage en crates

```
codescope-cli ──► codescope-core ──► codescope-parser   (tree-sitter, chunking)
      │                  ├─────────► codescope-embed    (ONNX, tokenizer, registry)
      └────────────────► codescope-search              (SQLite + Tantivy + HNSW + fusion)
```

- Dépendances strictement unidirectionnelles, aucun cycle. `parser`, `embed` et `search` sont indépendants entre eux — bonne testabilité et compilation incrémentale.
- Chaque crate expose une API publique minimale via `lib.rs` (7 exports pour parser, 9 pour embed) et possède son propre enum `Error` (thiserror) avec alias `Result<T>`.
- Petit accroc au schéma annoncé dans CLAUDE.md : le CLI dépend **directement** de `codescope-search` (`index.rs:8`, `search.rs`), pas uniquement de core. La façade core est donc partiellement contournée.

### 2.2 Pipeline d'indexation (`codescope index`)

```
Walker (ignore/.gitignore, filtre 1 Mo)
  └► ChangeDetector (XXH3 + mtime + size, SQLite WAL)
       └► FileReader  ──[crossbeam channel borné]──► FileParser (tree-sitter, N threads)
            └► boucle principale (CLI, mono-thread) :
                 SQLite (upsert file, imports, chunks, call_sites)
                 + Tantivy add_document
                 + EmbeddingPipeline → HNSW add
       └► résolution des call sites (tous les fichiers)
```

**Points forts** :
- Pipeline streaming producteur-consommateur avec **backpressure** via channels bornés (`file_reader.rs:157-189`) : la mémoire reste bornée même sur de très gros dépôts.
- Détection incrémentale économe : mtime+size d'abord, hash seulement si changement (`change_detector.rs`), pragmas SQLite WAL + `synchronous=NORMAL` adaptés.
- Threads dimensionnés par `Profile` (light/default/heavy) selon les cœurs disponibles (`profile.rs`).

**Points faibles** (détaillés §6) : consommateur mono-thread qui sérialise SQLite + Tantivy + embeddings + HNSW ; insertions SQLite unitaires en autocommit ; fichiers relus une seconde fois pour le hash (`index.rs:204`) alors que `FileReader` détient déjà le contenu ; résolution des call sites refaite sur **tous** les fichiers à chaque indexation incrémentale (`index.rs:312-325`).

### 2.3 Pipeline de recherche (`codescope search`)

`SearchEngine` (façade, `engine.rs:46-190`) ouvre les trois index et expose `search_lexical`, `search_semantic_by_vector`, `search_hybrid`. Le mode hybride interroge BM25 et HNSW (top_k chacun), fusionne par RRF (k=60) ou fusion pondérée, hydrate les résultats depuis SQLite, applique un rerank (match de symbole, proximité de fichier) puis la déduplication par chevauchement de lignes.

L'implémentation RRF est correcte et robuste (basée sur les rangs, insensible aux échelles de scores). Deux réserves : la fusion pondérée normalise min/max par liste, instable quand une liste n'a qu'un résultat (`fusion.rs:99-101`) ; et la fusion opère sur top_k candidats par source seulement, alors que la pratique standard est de récupérer 2-3× top_k avant fusion pour améliorer le rappel.

### 2.4 Responsabilités des modules clés

| Module | Rôle | Évaluation |
|---|---|---|
| `core/walker.rs` | Découverte fichiers (.gitignore + .codescopeignore) | Simple, robuste |
| `core/file_reader.rs` | Lecture + parsing concurrents, channels bornés | Excellent — meilleure pièce du projet |
| `core/change_detector.rs` | Détection incrémentale add/mod/del | Bon |
| `core/profile.rs` | Profils light/default/heavy (threads, mémoire, M/ef HNSW) | Bon concept ; estimations mémoire optimistes |
| `core/memory.rs` | MemoryTracker (AtomicU64) + MemoryBudget + guard RAII | Bien conçu mais **jamais utilisé pour limiter** (observabilité seule) |
| `core/call_graph.rs` | BFS callers/callees, sorties JSONL/DOT | Correct ; 1 requête SQL par nœud (§6.5) |
| `parser/parser.rs` | Pool de parsers tree-sitter par langue (Mutex), chunking AST + fallback 500 lignes/overlap 50 | Bon ; mono-thread par fichier (parallélisme assuré au niveau core) |
| `embed/pipeline.rs` | Préprocess → tokenize (padding fixe) → inférence batch → normalisation L2 | Bon ; limite 8 Ko de préprocess codée en dur |
| `embed/registry.rs` + `download.rs` | 2 modèles codés en dur, téléchargement atomique (tmp + rename) | Sécurisé mais checksums `None` ⇒ jamais vérifiés, pas de retry |
| `search/storage.rs` | SQLite + pool de connexions (parking_lot Mutex/Condvar), `prepare_cached` généralisé | Bon ; fichier très gros (~2 500 lignes, résolution multi-langage incluse) |
| `search/bm25.rs` | Schéma Tantivy 5 champs, writer unique | OK ; merge policy par défaut, `get_field().unwrap()` ×4 |
| `search/hnsw.rs` | usearch cosine f32, tombstones HashSet, persistance binaire + .meta versionné, mmap optionnel | OK ; compaction manuelle uniquement |

---

## 3. Design patterns

### 3.1 Patterns identifiés (et justifiés)

| Pattern | Localisation | Remarque |
|---|---|---|
| **Façade** | `SearchEngine` (engine.rs), `core/embedding.rs` | Masque la complexité des 3 index / d'ONNX |
| **Strategy** | trait `Embedder` (embedder.rs:10), `FusionStrategy`, extracteurs par langage (parser.rs:172-191) | `Embedder` n'a qu'une implémentation réelle (`OnnxEmbedder`) + un mock — abstraction légère, acceptable car l'extension multi-provider est planifiée (epic 13) |
| **Repository** | `Storage`, `ChangeDetector` | Pas d'interface trait ⇒ tests couplés à SQLite réel |
| **Factory** | `Project::init/open/find`, `ModelRegistry::ensure_model` | `find` remonte jusqu'à la racine, élégant |
| **Pipeline / producteur-consommateur** | `FileReader`/`FileParser` + crossbeam | Backpressure native, choix sync (std::thread) pertinent pour du CPU-bound |
| **Object pool** | `StoragePool` (connexions SQLite), pool de parsers tree-sitter par langue | Condvar wait/notify propre |
| **RAII** | `MemoryGuard` (memory.rs) | Bien écrit mais inutilisé en pratique |
| **Tombstone** | `HNSWIndex::mark_deleted` + `compact()` | Conforme au design annoncé ; déclenchement manuel seulement |
| **Registry** | `ModelRegistry` | Fermé : 2 modèles codés en dur, pas d'extensibilité sans recompilation |

### 3.2 Anti-patterns et écarts

1. **Logique métier dans le CLI** — `commands/index.rs` (362 lignes) orchestre tout le pipeline d'indexation (storage + BM25 + HNSW + embeddings + résolution) mêlé aux progress bars. Cette orchestration devrait vivre dans core (ex. `IndexPipeline`), le CLI ne gardant que l'affichage. Conséquence actuelle : la logique n'est testable qu'en intégration, et inutilisable par un futur daemon (epic 10).
2. **Pas d'abstraction de stockage** — core et CLI dépendent du type concret `Storage` ; aucun trait ne permet de mocker ni de préparer l'epic 14 (Postgres/pgvector).
3. **`unwrap()` sur le schéma Tantivy** — `schema.get_field("…").unwrap()` ×4 (`bm25.rs:37-41`) : infaillible aujourd'hui, fragile si le schéma évolue (à centraliser dans une struct de champs construite une fois).
4. **Constantes magiques contournant les profils** — heap Tantivy d'écriture codé en dur à 200 Mo (`index.rs:122`) alors que `Profile` définit cette valeur ; dimension fallback 384 (`index.rs:143`) ; limite préprocess 8 Ko (`pipeline.rs:33`) ; snippet 12 lignes (`engine.rs:173`).
5. **Dépendances mortes** — `tokio` déclaré par le CLI, `rayon` par parser et embed : aucun usage dans le code. Temps de compilation et surface d'audit inutiles.

---

## 4. Bonnes pratiques

### 4.1 Conformes ✅

- **Erreurs** : thiserror par crate + anyhow avec `.context(...)` côté CLI ; affichage hiérarchique des causes ; aucun `unsafe` dans tout le workspace. Les ~250 `unwrap()` restants sont à ~95 % dans les tests ; les deux signalés à risque (`import.rs:542`, `onnx.rs:289`) sont en réalité protégés par des gardes en amont — faux positifs.
- **Logging** : `tracing` structuré, filtres par module, logs sur stderr (stdout réservé au JSONL), mode quiet.
- **CI** : GitHub Actions sur Linux/macOS/Windows — check, fmt, clippy `-D warnings`, tests ; workflow release séparé.
- **Documentation** : ~290 docstrings, README, `docs/cli.md`, plan d'implémentation en 16 epics ; CLAUDE.md à jour.
- **Téléchargement de modèles** : écriture atomique (fichier temporaire + rename), nettoyage sur erreur, URLs de repli, vérification SHA-256 implémentée (mais voir 4.2).
- **Concurrence** : `parking_lot` partout, `AtomicU64` relaxed pour les compteurs, channels bornés ; le choix **sync** (pas de tokio) est correct pour une charge CPU-bound.
- **Tokenisation** : padding fixe (shapes statiques ONNX fiables), troncature `LongestFirst`, normalisation L2 des embeddings.

### 4.2 Écarts ❌

- **Checksums inactifs** : `ModelInfo.model_sha256 = None` pour les deux modèles (`registry.rs:74, 91`) ⇒ le code de vérification SHA-256 n'est jamais exercé. Un binaire ONNX corrompu ou altéré serait chargé sans détection.
- **Pas de retry réseau** : `download.rs` fait une seule tentative par URL (timeout fixe 300 s, pas de reprise partielle).
- **Budget mémoire non appliqué** : `MemoryBudget` calcule une répartition (10 % lecture, 25 % embed, 25 % HNSW…) mais aucun composant ne la consulte ; le tracker est purement observationnel.
- **Pas de validation de config** : clés TOML inconnues silencieusement ignorées ; pas d'override par variables d'environnement (gênant en CI/Docker).
- **Tests** : bonne base (29 tests dans search, ~310 au total, intégration CLI avec fixtures, benches Criterion pour BM25/HNSW), mais pas de tests de cas limites (fichier vide, code malformé, UTF-8 invalide), pas de tests de pannes partielles inter-index, pas de test de charge, pas de test ONNX réel.
- **Erreurs avalées** : `serde_json::to_string().unwrap_or_default()` (`result.rs:79,93,103`) produit une ligne JSONL vide en cas d'échec de sérialisation, sans signal.

---

## 5. Cohérence des index (fiabilité)

C'est le **risque de correction le plus important** du projet : aucune transaction ne couvre les trois index.

- Pendant `codescope index`, chaque chunk est écrit dans SQLite, puis Tantivy, puis HNSW (`index.rs:239-286`). Un crash ou une erreur au milieu laisse les index **désynchronisés** : chunks SQLite sans document BM25, vecteurs HNSW orphelins, ou inverse.
- `bm25.end_write()` et `hnsw.save()` n'interviennent qu'en fin de run (`index.rs:306-307`) : un crash avant perd tout le travail Tantivy/HNSW alors que SQLite et le ChangeDetector ont déjà enregistré les fichiers comme indexés ⇒ la prochaine indexation incrémentale **ne les reprendra pas** (corruption silencieuse durable, seul `index --all` répare).
- À la recherche, `hydrate_results` traite un chunk_id manquant comme une erreur fatale (`engine.rs:169-171`) : une désynchronisation rend la recherche entièrement inopérante au lieu de dégrader proprement.

Mitigation actuelle : aucune (ni journal d'indexation, ni marqueur dirty, ni vérification de cohérence dans `status`).

---

## 6. Scalabilité

### 6.1 Capacité par étage (dépôt de 100k fichiers / ~1M chunks)

| Étage | Comportement | Limite |
|---|---|---|
| Walker | O(N), streaming | OK |
| Lecture/parsing | N threads, mémoire bornée par channels | OK (CPU-bound, scale avec les cœurs) |
| **Écriture SQLite** | 1 INSERT autocommit par chunk/import/call_site, mono-thread | **Goulet n°1** : ~1M+ transactions implicites ; WAL aide mais chaque statement paie son commit |
| **Embeddings** | Batch ≤ taille du fichier courant, inférence dans la boucle consommatrice | **Goulet n°2** : batches sous-remplis (fichiers à 1-5 chunks), et toute la chaîne lecture/parsing s'arrête pendant l'inférence ONNX |
| Tantivy | Writer 200 Mo, merge policy par défaut | Acceptable ; à régler au-delà de quelques M de docs |
| **HNSW** | Intégralement en mémoire (usearch), f32 | **Goulet n°3** : 1M vecteurs × 384 d × 4 o ≈ 1,5 Go + graphe (M=24-32) ⇒ dépasse les budgets des profils light/default ; pas de quantisation (i8/f16) ni de chargement paresseux — mmap existe à la lecture (`engine.rs / hnsw.rs:192-194`) mais pas exploité par défaut |
| **Résolution call sites** | Re-balaye tous les fichiers à chaque run | **Goulet n°4** : coût O(total) même pour un commit d'un fichier |

### 6.2 Mémoire

- Les estimations de `profile.rs:214-223` (512 Mo / 1 Go / 2 Go) ne tiennent pas compte du coût réel du graphe HNSW ; en profil default (M=24), 1M de vecteurs ≈ 2+ Go.
- Tombstones HNSW : `HashSet<u64>` en mémoire, sérialisé dans `.meta`, filtré à chaque recherche (`hnsw.rs:113-115`). Sans compaction automatique, un dépôt très churné accumule indéfiniment des tombstones : mémoire, latence de recherche (les voisins supprimés consomment du top_k) et qualité de rappel se dégradent. `compact()` existe (`hnsw.rs:149-159`, suppression séquentielle) mais **rien ne le déclenche**.

### 6.3 Latence de recherche

- BM25 : O(log segments), sub-ms à l'échelle testée (benches 5 000 docs).
- HNSW : ef_search=100 par défaut, correct jusqu'à plusieurs M de vecteurs.
- Hydratation : 1 requête SQLite par hit (N+1), mais N=top_k (10-50) avec `prepare_cached` ⇒ acceptable.
- Déduplication : O(N²) par fichier sur les résultats retournés ⇒ négligeable à top_k≤100.

### 6.4 Inefficacités du pipeline d'indexation

1. **Double lecture** : `std::fs::read` pour recalculer le hash (`index.rs:204`) alors que `FileReader` a déjà le contenu — 2× les I/O disque.
2. **Batching d'embeddings par fichier** : `new_chunk_ids.chunks(embed_batch_size)` (`index.rs:276-278`) ne franchit jamais la frontière du fichier ; un dépôt de petits fichiers fait de l'inférence en batches de 1-3 au lieu de 16-32 (padding fixe ⇒ le coût d'un batch de 1 ≈ celui d'un batch plein par séquence).
3. **Consommateur mono-thread** : SQLite, Tantivy, ONNX et HNSW sont sérialisés dans la même boucle ; les threads de parsing attendent dès que le channel se remplit.
4. **Préprocess 8 Ko / 512 tokens non coordonnés** : la coupe à 8 Ko (`pipeline.rs:33`) et la troncature tokenizer ne sont pas alignées ; le split d'identifiants (camelCase → mots) augmente le nombre de tokens et aggrave la troncature des gros chunks.

### 6.5 Call graph (`trace`)

BFS correct avec garde de cycles (`call_graph.rs:76-146`) ; complexité O(V+E) mais **une requête SQLite par nœud visité** (`get_callees`/`get_callers`). Sur un graphe dense (profondeur 3-4, fan-out élevé), la latence est dominée par les allers-retours DB — un préchargement par lots (JOIN sur la frontière) la réduirait d'un ordre de grandeur.

---

## 7. Tableau récapitulatif des constats

| # | Constat | Sévérité | Référence |
|---|---|---|---|
| C1 | Aucune cohérence transactionnelle entre SQLite / Tantivy / HNSW ; crash ⇒ index désynchronisés non réparés par l'incrémental | 🔴 Haute | `index.rs:199-307`, `engine.rs:128-162` |
| C2 | Insertions SQLite unitaires sans transaction englobante (autocommit par statement) | 🔴 Haute | `index.rs:208-260`, `storage.rs:348` |
| C3 | HNSW entièrement en mémoire, estimations de profils irréalistes au-delà de ~500k vecteurs | 🔴 Haute | `hnsw.rs`, `profile.rs:214-223` |
| C4 | Tombstones HNSW sans compaction automatique | 🟠 Moyenne | `hnsw.rs:23-24, 149-159` |
| C5 | Batching d'embeddings borné au fichier + inférence bloquant le pipeline | 🟠 Moyenne | `index.rs:275-286` |
| C6 | Résolution des call sites sur tous les fichiers à chaque index incrémental | 🟠 Moyenne | `index.rs:312-325` |
| C7 | Checksums de modèles jamais vérifiés (sha256=None) ; pas de retry de téléchargement | 🟠 Moyenne | `registry.rs:74,91`, `download.rs` |
| C8 | Orchestration métier dans le CLI (non réutilisable par le futur daemon, testable seulement en intégration) | 🟠 Moyenne | `commands/index.rs` |
| C9 | Budget mémoire calculé mais jamais appliqué | 🟠 Moyenne | `memory.rs` |
| C10 | Double lecture des fichiers pour le hash | 🟡 Basse | `index.rs:204` |
| C11 | Constantes magiques court-circuitant les profils (heap 200 Mo, dims 384, 8 Ko, 12 lignes) | 🟡 Basse | `index.rs:122,143`, `pipeline.rs:33` |
| C12 | `get_field().unwrap()` ×4 ; erreurs de sérialisation JSONL avalées | 🟡 Basse | `bm25.rs:37-41`, `result.rs:79,93` |
| C13 | Dépendances mortes (tokio CLI, rayon parser/embed) | 🟡 Basse | `Cargo.toml` des crates |
| C14 | Call graph : 1 requête SQL par nœud BFS | 🟡 Basse | `call_graph.rs:106-144` |
| C15 | Fusion pondérée instable sur liste à 1 résultat ; fusion sur top_k seulement (rappel) | 🟡 Basse | `fusion.rs:99-101`, `engine.rs:137-151` |
| C16 | Pas de merge policy Tantivy explicite | 🟡 Basse | `bm25.rs` |
| C17 | Registry de modèles fermé (2 modèles codés en dur) | 🟡 Basse | `registry.rs:63-98` |

---

## 8. Conclusion

L'architecture de codescope est saine : découpage en crates exemplaire, pipeline streaming avec backpressure, patterns idiomatiques, hygiène de code (CI stricte, zéro unsafe, erreurs typées) au-dessus de la moyenne. Le projet tient ses promesses jusqu'à l'échelle d'un monorepo moyen (~50-100k fichiers).

Les trois chantiers qui conditionnent le passage à l'échelle et la fiabilité en production sont : **(1)** la cohérence inter-index après crash, **(2)** le débit d'écriture du pipeline d'indexation (transactions SQLite, batching d'embeddings global, découplage de l'inférence), **(3)** l'empreinte mémoire du HNSW (quantisation, mmap, compaction automatique). Tous trois sont réalisables sans refonte — voir [`amelioration.md`](./amelioration.md) pour le plan d'action priorisé.
