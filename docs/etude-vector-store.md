# Étude — Remplacement du stockage : vector store moderne (ChromaDB, LanceDB…)

> **Date** : 2026-07-03 · Fait suite à l'[audit d'architecture](./audit-architecture.md) (constats C3, C4) et au [plan d'amélioration](./amelioration.md) (A5, A11).
> **Demande** : migrer de SQLite vers une base plus propre type ChromaDB, avec déploiement automatique et **zéro installation côté utilisateur**.
> **Livrable associé** : [`plan/epic-17-vector-store.md`](./plan/epic-17-vector-store.md) — plan d'implémentation détaillé, exécutable par des agents de coding.

---

## 1. Cadrage : ce que SQLite fait vraiment dans codescope

Avant de choisir une cible, il faut être précis sur ce qu'on remplace. Le stockage actuel est **tripartite** :

| Composant | Techno actuelle | Contenu | Problème réel ? |
|---|---|---|---|
| Métadonnées + call graph | **SQLite** (`meta.sqlite`) | files, chunks (contenu inclus), imports, call_sites, tombstones, états de fichiers | **Non** — usage relationnel (jointures, résolution de call sites) où SQLite excelle ; les défauts relevés par l'audit (autocommit par INSERT, C2) sont **déjà corrigés** (PR #21) |
| Index lexical BM25 | **Tantivy** | index plein-texte | Non — état de l'art Rust |
| **Vecteurs (embeddings)** | **usearch/HNSW** (`hnsw.index`) | vecteurs 384d, tombstones en RAM | **Oui** — c'est le vrai point de douleur (C3/C4) : index intégralement chargé en mémoire, pas de filtrage par métadonnées, tombstones sans compaction automatique, pas de transactionnalité |

**Conclusion de cadrage** : « migrer SQLite vers ChromaDB » se traduit techniquement par **remplacer le stockage vectoriel (usearch) et le lier aux métadonnées**, ce qui est exactement le rôle d'un vector store moderne (collection = vecteurs + metadata + filtrage + persistance gérée). SQLite n'est pas la partie « pourrie » : c'est usearch qui plafonne. L'étude ci-dessous couvre donc le remplacement du **couple vecteurs + métadonnées de chunks**, avec le devenir de SQLite traité en §6.

## 2. Contraintes produit (non négociables)

Issues du README/CLAUDE.md et de la demande :

- **C-P1** — Offline : tout fonctionne sans réseau après le premier `index`.
- **C-P2** — Multi-OS : Linux, macOS, Windows (la CI teste les trois).
- **C-P3** — Zéro installation utilisateur : pas de `pip install`, pas de Docker requis, pas de service à lancer à la main.
- **C-P4** — CLI mono-binaire : consommateur principal = agents IA qui lancent `codescope search` ; démarrage rapide.
- **C-P5** — Migration douce : les index `.codescope/` existants doivent être migrés ou reconstruits automatiquement, sans action manuelle autre qu'un éventuel `index --all` guidé.

## 3. Options étudiées

Versions vérifiées sur crates.io le 2026-07-03.

### Option A — ChromaDB (demandée)

- **Nature** : vector database client-serveur. Cœur réécrit en Rust, mais le mode d'usage officiel reste **un serveur** (processus `chroma` ou conteneur) ; la crate Rust `chromadb` **2.3.0** est un **client HTTP uniquement** — il n'existe pas de mode embarqué in-process pour Rust (le mode "embedded" n'existe qu'en Python/JS).
- **Apports** : API collections propre (vecteurs + documents + metadata + filtres `where`), suppression/upsert natifs (fini les tombstones maison), multi-clients (plusieurs agents/process en parallèle — utile pour l'epic 10 daemon), écosystème connu.
- **Coûts pour respecter C-P3** : il faut que **codescope provisionne et pilote lui-même un serveur local** :
  1. téléchargement du binaire `chroma` versionné + checksum au premier run (comme on le fait déjà pour les modèles ONNX) ;
  2. gestion du cycle de vie : spawn à la demande, port dynamique, health-check, arrêt/idle-timeout, verrou multi-process, logs ;
  3. matrice OS/arch des binaires à maintenir (Linux x64/arm64, macOS x64/arm64, Windows) ;
  4. latence : démarrage du serveur (~1-3 s) à amortir sur chaque commande CLI froide + overhead HTTP par requête.
- **Risques** : dépendance à la distribution binaire d'un tiers ; client Rust jeune (2.3.0, 8 deps) et non officiel ; débogage d'un process externe ; empreinte disque (~100-200 Mo).

### Option B — LanceDB (embarqué, Rust natif)

- **Nature** : vector database **embarquée** (bibliothèque Rust, crate `lancedb` **0.31.0**), format colonnaire Lance/Arrow sur disque, index ANN (IVF-PQ, HNSW) **sur disque avec mmap** — pas de chargement intégral en RAM.
- **Apports** : répond directement à C3 (mémoire), C4 (deletes natifs, compaction intégrée) et C1 partiel (versioning des tables, écritures atomiques) ; vecteurs + metadata + filtres SQL (DataFusion) dans le même moteur ; **aucun serveur, aucun binaire externe, zéro installation** — la « base propre » sans casser le modèle mono-binaire ; scalaire quantization intégrée.
- **Coûts** : dépendance de build lourde (~83 deps directes, Arrow/DataFusion — temps de compilation et taille du binaire +20-40 Mo) ; API Arrow plus verbeuse que rusqlite ; mono-écrivain local (suffisant aujourd'hui, le multi-process partagé restant le rôle des options serveur).
- **Risques** : évolution rapide de l'API (0.x) — à épingler ; pas de partage réseau natif (hors scope actuel).

### Option C — Statu quo amélioré (usearch + A5/A6)

Quantisation f16/i8, mmap par défaut, compaction automatique. Le moins cher (~2 jours), mais ne apporte ni filtrage par métadonnées, ni transactionnalité, ni la « propreté » demandée. Documenté pour référence — ce sont les actions A5/A6 du plan d'amélioration, qui restent valables comme mitigation court terme.

### Options écartées (résumé)

| Option | Verdict | Raison |
|---|---|---|
| **sqlite-vec** | ❌ | Extension encore alpha (0.1.10-alpha.4) ; brute-force sans vrai index ANN au-delà de ~100k vecteurs |
| **Qdrant** | ✅ backend BYO | Serveur uniquement (client Rust officiel 1.18, mature) ; retenu comme backend « bring your own » derrière le trait ; pas de mode géré en v1 |
| **pgvector** | ✅ backend BYO | Retenu comme backend « bring your own » (DSN Postgres) — sous-ensemble vectoriel de l'epic 14, qui garde le scope complet (métadonnées partagées, FTS) |
| **DuckDB + vss** | ❌ | Extension vectorielle expérimentale, chargement d'extension à distribuer |

## 4. Décision retenue

**Une abstraction `VectorStore` unique, trois niveaux d'usage** (décision validée le 2026-07-03) :

1. **Défaut : LanceDB embarqué** — satisfait *simultanément* C-P1→C-P5 : l'utilisateur ne voit rien (pas de serveur, pas d'install), on gagne deletes natifs, filtrage par métadonnées, index sur disque (mémoire bornée), compaction et écritures atomiques. Le « smooth pour l'utilisateur » est structurel, pas à construire.
2. **Backends serveurs en mode « bring your own »** (priorité 1 pour l'externe) : l'utilisateur qui a déjà une base pointe codescope dessus via `config.toml` (`endpoint`/DSN + auth) — **ChromaDB**, **pgvector** et **Qdrant**, les trois serveurs les plus répandus. Coût minimal (clients Rust uniquement + documentation de raccordement), aucune gestion de process.
3. **Mode « géré » (managed)** : pour l'utilisateur qui veut un backend serveur **sans rien installer**, codescope provisionne lui-même le serveur local (téléchargement du binaire avec checksum, démarrage, health-check, arrêt). Livré d'abord pour Chroma (seul des trois à distribuer un binaire local autonome raisonnable) ; pgvector/Qdrant managés restent documentés comme extension future (Postgres embarqué est trop lourd, on n'impose pas Docker — C-P3).

### Perd-on le mono-binaire ? Non.

Point important pour lever l'ambiguïté :

- Les **clients** (crates `chromadb`, `qdrant-client`, `postgres`+pgvector) sont des **bibliothèques Rust compilées dans l'unique binaire `codescope`**. Ajouter ces backends grossit le binaire de quelques Mo mais ne crée ni deuxième exécutable ni dépendance d'installation.
- Ce qui est externe, c'est le **serveur**, et seulement si l'utilisateur **opte** pour un backend serveur : soit il le possède déjà (mode BYO — rien à gérer pour nous), soit codescope télécharge un *asset* versionné dans `~/.codescope/bin/` (mode géré — même statut que les modèles ONNX déjà téléchargés aujourd'hui : un asset au premier run, pas une installation).
- Le chemin par défaut (LanceDB) reste **100 % in-process** : un seul binaire, zéro process annexe, offline.

Points d'honnêteté d'architecte :

- Faire d'un backend serveur le **défaut** violerait C-P3/C-P4 (un serveur auto-géré reste un serveur : port, process, pannes) et ferait dépendre chaque `codescope search` d'un sidecar — d'où l'embarqué par défaut, les serveurs en opt-in.
- **SQLite reste** pour le relationnel pur (call graph, états de fichiers) dans cette phase : la résolution des call sites est du SQL à jointures que ni Chroma ni Lance ne font mieux. Il passe derrière un trait `MetadataStore` (action A11 de l'audit), ce qui rend son remplacement ultérieur (Postgres/epic 14) mécanique. Le contenu des chunks, lui, migre dans le vector store (source unique pour l'hydratation des résultats).
- La bascule du défaut est sécurisée par un **feature flag + bench comparatif** (rappel@k, latence, RAM) avant de changer le défaut.

## 5. Expérience utilisateur cible

```text
# Utilisateur standard (défaut LanceDB) — rien ne change, rien à installer :
codescope init && codescope index && codescope search "..."

# Index existant (usearch) détecté → migration automatique proposée :
codescope index
> Index au format v1 (usearch) détecté ; reconstruction au format v2 (lance)... [auto]

# Utilisateur qui a déjà une base (mode « bring your own », documenté) :
#   config.toml :
#     [vector_store]
#     backend = "chroma"                    # ou "qdrant", "pgvector"
#     endpoint = "http://10.0.0.5:8000"     # ou DSN postgres pour pgvector
codescope index      # se connecte, crée la collection/table, rien d'autre à faire

# Utilisateur qui veut Chroma sans rien installer (mode géré) :
codescope init --vector-store chroma   # sans endpoint → provisioning automatique
codescope index
> Téléchargement de chroma-server 1.x (sha256 vérifié)... démarrage local :127.0.0.1:<port>
```

Règle simple : `endpoint` renseigné → mode BYO (on s'y connecte, point) ; absent → mode géré (codescope fait tout : téléchargement, lancement, arrêt).

## 6. Devenir de SQLite et trajectoire long terme

| Horizon | Vecteurs | Métadonnées/chunks | Call graph & états |
|---|---|---|---|
| Aujourd'hui | usearch (RAM) | SQLite | SQLite |
| **Epic 17 (ce plan)** | **LanceDB** (défaut) / Chroma (opt-in) | **LanceDB** (table chunks) / Chroma (documents+metadata) | SQLite derrière trait `MetadataStore` |
| Plus tard (epic 14) | + backend pgvector partagé | + Postgres | Postgres |

## 7. Chiffrage global

| Phase | Contenu | Effort estimé |
|---|---|---|
| 0 | Abstractions (`VectorStore`, `MetadataStore`), extraction `IndexPipeline` vers core (A10) | ~1 semaine |
| 1 | Backend LanceDB + migration auto des index + bench | ~1,5 semaine |
| 2 | Backends serveurs en mode BYO : config générique `endpoint`, ChromaStore, PgvectorStore, QdrantStore, doc « connecter sa base » | ~1,5 semaine |
| 3 | Mode géré : provisioner Chroma (download/lifecycle) + intégration CLI | ~1 semaine |
| 4 | Bascule du défaut, docs, CI matrix, nettoyage usearch | ~0,5 semaine |

Détail ticket par ticket, contrats d'API et critères d'acceptation : voir [`plan/epic-17-vector-store.md`](./plan/epic-17-vector-store.md).
