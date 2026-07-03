# Plan d'amélioration — codescope

> Suite de l'[audit d'architecture](./audit-architecture.md) (2026-06-10, v0.2.0).
> Les références `C1…C18` renvoient au tableau récapitulatif de l'audit.
> Priorisation : **P0** = fiabilité/correction · **P1** = scalabilité · **P2** = qualité/maintenabilité · **P3** = confort.
>
> **Mise à jour 2026-07-02 — vague 1 implémentée** : A1 (marqueur `indexing.dirty` + réparation par ré-index complet, ChangeDetector mis à jour après les commits BM25/HNSW, hydratation tolérante, contrôle de cohérence dans `status`), A2 (transaction SQLite par fichier, `transaction()` réentrante), A3 (retry exponentiel 3×, timeout configurable via `CODESCOPE_DOWNLOAD_TIMEOUT_SECS`, checksums épinglés en trust-on-first-use), A12, A14. Bonus découvert en route : **C18** — les suppressions BM25 étaient des no-ops silencieux (`chunk_id` non indexé) ; corrigé. Les index Tantivy existants ont l'ancien schéma : un `codescope index --all` est requis après mise à jour.
> Écart assumé sur A1.3 : pas de commits intermédiaires tous les N fichiers — la stratégie de réparation retenue (ré-index complet sur marqueur dirty) les rendrait inutiles ; à revisiter avec A10.

---

## P0 — Fiabilité (à traiter en premier)

