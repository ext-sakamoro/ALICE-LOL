# Conformance scoring for laws/spike

This directory holds the tools that check an independent implementation of the laws in `laws/spike/` against a corpus generated from those law files. The corpus is not stored. It is generated for one scoring run and deleted afterwards.

## Contents

| file | role |
|------|------|
| `TASK.md` | the contract given to an implementer, with the guide to reading law files |
| `gen_corpus.py` | builds the corpus from the law files (needs mpmath) |
| `run_conformance.py` | runs one implementation against a corpus |
| `ref_impl.py` | reference implementation; `REF_BUG=1..10` selects a defective variant |
| `score.sh` | generate, check corner coverage, score, delete |
| `controls.sh` | must-red controls for the generator and the runner |
| `kepler_sweep.c`, `kepler_sweep.py` | measurement of the Kepler valid ranges |
| `kepler_divergence.py` | 1 ulp separation of two Kepler trajectories |
| `kepler_mp_check.py` | 50-digit re-integration of Kepler spot points |
| `kepler_range.md` | how the Kepler ranges were measured, with the output of the scripts above |
| `requirements.txt` | Python packages for the corpus generator (mpmath) |

`scripts/law_corners.py` and `scripts/law_corpus_cover.py` are also used here. They need only the standard library, and CI runs their tests.

## Setup

```sh
python3 -m venv .venv-conformance
.venv-conformance/bin/python -m pip install -r conformance/requirements.txt
export PYTHON=$PWD/.venv-conformance/bin/python    # used by score.sh and controls.sh
```

`run_conformance.py`, `ref_impl.py`, `kepler_divergence.py` and `kepler_sweep.py` need only the standard library. `kepler_sweep.py` also needs `cc`. `gen_corpus.py` and `kepler_mp_check.py` need mpmath.

## Program contract

The contract given to an implementer is `TASK.md`: the program contract and the guide to reading law files. In short, an implementation is one command:

- It reads `{"law": <name>, "inputs": {...}}` on standard input.
- It writes `{"outputs": {...}}` or `{"rejected": "<reason>"}` on standard output and exits with status 0.
- For an unknown law or a missing input of a quantitative law, it exits with status 2 and writes nothing on standard output.

The law files and `TASK.md` are its whole specification.

## Order of work

1. **Give the implementer only the law files and `TASK.md`, and state the kind wanted** (`static`: a program for these law files; `reader`: a program that reads law files when it runs; see `TASK.md`). Do not give them the rest of `conformance/`, the source tests cited on `source` lines, or any generated corpus.
2. **Do not generate a corpus while any implementation is still being written.** No corpus file exists during that time.
3. **When every implementation is finished, score each one:**

   ```sh
   conformance/score.sh --kind static|reader <command that runs the implementation>
   ```

   `score.sh` does five things:
   - generates the corpus into a temporary directory from `laws/spike/*.law`;
   - runs `scripts/law_corpus_cover.py`, which stops the run when a corner of a law is missing or nothing was compared;
   - scores with `run_conformance.py`;
   - checks the probes with `check_probes.py --kind` (the kind the implementation was asked for: `static` skips the probes with a law file of their own and prints how many it skipped);
   - deletes the temporary directory.
4. **After any change to the generator, the runner or the law files, run `conformance/controls.sh`.** It must report that every control behaves as expected (see `kepler_range.md` for its output).

## What the corpus is made of

From the law files:

- **Valid range.** An accepted vector satisfies every `input ... range`, `x-integer` and `x-range` line, and a rejected vector breaks one. The generator checks this in double and at 40 digits, and stops when the two disagree; such a vector lies within rounding of an edge.
- **Corners.** They come from `law_corners.corners`:
  - every bound, and both parities of integer inputs;
  - both sides outside each bound, and non-integer values;
  - derived-range edges and inner piece edges;
  - the vertices of the ranged inputs.
- **Expected values.**
  - Closed-form laws: the `output` / `x-piece` / `x-expr` and `tolerance` expressions, evaluated with mpmath at 40 digits.
  - Kepler laws: the start, the acceleration and the method (`x-initial`, `x-ode`, `x-method`), integrated with mpmath at 30 digits, to give the states after step 1 and after the last step.
  - Audit laws: the clauses of the audit block.

Held in `gen_corpus.py` itself, because a law file does not state them:

- the interior points;
- a base point per law for the inputs a corner leaves free;
- the request that realises each derived-range edge;
- the derivation of the free-text `x-metric` lines of `identifier_feature_independent`.

## Limits

- **Separation is by procedure only.** A process running as the same user can read every file in the repository, including `conformance/`. Keeping the corpus away from an implementation is not enforced by permissions. It relies on two things: the corpus does not exist until scoring, and the implementer is given only the law files and the contract.
- **The generator reveals the test points.** `gen_corpus.py`, `run_conformance.py` and `ref_impl.py` contain the designed points and the reference behaviour. Anyone who reads them can reconstruct the corpus. Withholding them is part of the procedure in step 1, not a mechanism.
- **Reading the source tests is not detected.** The law files cite the tests they were taken from on `source` lines. Nothing checks whether an implementer read those tests.
- **Witness search is not a proof.** It tries 9 values per other input. A bound marked unreachable means that no point was found, not that none exists.
- **Vertices on an edge are left out.** A vertex whose derived quantity lies within 1e-9 of an x-range edge is not enumerated, because the verdict there depends on rounding.
