#!/usr/bin/env python3
"""The editions of each rule set under laws/<domain>/ (conformance/TASK.md "Editions of a rule set" and
"A later edition written as a delta"), checked with scripts/law_rules.py.

Each edition is laws/<domain>/<edition>/<rule>.law (the first) or <rule>.delta (a later one), with its
frozen example corpus laws/<domain>/<edition>/examples.jsonl: one JSON object per line, `name` (the
first 12 hex digits of the SHA-256 of the canonical JSON of `inputs`), `inputs` and `answer`
(`{"outputs": ...}`, `{"rejected": true}` or `{"request_error": true}`). The corpus is written once,
by `--freeze`, when the edition is added, and never again. laws/<domain>/EDITIONS.toml records every
edition (identifier, predecessor, period, semantics, digest of its corpus); `--write` writes it.

`--check` (CI) fails when
  - the ledger is not what the files give (an edition file or a frozen corpus changed, or an edition
    was added or removed without the ledger),
  - the chain is not one line from a `first` edition, or the periods leave a gap or overlap,
  - an edition's frozen corpus is not answered the same by the edition today,
  - the requests whose answer differs between an edition and the one it supersedes (over both
    corpora) are not exactly the names its `changes` lines give,
  - nothing was compared.

    python3 scripts/edition_check.py --check
    python3 scripts/edition_check.py --freeze laws/<domain>/<edition>   write a new edition's corpus
    python3 scripts/edition_check.py --write                            write the ledgers
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "scripts"))
import law_rules  # noqa: E402
import rule_law_ids  # noqa: E402


def canonical(v) -> str:
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def example_name(inputs) -> str:
    return hashlib.sha256(canonical(inputs).encode("utf-8")).hexdigest()[:12]


def answer_json(answer) -> dict:
    kind, val = answer
    if kind == "outputs":
        return {"outputs": val}
    return {"rejected": True} if kind == "rejected" else {"request_error": True}


class Edition:
    def __init__(self, path: Path, text: str, resolved: str, changes: list[str], law: dict, lid: bytes):
        self.path, self.text, self.resolved, self.changes, self.law, self.lid = path, text, resolved, changes, law, lid
        self.dir = path.parent
        ed = law["edition"]
        self.name, self.supersedes, self.start, self.until = ed["name"], ed["supersedes"], ed["from"], ed["until"]

    def corpus_path(self) -> Path:
        return self.dir / "examples.jsonl"

    def corpus(self) -> list[dict]:
        p = self.corpus_path()
        if not p.is_file():
            raise SystemExit(f"{p.relative_to(ROOT)}: no frozen corpus (python3 scripts/edition_check.py --freeze {self.dir.relative_to(ROOT)})")
        return [json.loads(ln) for ln in p.read_text(encoding="utf-8").splitlines() if ln.strip()]

    def answer(self, inputs) -> dict:
        return answer_json(law_rules.respond(self.law, inputs))


def editions(domain: Path, sem) -> list[Edition]:
    """The editions of one domain, in chain order (first to last)"""
    by_id, deltas = {}, []
    for f in sorted(domain.glob("*/*")):
        if f.suffix == ".law":
            text = f.read_text(encoding="utf-8")
            law = law_rules.parse(text, sem)
            if law["edition"] is None:
                raise SystemExit(f"{f.relative_to(ROOT)}: a rule set's law file has the edition header")
            lid = law_rules.law_id(law, sem)
            by_id[lid] = Edition(f, text, law_rules.resolved_text(text, sem), [], law, lid)
        elif f.suffix == ".delta":
            deltas.append((f, f.read_text(encoding="utf-8")))
    while deltas:
        progressed = False
        for item in list(deltas):
            f, text = item
            sup = law_rules._edition(law_rules._lines(text), None, "table")[0]["supersedes"]
            if sup in by_id:
                resolved, changes = law_rules.resolve(by_id[sup].resolved, text, sem)
                law = law_rules.parse(resolved, sem)
                lid = law_rules.law_id(law, sem)
                by_id[lid] = Edition(f, text, resolved, changes, law, lid)
                deltas.remove(item)
                progressed = True
        if not progressed:
            raise SystemExit(f"{deltas[0][0].relative_to(ROOT)}: `supersedes` names no edition of {domain.name}")
    firsts = [e for e in by_id.values() if e.supersedes is None]
    if len(firsts) != 1:
        raise SystemExit(f"{domain.relative_to(ROOT)}: {len(firsts)} first editions (one is required)")
    chain, nxt = [firsts[0]], {e.supersedes: e for e in by_id.values() if e.supersedes is not None}
    if len(nxt) != len(by_id) - 1:
        raise SystemExit(f"{domain.relative_to(ROOT)}: two editions supersede the same edition")
    while chain[-1].lid in nxt:
        chain.append(nxt[chain[-1].lid])
    return chain


def ledger(domain: Path, chain: list[Edition]) -> str:
    out = ["# the editions of this rule set, written by scripts/edition_check.py --write: do not edit\n"]
    for e in chain:
        corpus = hashlib.sha256(e.corpus_path().read_bytes()).hexdigest() if e.corpus_path().is_file() else ""
        out.append(f'\n[[edition]]\nname = "{e.name}"\nfile = "{e.path.relative_to(domain).as_posix()}"\n'
                   f'law_id = "{e.lid.hex()}"\nsupersedes = "{e.supersedes.hex() if e.supersedes else "first"}"\n'
                   f'from = {e.start}\nuntil = {e.until if e.until is not None else -1}\n'
                   f'semantics = "{e.law["edition"]["semantics"].hex()}"\ncorpus_sha256 = "{corpus}"\n')
    return "".join(out)


def problems(domain: Path, chain: list[Edition]) -> tuple[list[str], int]:
    errs, compared = [], 0
    rel = domain.relative_to(ROOT)
    for a, b in zip(chain, chain[1:]):
        if a.until is None or a.until != b.start:
            errs.append(f"{rel}: {a.name} ends at {a.until}, {b.name} starts at {b.start} (a gap or an overlap)")
    for e in chain:
        for ex in e.corpus():
            compared += 1
            if ex["name"] != example_name(ex["inputs"]):
                errs.append(f"{e.corpus_path().relative_to(ROOT)}: {ex['name']} is not the name of its inputs")
            if e.answer(ex["inputs"]) != ex["answer"]:
                errs.append(f"{e.corpus_path().relative_to(ROOT)}: {ex['name']}: the edition answers "
                            f"{canonical(e.answer(ex['inputs']))}, frozen {canonical(ex['answer'])}")
    for a, b in zip(chain, chain[1:]):
        union = {ex["name"]: ex["inputs"] for e in (a, b) for ex in e.corpus()}
        differ = sorted(n for n, inp in union.items() if a.answer(inp) != b.answer(inp))
        if sorted(set(b.changes)) != differ or len(b.changes) != len(set(b.changes)):
            errs.append(f"{b.path.relative_to(ROOT)}: changes {sorted(b.changes)}, but the answers that differ "
                        f"from {a.name} are {differ}")
    return errs, compared


def freeze(edir: Path, sem) -> int:
    domain = edir.parent
    chain = editions(domain, sem)
    e = next((x for x in chain if x.dir == edir), None)
    if e is None:
        raise SystemExit(f"{edir}: not an edition of {domain.name}")
    if e.corpus_path().exists():
        raise SystemExit(f"{e.corpus_path().relative_to(ROOT)}: a frozen corpus is never written again")
    rows = []
    for v in law_rules.vectors(e.law, e.path.stem):
        inp = v.get("request_inputs", v["inputs"])
        if inp is None:
            continue
        rows.append({"name": example_name(inp), "inputs": inp, "answer": e.answer(inp)})
    if not rows:
        raise SystemExit(f"{edir}: no example")
    e.corpus_path().write_text("".join(canonical(r) + "\n" for r in rows), encoding="utf-8")
    print(f"edition_check: froze {len(rows)} examples of {domain.name}/{e.name}")
    return 0


def main(argv: list[str]) -> int:
    sem = rule_law_ids.semantics((ROOT / "alice-lol/src/rule_law.rs").read_text(encoding="utf-8"))
    if len(argv) == 2 and argv[0] == "--freeze":
        return freeze((Path.cwd() / argv[1]).resolve(), sem)
    domains = sorted(d for d in (ROOT / "laws").iterdir() if d.is_dir() and any(d.glob("*/*.delta")) or
                     (d.is_dir() and any(d.glob("*/*.law"))))
    if not domains:
        raise SystemExit("no rule set under laws/<domain>/<edition>/ (compared nothing)")
    if argv == ["--write"]:
        for d in domains:
            (d / "EDITIONS.toml").write_text(ledger(d, editions(d, sem)), encoding="utf-8")
        print(f"edition_check: wrote {len(domains)} ledger(s)")
        return 0
    if argv != ["--check"]:
        print(__doc__, file=sys.stderr)
        return 2
    errs, compared, n_ed = [], 0, 0
    for d in domains:
        chain = editions(d, sem)
        n_ed += len(chain)
        current = (d / "EDITIONS.toml").read_text(encoding="utf-8") if (d / "EDITIONS.toml").is_file() else ""
        if current != ledger(d, chain):
            errs.append(f"{d.relative_to(ROOT)}/EDITIONS.toml is not what the editions give (an edition or its frozen corpus changed?)")
        e, c = problems(d, chain)
        errs += e
        compared += c
    for e in errs:
        print(f"error: {e}", file=sys.stderr)
    if compared == 0:
        print("error: no example compared", file=sys.stderr)
        return 1
    print(f"edition_check: {len(domains)} rule set(s), {n_ed} edition(s), {compared} example(s) compared, {len(errs)} problem(s)")
    return 1 if errs else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