### A1. Cohérence inter-index et reprise après crash (C1)
**Problème** : un crash pendant `codescope index` laisse SQLite, Tantivy et HNSW désynchronisés, et le ChangeDetector considère les fichiers comme indexés ⇒ l'incrémental ne répare jamais.
**Actions** :
1. N'appeler `detector.update_file_state()` qu'après la persistance effective des trois index — déplacer la mise à jour du ChangeDetector **après** `bm25.end_write()` / `hnsw.save()` (aujourd'hui à `index.rs:288`, avant les commits de `index.rs:306-307`), en accumulant les chemins traités.
2. Poser un marqueur `indexing.dirty` dans `.codescope/` au début du run, supprimé en fin de run ; au démarrage, si présent ⇒ proposer/forcer la réindexation des fichiers du run interrompu.
3. Commits intermédiaires : `bm25.commit()` + `hnsw.save()` + flush du ChangeDetector tous les N fichiers (N ≈ 500) pour borner la perte en cas de crash.
4. À la recherche, dégrader proprement : un chunk_id introuvable (`engine.rs:169-171`) doit être loggé et ignoré, pas faire échouer toute la requête ; ajouter un contrôle de cohérence dans `codescope status` (compte SQLite vs BM25 vs HNSW).

**Effort** : 2-3 jours. **Gain** : élimine la corruption silencieuse, le défaut le plus grave du projet.

### A2. Transactions SQLite par fichier (C2)
**Problème** : chaque chunk/import/call_site est un INSERT autocommit ⇒ des centaines de milliers de micro-transactions sur un gros dépôt.
**Action** : envelopper le traitement de chaque fichier (`index.rs:208-292`) dans `storage.transaction(...)` (l'API existe déjà, `storage.rs:932`) ; idéalement une transaction par lot de N fichiers. Conserver `prepare_cached` (déjà en place).
**Effort** : ½ journée. **Gain** : ×5 à ×50 sur le débit d'écriture SQLite (gain classique batch vs autocommit).

### A3. Activer la vérification des modèles + retry réseau (C7)
**Actions** :
1. Renseigner `model_sha256` (et idéalement tokenizer/config) pour les deux modèles du registry (`registry.rs:74, 91`) — le code de vérification existe déjà dans `download.rs:58-69`.
2. Ajouter un retry avec backoff exponentiel (3 tentatives : 2s/4s/8s) dans `download_file`, et rendre le timeout (300 s codé en dur, `download.rs:83`) configurable.
3. Bonus : reprise partielle via header `Range` si le serveur le supporte.

**Effort** : 1 jour. **Gain** : robustesse à l'installation, intégrité supply-chain des modèles.

---

## P1 — Scalabilité

### A4. Batching d'embeddings global + découplage de l'inférence (C5)
**Problème** : les batches d'embeddings ne franchissent pas la frontière du fichier (`index.rs:275-286`) et l'inférence ONNX bloque la boucle consommatrice (le parsing s'arrête pendant ce temps).
**Actions** :
1. Accumuler les chunks `(chunk_id, texte)` dans un tampon global et n'appeler `pipeline.embed_texts` que lorsque `embed_batch_size` est atteint (flush final en fin de stream). Les batches passent de 1-5 à 16-32 éléments ⇒ amortit le padding fixe.
2. Étape suivante : déplacer l'embedding dans un thread dédié alimenté par un channel borné (même pattern que `FileReader`/`FileParser`), la boucle principale ne faisant plus que SQLite + Tantivy. Le pipeline devient : parse ∥ embed ∥ write.

**Effort** : 1 (étape 1) + 2 jours (étape 2). **Gain** : ×2 à ×5 sur le temps d'indexation des dépôts à petits fichiers.

### A5. Maîtriser l'empreinte mémoire HNSW (C3)
**Actions, par ordre de coût/bénéfice** :
1. Activer la **quantisation usearch** (`ScalarKind::F16` voire `I8`) par profil : f16 divise par 2 la mémoire vecteurs avec une perte de rappel négligeable pour du code search.
2. Utiliser `Index::view()` (mmap, déjà supporté à la lecture, `hnsw.rs:192-194`) par défaut pour `codescope search` — l'index n'a pas besoin d'être résident pour une requête ponctuelle.
3. Corriger les estimations de `profile.rs:214-223` pour intégrer le coût réel (dims × 4 o × N + graphe ≈ M × 8 o × N) et documenter la limite pratique par profil.
4. Long terme : sharder l'index par sous-arborescence ou passer le stockage vectoriel derrière un trait (préparation epic 14 pgvector).

**Effort** : 1-2 jours (points 1-3). **Gain** : ~×2-4 de capacité à mémoire constante.

### A6. Compaction automatique des tombstones (C4)
**Action** : déclencher `hnsw.compact()` automatiquement en fin d'indexation lorsque `tombstones.len() > ratio × index.len()` (seuil ~10-20 %, configurable). Logger l'opération ; exposer aussi `codescope index --compact` pour forcer.
**Effort** : ½ journée. **Gain** : latence et rappel stables sur dépôts à fort churn.

### A7. Résolution incrémentale des call sites (C6)
**Problème** : `index.rs:312-325` re-résout tous les fichiers à chaque run.
**Action** : ne résoudre que (a) les fichiers ajoutés/modifiés et (b) les fichiers dont des call sites pointaient vers des symboles supprimés/ajoutés dans ce run (requête sur `callee_name` ∈ symboles touchés). Conserver `--all` pour la résolution complète.
**Effort** : 1 jour. **Gain** : indexation incrémentale en O(changement) au lieu de O(dépôt).

### A8. Éliminer la double lecture des fichiers (C10)
**Action** : faire transporter le hash XXH3 (ou le contenu brut) par `FileContent`/`ParsedFile` depuis `FileReader`, et supprimer le `std::fs::read` de `index.rs:204`.
**Effort** : ½ journée. **Gain** : −50 % d'I/O disque à l'indexation ; supprime aussi la fenêtre de course (fichier modifié entre lecture et hash).

### A9. Préchargement par lots dans le call graph (C14)
**Action** : dans le BFS de `call_graph.rs:106-144`, remplacer la requête par nœud par une requête par **frontière** (`WHERE chunk_id IN (...)` sur tout le niveau courant).
**Effort** : ½ journée. **Gain** : latence de `trace` divisée par le fan-out moyen.

---

## P2 — Architecture & qualité

### A10. Extraire l'orchestration d'indexation vers core (C8)
**Action** : créer `codescope_core::IndexPipeline` encapsulant la logique de `commands/index.rs` (deletions, boucle de stream, batching, commits), avec un trait/callback de progression (`on_file_indexed`, `on_stage`) consommé par le CLI pour les progress bars. Le CLI ne garde que parsing des flags + affichage.
**Bénéfices** : testable en unitaire, réutilisable par le daemon (epic 10) ; corrige aussi le contournement de la façade core (CLI → search en direct).
**Effort** : 2 jours. À faire idéalement **avant** A1/A2/A4 pour ne pas refactorer deux fois — sinon juste après.

### A11. Trait de stockage (préparation epic 14)
**Action** : définir un trait `MetadataStore` (sous-ensemble de l'API `Storage` réellement consommée) implémenté par SQLite ; en profiter pour découper `storage.rs` (~2 500 lignes) en modules (schéma, chunks, call_sites, résolution par langage).
**Effort** : 2-3 jours. **Gain** : mockabilité des tests, voie ouverte à Postgres/pgvector.

### A12. Durcir les points de panique et erreurs avalées (C12)
**Actions** :
1. Centraliser les champs Tantivy dans une struct `Bm25Fields` construite une fois avec propagation d'erreur (`bm25.rs:37-41`).
2. Remplacer `serde_json::to_string().unwrap_or_default()` (`result.rs:79,93,103`) par une propagation d'erreur ou au minimum un `tracing::error!`.
3. Supprimer les `unwrap()` des templates indicatif (`index.rs:33,45,114,316`) au profit d'`expect("template statique valide")` documenté.

**Effort** : ½ journée.

### A13. Renforcer les tests
**Priorités** :
1. **Tests de pannes partielles** : simuler un échec entre SQLite et BM25/HNSW et vérifier la détection/réparation (accompagne A1).
2. **Cas limites parser** : fichier vide, code malformé, UTF-8 invalide, fichier d'1M lignes (fallback chunking).
3. **Test de bout en bout avec un modèle ONNX minuscule** en fixture (ou feature-gated) pour couvrir onnx.rs/mean pooling, aujourd'hui non testés.
4. **Bench d'indexation** (Criterion) sur dépôt synthétique de 10k fichiers pour objectiver les gains A2/A4 et prévenir les régressions.

**Effort** : 2-3 jours, parallélisable.

### A14. Nettoyage des dépendances (C13)
**Action** : retirer `tokio` de codescope-cli et `rayon` de codescope-parser/-embed (aucun usage) ; ajouter `cargo-udeps` ou `cargo machete` au CI.
**Effort** : 1 heure. **Gain** : compilation plus rapide, surface d'audit réduite.

### A15. Configurer Tantivy pour le volume (C16)
**Action** : utiliser le heap du profil au lieu du 200 Mo codé en dur (`index.rs:122`) ; expliciter une merge policy (`LogMergePolicy` avec paramètres adaptés) pour les dépôts à millions de chunks.
**Effort** : ½ journée.

---

## P3 — Confort & finitions

| # | Action | Référence |
|---|---|---|
| A16 | Overrides par variables d'environnement (`CODESCOPE_PROFILE`, `CODESCOPE_JOBS`, `CODESCOPE_MODELS_DIR`) + validation de config (rejet/warning des clés inconnues via `serde(deny_unknown_fields)`) | C11 |
| A17 | Coordonner `preprocess_max_chars` (8 Ko, `pipeline.rs:33`) avec `max_seq_len` : dériver la limite de caractères du budget tokens (~4 car/token) et la rendre configurable | C11 |
| A18 | Fusion : récupérer 2-3× top_k par source avant fusion RRF pour améliorer le rappel ; protéger la normalisation min/max de `WeightedFusion` quand une liste a < 2 éléments (`fusion.rs:99-101`) | C15 |
| A19 | Appliquer réellement `MemoryBudget` : faire dépendre la taille des channels et des batches du budget, et émettre un warning quand le tracker dépasse le budget du profil | C9 |
| A20 | Ouvrir le `ModelRegistry` : chargement de définitions de modèles depuis un TOML utilisateur (`~/.codescope/models.toml`) en plus des 2 modèles intégrés | C17 |
| A21 | Spans `tracing` par étape du pipeline (walk/read/parse/store/embed/resolve) avec durées, exposées dans `codescope status --verbose` | — |
| A22 | Documenter un guide de tuning (quand passer en profil heavy, impact de M/ef_construction/ef_search, limites par taille de dépôt) | — |

---

## Feuille de route suggérée

| Vague | Contenu | Effort cumulé | Résultat |
|---|---|---|---|
| **1 — Fiabilité** | A1, A2, A3, A12, A14 | ~1 semaine | Plus de corruption silencieuse, installation robuste |
| **2 — Débit d'indexation** | A8, A4, A7, A15 (+ A13.4 pour mesurer) | ~1 semaine | Indexation ×3-10 sur gros dépôts |
| **3 — Mémoire & échelle** | A5, A6, A19 | ~1 semaine | 1M+ chunks tenables en profil default |
| **4 — Architecture** | A10, A11, A13 | ~1,5 semaine | Base saine pour les epics 10 (daemon) et 14 (pgvector) |
| **5 — Finitions** | A9, A16-A18, A20-A22 | au fil de l'eau | Confort utilisateur et qualité de recherche |

> **Note de cadrage** : si l'epic 10 (daemon) est imminent, faire A10 en tout début de vague 1 pour éviter de refactorer deux fois `index.rs`.
