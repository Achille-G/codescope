# Epic 17: Vector Store Migration (LanceDB embarqué + ChromaDB optionnel)

**Status**: 🔴 Not Started
**Prérequis de lecture** : [`docs/etude-vector-store.md`](../etude-vector-store.md) (décisions actées), [`docs/audit-architecture.md`](../audit-architecture.md) (C3, C4), `docs/amelioration.md` (A5, A10, A11).

## Description

Remplacer le stockage vectoriel usearch (en RAM, tombstones manuels) par un vector store moderne, derrière une abstraction :

- **Backend par défaut : LanceDB** (embarqué, zéro installation utilisateur, index ANN sur disque, deletes natifs, metadata + filtres).
- **Backend optionnel : ChromaDB** (serveur local **auto-provisionné** par codescope : téléchargement du binaire, cycle de vie géré — l'utilisateur n'installe rien).
- SQLite est conservé pour le relationnel (call graph, états de fichiers) mais passe derrière un trait `MetadataStore` ; le **contenu des chunks migre dans le vector store** (source unique d'hydratation).
- Migration automatique et guidée des index existants (format v1 → v2).

## Décisions actées (ne pas re-débattre pendant l'implémentation)

1. Défaut = LanceDB ; Chroma = opt-in via `config.toml` (`[vector_store] backend = "chroma"`).
2. Le trait `VectorStore` couvre vecteurs **et** métadonnées/contenu des chunks (l'hydratation des résultats ne passe plus par SQLite).
3. Tantivy reste l'index lexical ; SQLite reste pour call graph + change detection (derrière `MetadataStore`).
4. Versionnage du format d'index dans `.codescope/` (`index_version`) avec migration à l'ouverture.
5. Aucune dépendance réseau au moment de la recherche ; pour Chroma, le seul accès réseau est le téléchargement initial du binaire (checksum SHA-256 épinglé, même mécanisme que les modèles ONNX).

## Consignes d'exécution pour les agents

- **Ordre** : Phase 0 → Phase 1 → (Phase 2 ∥ Phase 3.1) → Phase 3. Les tickets marqués ∥ sont parallélisables entre agents (fichiers disjoints).
- **Une PR par ticket** (ou par paire de tickets étroitement liés), branchée depuis `dev`, un commit par ticket, CI verte exigée (`cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`, 3 OS).
- **Compat descendante** : jusqu'au ticket 17.12, `cargo build` sans features doit produire exactement le comportement actuel (usearch). Tout nouveau backend arrive derrière un feature flag ou une valeur de config.
- **Interdits** : ne pas modifier le format JSONL de sortie de `search` ; ne pas casser `codescope trace` ; ne pas introduire de dépendance réseau dans le chemin de recherche.
- **Tests hors-ligne** : la CI n'a pas toujours accès au réseau — les tests du provisioner Chroma utilisent un faux binaire (script) et un serveur HTTP mock ; aucun test ne télécharge le vrai binaire.
- En cas d'ambiguïté non couverte par « Décisions actées », ouvrir une question dans la PR plutôt que de trancher silencieusement.

---

## Phase 0 — Abstractions (fondations, ~1 semaine)

### 17.1 Trait `VectorStore` 🔴

**Goal** : isoler tout usage de vecteurs derrière un trait objet-safe.

**Contexte** : usages actuels de `HNSWIndex` dans `crates/codescope-cli/src/commands/index.rs` (add/mark_deleted/save), `crates/codescope-search/src/engine.rs` (search/len), `crates/codescope-cli/src/commands/status.rs`.

**Contrat** (dans `crates/codescope-search/src/vector_store/mod.rs`) :

```rust
pub struct ChunkRecord {
    pub chunk_id: i64,
    pub file_path: String,
    pub symbol: Option<String>,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
}

pub struct VectorHit { pub chunk_id: i64, pub score: f32 }

pub trait VectorStore: Send + Sync {
    fn upsert(&mut self, records: &[ChunkRecord], vectors: &[Option<Vec<f32>>]) -> Result<()>;
    fn delete_by_chunk_ids(&mut self, chunk_ids: &[i64]) -> Result<()>;
    fn delete_by_file(&mut self, file_path: &str) -> Result<()>;
    fn search(&self, query: &[f32], top_k: usize) -> Result<Vec<VectorHit>>;
    fn get_chunks(&self, chunk_ids: &[i64]) -> Result<Vec<Option<ChunkRecord>>>; // hydratation
    fn len(&self) -> Result<usize>;
    fn dimensions(&self) -> Option<usize>;
    fn flush(&mut self) -> Result<()>;      // rend durable (remplace hnsw.save)
    fn maintain(&mut self) -> Result<()>;   // compaction/optimize, best-effort
}
```

**Tasks** :
- [ ] Créer le module `vector_store` avec le trait + types ci-dessus.
- [ ] Implémenter `UsearchStore` (adaptateur du `HNSWIndex` actuel + lecture SQLite pour `get_chunks`) — comportement identique à aujourd'hui.
- [ ] Brancher `SearchEngine` et `commands/index.rs` sur `Box<dyn VectorStore>` ; supprimer les appels directs à `HNSWIndex` hors de l'adaptateur.
- [ ] `vectors: &[Option<Vec<f32>>]` : `None` quand les embeddings sont désactivés (le store n'indexe alors que le contenu/metadata).

**Acceptance criteria** : aucun changement de comportement observable ; `cargo test --workspace` vert ; grep `HNSWIndex` ne matche plus que `vector_store/usearch.rs` + tests.

**Estimation** : 2 j.

---

### 17.2 Trait `MetadataStore` (A11) ∥ 🔴

**Goal** : le reste du code ne dépend plus du type concret `Storage` (SQLite) — préparation au découplage et à l'epic 14.

**Contexte** : `crates/codescope-search/src/storage.rs` (~2 500 lignes) ; consommateurs : `commands/index.rs`, `commands/trace.rs`, `codescope-core/src/call_graph.rs`, `engine.rs` (stats).

**Tasks** :
- [ ] Extraire l'API réellement consommée dans un trait `MetadataStore` (upsert_file, insert_chunk, delete_chunks_for_file, insert/resolve call sites, get_callees/get_callers, stats, transaction).
- [ ] Découper `storage.rs` en modules : `schema.rs`, `files.rs`, `chunks.rs`, `call_sites.rs`, `resolve/` (par langage) — sans changement fonctionnel.
- [ ] `impl MetadataStore for SqliteStore` (renommage de `Storage`, alias public conservé pour compat).

**Acceptance criteria** : diff de comportement nul ; tests existants inchangés et verts ; aucun fichier > 800 lignes dans le module storage.

**Estimation** : 2 j.

---

### 17.3 Extraction `IndexPipeline` vers core (A10) 🔴

**Goal** : sortir l'orchestration d'indexation du CLI (`commands/index.rs`, ~400 lignes) vers `codescope-core::IndexPipeline`, pour qu'elle soit testable unitairement et réutilisable (daemon epic 10, tests de migration 17.6).

**Contrat** :

```rust
pub struct IndexPipeline { /* détient MetadataStore, BM25, Box<dyn VectorStore>, EmbeddingPipeline?, ChangeDetector */ }
pub trait IndexObserver { fn on_stage(&self, stage: Stage); fn on_file(&self, done: usize, total: usize, last: &str); }
impl IndexPipeline {
    pub fn open(project: &Project, opts: IndexOptions) -> Result<Self>;
    pub fn run(&mut self, observer: &dyn IndexObserver) -> Result<IndexReport>;
}
```

**Tasks** :
- [ ] Déplacer la logique (marqueur dirty, deletions, boucle parse→store→embed, commits, mises à jour différées du ChangeDetector, résolution call sites) dans core ; le CLI ne garde que flags + progress bars via `IndexObserver`.
- [ ] Conserver à l'identique la sémantique fiabilité de la PR #21 (ordre commits/detector, marqueur dirty).
- [ ] Test d'intégration core : indexer un répertoire temporaire sans passer par le binaire.

**Acceptance criteria** : `commands/index.rs` < 150 lignes ; sortie CLI inchangée ; nouveau test core couvrant index incrémental + interruption simulée (dirty marker).

**Estimation** : 2-3 j. **Dépend de** : 17.1, 17.2.

---

## Phase 1 — Backend LanceDB (défaut cible, ~1,5 semaine)

### 17.4 `LanceStore` : écriture 🔴

**Goal** : implémentation `VectorStore` sur `lancedb` (épinglé `=0.31`) derrière le feature flag `lance` (activé par défaut dans le binaire de release, mais backend choisi par config).

**Design imposé** :
- Données dans `.codescope/lance/` ; une table `chunks` : colonnes `chunk_id (Int64, PK logique)`, `file_path (Utf8)`, `symbol (Utf8?)`, `kind (Utf8)`, `start_line/end_line (UInt32)`, `content (Utf8)`, `vector (FixedSizeList<Float32, dims>?)`.
- `dims` fixé à la création depuis le modèle d'embedding ; table sans index ANN en dessous de 5 000 vecteurs (brute force Lance), création/refresh de l'index (IVF_PQ par défaut, parametré par `Profile`) dans `maintain()`.
- `delete_by_chunk_ids`/`delete_by_file` = `delete("chunk_id IN (...)")` natif — **plus de tombstones**.
- `upsert` = `merge_insert` sur `chunk_id`.

**Tasks** :
- [ ] Module `vector_store/lance.rs` + conversion Arrow (RecordBatch) depuis `ChunkRecord`.
- [ ] Gestion `vectors = None` (colonne vector nullable) pour le mode « embeddings désactivés ».
- [ ] Paramètres d'index par `Profile` (light/default/heavy → nprobes, num_partitions).
- [ ] Tests : upsert/delete/roundtrip get_chunks, réouverture après crash simulé (kill entre write et flush → table lisible, version précédente).

**Acceptance criteria** : `cargo test -p codescope-search --features lance` vert ; RAM stable pendant `search` sur une table de 100k vecteurs générés (test `#[ignore]` bench-like).

**Estimation** : 3 j. **Dépend de** : 17.1.

### 17.5 `LanceStore` : recherche + hydratation 🔴

**Tasks** :
- [ ] `search()` : ANN top_k (`nearest_to` + `nprobes` du profil), score normalisé en similarité (cohérent avec l'existant : plus grand = meilleur).
- [ ] `get_chunks()` par lot (`chunk_id IN (...)`) — l'hydratation de `SearchEngine` bascule sur le `VectorStore` (supprime le N+1 SQLite actuel, cf. audit).
- [ ] `SearchEngine::open` choisit le backend selon `config.toml` (`vector_store.backend = "usearch" | "lance" | "chroma"`, défaut actuel : `usearch`).
- [ ] Tests d'intégration : lexical/semantic/hybrid identiques sur les deux backends (même fixture, mêmes assertions — table de vérité partagée).

**Acceptance criteria** : la suite `engine.rs` s'exécute paramétrée sur les 2 backends ; résultats hybrid identiques à ±1 rang sur la fixture.

**Estimation** : 2 j. **Dépend de** : 17.4.

### 17.6 Versionnage + migration automatique v1→v2 🔴

**Goal** : C-P5 — l'utilisateur ne fait rien à la main.

**Tasks** :
- [ ] Fichier `.codescope/index_version` (`1` = usearch implicite, `2` = lance, `3` = chroma) ; absence = v1.
- [ ] À l'ouverture (`index`/`search`/`status`) : si version ≠ backend configuré → message clair ; `codescope index` déclenche la reconstruction automatique (équivalent `--all`, on ré-embedde — la migration binaire usearch→lance n'en vaut pas la complexité, décision actée) ; `search` refuse avec un message actionnable (« run `codescope index` »).
- [ ] `codescope clean` purge tous les formats (usearch + lance + chroma dir).
- [ ] Étendre le contrôle de cohérence de `status` (PR #21) au nouveau backend.

**Acceptance criteria** : scénario e2e testé : index v1 existant → passage config en lance → `codescope index` reconstruit sans intervention → `status` OK ; `search` avant migration donne un message actionnable, code de sortie ≠ 0.

**Estimation** : 2 j. **Dépend de** : 17.5, 17.3.

### 17.7 Bench comparatif usearch vs lance ∥ 🔴

**Tasks** :
- [ ] Bench Criterion `vector_store_bench` : latence search (10k/100k/500k vecteurs, 384d), débit d'ingestion, RSS max, rappel@10 vs brute force.
- [ ] Publier les chiffres dans `docs/benchmarks-vector-store.md` ; seuils de non-régression : rappel@10 ≥ 0,95 ; latence p50 ≤ 2× usearch ; RSS search ≤ 0,5× usearch à 500k.

**Acceptance criteria** : rapport commité ; si un seuil échoue → ticket bloquant avant 17.12, ne pas changer le défaut.

**Estimation** : 1-2 j. **Dépend de** : 17.5.

---

## Phase 2 — Backend ChromaDB auto-provisionné (~1,5 semaine)

### 17.8 `ChromaProvisioner` : binaire + cycle de vie 🔴

**Goal** : l'utilisateur choisit `backend = "chroma"` et **n'installe rien** : codescope télécharge, démarre, surveille et arrête un serveur Chroma local.

**Design imposé** :
- Binaire dans `~/.codescope/bin/chroma/<version>/` ; téléchargement via le module `download` existant (retry + SHA-256 épinglés par OS/arch dans un registre statique, comme les modèles ONNX). Matrice : linux-x64, linux-arm64, darwin-x64, darwin-arm64, windows-x64.
- Données du serveur dans `.codescope/chroma/` (par projet — pas de partage inter-projets en v1).
- Démarrage à la demande : port éphémère (bind 127.0.0.1:0 → port choisi écrit dans `.codescope/chroma/endpoint`), health-check HTTP avec timeout (10 s), réutilisation si un serveur du même projet répond déjà (lockfile + PID + heartbeat).
- Arrêt : idle-timeout côté wrapper (option `chroma.keep_alive_secs`, défaut 300 s via un fichier `last_used`), kill propre du process à la fin de `index` si codescope l'a démarré et que `keep_alive` = 0.
- **Fallback documenté** : si `chroma.endpoint` est renseigné en config, ne rien provisionner (serveur de l'utilisateur/équipe).

**Tasks** :
- [ ] `crates/codescope-core/src/chroma_provisioner.rs` : `ensure_running(project, config) -> Result<Endpoint>` + `shutdown_if_owned()`.
- [ ] Registre binaire versionné (URLs + sha256) ; refus explicite avec message clair si OS/arch non supporté.
- [ ] Tests **sans réseau ni vrai binaire** : fake `chroma` (script sh/bat qui ouvre un port et répond au health-check) injecté par variable d'env `CODESCOPE_CHROMA_BIN` ; tests : démarrage, réutilisation, endpoint file, arrêt, binaire corrompu (checksum), port occupé.

**Acceptance criteria** : cycle de vie complet couvert par tests offline sur les 3 OS de la CI ; aucun test n'accède au réseau.

**Estimation** : 3 j. **Indépendant de la Phase 1** (parallélisable).

### 17.9 `ChromaStore` : implémentation `VectorStore` 🔴

**Tasks** :
- [ ] Module `vector_store/chroma.rs` sur la crate `chromadb` (épinglée `=2.3`) : collection `codescope_chunks`, embeddings fournis par codescope (pas de fonction d'embedding côté serveur), metadata = champs de `ChunkRecord`, document = `content`.
- [ ] Mapping des opérations : upsert → `upsert`, delete_by_file → `delete(where={"file_path": ...})`, search → `query` (n_results=top_k), get_chunks → `get(ids)`.
- [ ] Timeouts et erreurs réseau locales converties en `Error::Index` avec message actionnable (« le serveur chroma local ne répond pas ; relancez la commande »).
- [ ] Tests contre un mock HTTP (wiremock ou serveur de test minimal) rejouant les réponses de l'API Chroma v2 ; test d'intégration complet `#[ignore]` (activable en local avec un vrai serveur).

**Acceptance criteria** : suite paramétrée backend (17.5) verte sur le mock ; la suite partagée usearch/lance/chroma est la même table de vérité.

**Estimation** : 3 j. **Dépend de** : 17.1, 17.8.

### 17.10 Intégration CLI + config Chroma 🔴

**Tasks** :
- [ ] `codescope init --vector-store <usearch|lance|chroma>` + section `config.toml` (`[vector_store]` : backend, `chroma.endpoint?`, `chroma.keep_alive_secs?`).
- [ ] `index`/`search`/`status` : provisioning transparent (spinner « starting local vector server... »), erreurs actionnables.
- [ ] `status` affiche backend, version binaire, endpoint, état du serveur.
- [ ] `index_version = 3` + reconstruction guidée (réutilise 17.6).

**Acceptance criteria** : e2e offline avec fake binaire : init --vector-store chroma → index → search → status sur les 3 OS CI.

**Estimation** : 2 j. **Dépend de** : 17.8, 17.9, 17.6.

---

## Phase 3 — Bascule, distribution, nettoyage (~0,5 semaine)

### 17.11 CI et distribution ∥ 🔴

**Tasks** :
- [ ] CI : job matrix features (`usearch` seul, `+lance`, `+chroma-mock`) sur les 3 OS ; temps de build surveillé (Lance est lourd — activer le cache sccache/actions-rust-cache si > 20 min).
- [ ] Release (`release.yml`) : binaires publiés avec `lance` + `chroma` activés ; vérifier la taille du binaire (< 80 Mo cible, sinon investiguer features Arrow/DataFusion superflues).
- [ ] `cargo deny`/audit des nouvelles licences (Lance/Arrow : Apache-2.0 — OK a priori, à confirmer).

**Estimation** : 1 j.

### 17.12 Bascule du défaut vers LanceDB 🔴

**Pré-conditions** : 17.6 vert, seuils du bench 17.7 atteints, 17.11 en place.

**Tasks** :
- [ ] `Config::default()` → `vector_store.backend = "lance"` pour les **nouveaux** `init` ; les projets existants gardent leur backend jusqu'à modification de config (pas de migration forcée).
- [ ] `usearch` rétrogradé en feature legacy (compilé par défaut pour lire les index v1 et permettre la reconstruction ; suppression planifiée à v0.4).
- [ ] Mise à jour : README, `docs/cli.md`, CHANGELOG (breaking notes : `index --all` conseillé), `docs/audit-architecture.md` (C3/C4 → résolus), `docs/amelioration.md` (A5 remplacé par epic 17).

**Acceptance criteria** : projet neuf = lance sans aucune action utilisateur ; projet v1 non touché tant que l'utilisateur ne change rien ; docs cohérentes.

**Estimation** : 1 j.

---

## Récapitulatif des dépendances

```text
17.1 ─┬─ 17.3 ─── 17.6 ─┬─ 17.10 ─┐
17.2 ─┘                 │         ├─ 17.12
17.1 ── 17.4 ── 17.5 ─┬─┘         │
                      └ 17.7 ─────┤
17.8 ── 17.9 ── 17.10             │
17.11 (∥ dès la phase 1) ─────────┘
```

## Risques & mitigations

| Risque | Prob. | Mitigation |
|---|---|---|
| API lancedb 0.x instable | Moyenne | Version épinglée `=0.31` ; adaptateur confiné à `vector_store/lance.rs` |
| Build Lance trop lourd (CI, taille binaire) | Moyenne | Mesuré en 17.11 ; si rédhibitoire, le trait permet de rester usearch par défaut sans jeter le travail |
| Client Rust `chromadb` incomplet vs API serveur | Moyenne | Tests mock figés sur l'API v2 ; fallback `chroma.endpoint` manuel ; Qdrant en plan B derrière le même trait |
| Distribution binaire Chroma (URLs mouvantes) | Moyenne | Registre versionné + checksums épinglés + miroir possible sur les releases GitHub du projet |
| Rappel ANN dégradé (IVF_PQ vs HNSW) | Faible | Seuils bloquants du bench 17.7 ; option index HNSW Lance si besoin |
| Régression fiabilité (acquis PR #21) | Faible | 17.3 reprend tel quel dirty marker + ordre des commits ; tests d'interruption repris dans core |
