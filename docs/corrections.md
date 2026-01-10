# Corrections / Review honnête du projet `codescope`

## Résumé honnête

- L’architecture “1 crate = 1 responsabilité” est saine et déjà très proche d’un produit utilisable (index + BM25 + HNSW + fusion + pipeline concurrent + config).
- Mais l’état “engineering” n’est pas encore au niveau “production-ready” annoncé : le repo ne passe pas ses propres garde-fous CI (`cargo fmt --check` et `cargo clippy -D warnings` échouent), et quelques choix actuels créent des risques de cohérence/perf à grande échelle.

## Ce qui est vraiment bien

- **Découpage clair** : `codescope-cli` (UX), `codescope-core` (projet/config/pipeline), `codescope-parser` (tree-sitter/chunking), `codescope-embed` (ONNX/tokenizer), `codescope-search` (BM25+HNSW+fusion).
- **Pipeline déjà “agent-friendly”** : JSONL par défaut + `--pretty`, sortie stable, bons types (`SearchResult`, `SearchResults`).
- **Incrémental + tombstones** : change detection + tombstones HNSW (et tombstones DB) = bonne base pour éviter les rebuilds complets.
- **Tests unitaires présents** et passent : `cargo test --workspace` OK dans cet environnement.

## Ce qui bloque “projet propre/maintenable” aujourd’hui

- **CI rouge en l’état** :
  - `cargo fmt --all -- --check` échoue (diffs de format).
  - `cargo clippy --workspace --all-targets -- -D warnings` échoue (plusieurs lints).
  - Donc la promesse “CI enforces fmt+clippy” est vraie… mais le code n’est pas aligné avec ça.
- **Incohérences doc/outillage** : `Cargo.toml` fixe `rust-version = "1.78"` mais le README annonce “Rust 1.75+”.
- **Paramètres de config non exploités par la CLI** : `Config.search.rrf_k` / `bm25_weight` existent, mais la CLI hardcode `FusionStrategy::Rrf { k: 60.0 }` et ne propose pas l’option Weighted.

## Risques techniques (les vrais “pain points” à venir)

- **Cohérence d’index** : l’indexing met à jour SQLite + Tantivy + HNSW dans un même run sans vraie stratégie d’atomicité (ex : crash au milieu ⇒ état partiellement mis à jour). À l’échelle, ça finit en “index corrompu / à nettoyer”.
- **Perf I/O inutile** : pendant l’indexing, relire le fichier au disque pour calculer un hash alors que le reader l’a déjà lu pour parser. Sur gros repos, ça coûte.
- **Qualité clippy** : lints “low value mais bruyants” non traités (format args, micro-simplifs, `too_many_arguments` sur `insert_chunk`). Tant que c’est là, tu ne peux pas compter sur clippy comme garde-fou.
- **Ergos/robustesse** :
  - `unwrap_or_default()` sur lecture fichier en index masque des erreurs I/O au lieu de les rendre visibles.
  - Les `expect(...)` “par invariant” peuvent devenir des panics difficiles à diagnostiquer si l’invariant casse.
- **Finition “distribution”** : `ModelRegistry` expose des URLs mais pas de download/checksum effectif (donc semantic/hybrid reste “manuel + fragile”).

## Recommandations priorisées (ordre qui maximise le ROI)

- **P0 – Remettre CI au vert** : exécuter `cargo fmt --all` + corriger les lints clippy.
- **P1 – Rendre l’indexing “recoverable”** : transactions DB par fichier/batch + stratégie “write-then-swap” pour Tantivy/HNSW (ou au minimum marqueurs de version + rollback).
- **P2 – Éliminer les doubles lectures** : faire remonter hash/bytes depuis le reader (ou hasher le `content` lu) au lieu de relire le fichier sur disque.
- **P3 – Exploiter la config côté CLI** : `rrf_k`, `bm25_weight`, pool size, threads, top_k par défaut ; éviter les constantes codées en dur.
- **P4 – “Product polish”** : verrou/lock d’indexing, messages d’erreurs plus actionnables, et une story “download modèle” avec checksum.

