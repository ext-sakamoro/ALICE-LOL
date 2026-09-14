# alice-lol-datagen

Synthetic **(caption, LOL)** pair generator for [ALICE-LOL](../README.md) — Track C1 of the "make models speak LOL" plan.

LOL is executable, so training data can be manufactured without human labelling. Each **template family** is a parametric generator that emits, from the *same* parameters: the LOL text (the learning target), an English and a Japanese caption, and oracle points (inside/outside) that the pipeline verifies with `alice_sdf::eval` before the sample is kept.

| family | targets (from the `llm_bench` baseline failure types) |
|---|---|
| `primitive_placed` | translate dropped / half-extent confusion (captions give full sizes, LOL uses half) |
| `stacked` | translate + composition dropped (snowman / stepped column) |
| `attachment` | composition dropped (mug handle / table legs / arch subtract) |
| `plate_holes` | subtract / `polar_repeat` / `repeat_finite` |
| `transformed` | rotate / scale |
| `intent_program` | Phase 3 Intent structure (`seq`/`par`, `entities`, ids) |
| `random_tree` | vocabulary coverage from the grammar buckets (structural captions) |
| `product_shortcut` | `pen_cup(50, 100)` etc. — `lol` keeps the shortcut, `lol_canonical` holds the expansion |

```bash
cargo run --release -p alice-lol-datagen --bin datagen -- --n 20000 --seed 42 --families all --out data.jsonl
# ≈ 5,000 samples/s single-thread; rejected samples (self-check failures) are reported on stderr
```

Each JSONL line: `{id, family, seed, caption_en, caption_ja, lol, lol_canonical, oracle:[[x,y,z,"in"|"out"],…], intent_verbs:[…]}`.

Self-check per sample: `parse_program` succeeds, `to_lol` is idempotent, every oracle point evaluates to the expected side, and (with `--features grammar-check`) the LLM grammar accepts both `lol` and `lol_canonical`. Deterministic for a given seed (xorshift64\*, no external RNG dependency).
