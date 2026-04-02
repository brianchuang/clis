# Rippy Search: Filtering & Ranking Design

Evaluation of data structures and strategies for accurate search in rippy,
designed around the codebase's composition-first philosophy.

---

## Current State

Two independent search paths exist today:

1. **CLI `search`** — `WHERE content LIKE '%query%'` in SQLite. Substring
   match, no ranking, ordered by recency.
2. **TUI filter** — `tui_core::compute_filtered` uses `SkimMatcherV2` for
   fuzzy matching. Returns indices sorted by score.

Both work when the user remembers exact words. Neither helps when they
remember the *meaning* ("that k8s config", "the API endpoint from earlier").

---

## Design Principles

Following the codebase's existing patterns:

1. **Scorers are pure functions.** Each scoring strategy is a function
   `(&[T], &str, Fn(&T) -> ...) -> Vec<(usize, f64)>` that takes items and
   a query, returns scored indices. No traits, no objects, no state.

2. **Ranking is composition.** A merge function combines scored index vectors
   from independent scorers. Weights are plain parameters, not config objects.

3. **Embeddings are a separate concern.** The embedding module produces and
   stores vectors. It knows nothing about ranking. Ranking knows nothing about
   how vectors were produced. They connect through `&[f32]` slices.

4. **Feature-gated, not abstracted.** Semantic search is behind
   `features = ["semantic"]`. When disabled, the code doesn't exist — no
   no-op implementations, no trait objects, no dynamic dispatch.

5. **No new crates.** Scoring functions go in `tui-core` (they're reusable).
   Embedding infrastructure goes in rippy (it's rippy-specific). No
   `search-core` crate for one consumer.

---

## Layer 1: Scoring Functions (in `tui-core`)

Each scorer independently produces `Vec<(usize, f64)>` — index + normalized
score. Callers compose them.

### Fuzzy scorer (evolve existing `compute_filtered`)

```rust
/// Score items by fuzzy match. Returns (index, score) pairs, score in 0.0..1.0.
pub fn score_fuzzy<T, F>(items: &[T], query: &str, text_fn: F) -> Vec<(usize, f64)>
where
    F: Fn(&T) -> String,
```

This replaces `compute_filtered`. The current function discards scores after
sorting — we need them for blending. `compute_filtered` becomes a thin wrapper
that calls `score_fuzzy` and drops the scores, preserving backward compat.

### Recency scorer

```rust
/// Score items by age. Returns (index, score) pairs, score in 0.0..1.0.
/// `half_life_hours` controls decay: score is 1.0 at now, 0.5 at half_life.
pub fn score_recency<T, F>(items: &[T], time_fn: F, half_life_hours: f64) -> Vec<(usize, f64)>
where
    F: Fn(&T) -> chrono::DateTime<chrono::Local>,
```

No query needed — recency is query-independent. Exponential decay:
`e^(-age_hours / half_life)`. Pure function, no dependencies beyond `chrono`.

### Merge function

```rust
/// Merge scored index vectors with weights. Returns indices sorted by
/// weighted sum, highest first.
pub fn merge_scores(
    scored_lists: &[(&[(usize, f64)], f64)],  // (scores, weight) pairs
    count: usize,                               // total item count
) -> Vec<usize>
```

Takes any number of `(scores, weight)` pairs. For each index, sums
`score * weight` across all lists where that index appears (missing = 0.0).
Returns sorted indices.

This is the composition point. Call sites look like:

```rust
let fuzzy = score_fuzzy(&entries, &query, |e| e.content.clone());
let recency = score_recency(&entries, |e| e.timestamp, 24.0);
let ranked = merge_scores(&[(&fuzzy, 0.7), (&recency, 0.3)], entries.len());
```

Adding semantic search later is one more line — no existing code changes:

```rust
let semantic = score_semantic(&entries, &query_embedding, |e| &embeddings[e.id]);
let ranked = merge_scores(
    &[(&fuzzy, 0.5), (&semantic, 0.4), (&recency, 0.1)],
    entries.len(),
);
```

---

## Layer 2: Vector Storage (in `rippy`, feature-gated)

Embeddings are stored and queried independently from ranking.

### SQLite: persistence

Add an `embedding BLOB` column to the `clips` table. Each blob is a
384-dimensional `f32` vector (1,536 bytes). SQLite handles this fine up to
100k+ entries.

```sql
ALTER TABLE clips ADD COLUMN embedding BLOB;
```

Two new methods on `Store`:

```rust
fn set_embedding(&self, id: i64, embedding: &[f32]) -> Result<()>;
fn get_embeddings(&self) -> Result<Vec<(i64, Vec<f32>)>>;
```

No vector operations in SQL. SQLite is just a persistence layer.

### In-memory: hot path

```rust
pub struct EmbeddingIndex {
    ids: Vec<i64>,
    /// Flat row-major f32 matrix: ids.len() × dim.
    /// Pre-normalized to unit vectors (cosine similarity = dot product).
    data: Vec<f32>,
    dim: usize,
}
```

**Why flat `Vec<f32>`?** Cache-friendly sequential access for brute-force dot
product. At 10k entries × 384 dims = 15 MB — fits in L3 cache. SIMD
auto-vectorizes the inner loop.

**Why not ANN (HNSW, etc.)?** Brute-force over 10k × 384 is ~2ms on Apple
Silicon. ANN adds complexity (rebuild on insert, parameter tuning) for zero
practical benefit at this scale. Even at 100k entries (~20ms), it's within
the interactive budget.

Operations:

```rust
impl EmbeddingIndex {
    fn from_rows(rows: Vec<(i64, Vec<f32>)>, dim: usize) -> Self;
    fn top_k(&self, query: &[f32], k: usize) -> Vec<(i64, f32)>;
    fn insert(&mut self, id: i64, embedding: &[f32]);
    fn remove(&mut self, id: i64);
}
```

`top_k` returns `(id, similarity)` pairs. The caller maps IDs back to item
indices for `merge_scores`. This keeps the index decoupled from the item
collection.

### Scorer bridge (in rippy, not tui-core)

```rust
/// Score items by semantic similarity to a query embedding.
/// `embedding_fn` looks up the pre-computed embedding for each item.
fn score_semantic<T, F>(
    items: &[T],
    query_embedding: &[f32],
    embedding_fn: F,
) -> Vec<(usize, f64)>
where
    F: Fn(&T) -> Option<&[f32]>,
```

This lives in rippy because it bridges rippy's `EmbeddingIndex` with
tui-core's scoring protocol. It's a pure function — no state, no side
effects.

---

## Layer 3: Inference (in `rippy`, feature-gated)

### Runtime: `ort` v2 (ONNX Runtime)

**Why `ort`?** Mature, fast (~1-3ms per embedding on Apple Silicon CPU),
well-supported on macOS ARM64. ONNX format lets us swap models without code
changes.

**Why not `candle`?** Smaller binary but requires manually wiring tokenizer +
model + mean-pooling. More code to own. The ~20MB size difference isn't worth
it for a clipboard manager.

**Why not `fastembed`?** Too much abstraction. It bundles model download
logic, tokenizer config, and inference into one opaque type. We want to
own each piece separately so they compose with rippy's existing patterns.

### Model: all-MiniLM-L6-v2

384 dimensions, ~23 MB ONNX file, ~58 MTEB average. Best quality/size ratio
for short text. Downloaded to `~/.local/share/rippy/models/` on first use.

### Embedder function

```rust
pub struct Embedder { /* ort session + tokenizer */ }

impl Embedder {
    fn load(model_dir: &Path) -> Result<Self>;
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}
```

This is the one struct in the design — it wraps the ONNX session which is
inherently stateful (loaded model weights). But it has no knowledge of
storage, ranking, or UI. It takes strings, returns vectors.

---

## What We Don't Need

| Approach | Why not |
|---|---|
| Vector DB (Qdrant, etc.) | <100k vectors. SQLite + flat scan is faster to start and simpler. |
| ANN index (HNSW) | Brute-force is <10ms at our scale. No tuning parameters to get wrong. |
| Full-text engine (Tantivy) | Designed for millions of docs. Fuzzy-matcher already handles our case. |
| GPU inference | CPU is 1-3ms. GPU adds cold-start latency for model compilation. |
| Re-ranking models | Single-pass embedding similarity is sufficient at our corpus size. |
| Trait-based scorer abstraction | We have 2-3 concrete scorers. Closures and `merge_scores` compose them. |

---

## Dependency Impact

New deps for semantic search (all behind `features = ["semantic"]`):

| Crate | Purpose | Size |
|---|---|---|
| `ort` ~2.0 | ONNX Runtime | ~15-30 MB (dynamic lib) |
| `tokenizers` | HuggingFace tokenizer | ~2 MB |

Default `cargo install rippy` stays lean. Semantic search is opt-in:
`cargo install rippy --features semantic`.

---

## Implementation Phases

Each phase is independently shippable and doesn't require the next.

### Phase 1: Scoring primitives (tui-core)

- Add `score_fuzzy` that returns `Vec<(usize, f64)>`
- Add `score_recency` for time-decay scoring
- Add `merge_scores` for weighted combination
- Rewrite `compute_filtered` as a wrapper around `score_fuzzy`
- Rippy's `refilter()` switches to `merge_scores` with fuzzy + recency
- **No new dependencies.** Immediate search quality improvement.

### Phase 2: Embedding storage (rippy)

- Add `embedding BLOB` column with migration in `Store::open`
- Implement `EmbeddingIndex` with flat vector storage
- Add `score_semantic` bridge function
- Tests with synthetic embeddings (no model needed)

### Phase 3: Inference pipeline (rippy, feature-gated)

- Add `ort` + `tokenizers` behind `semantic` feature
- Implement `Embedder` struct
- Model download on first use
- Embed on clipboard write in watcher thread
- `yy embed --backfill` for existing history
- Wire `score_semantic` into TUI and CLI ranking

### Phase 4: UX polish

- Mode toggle in TUI (`Ctrl-S` cycles Fuzzy / Semantic / Hybrid)
- `--fuzzy-only` / `--semantic` CLI flags
- `[search]` section in config.toml for weight tuning
