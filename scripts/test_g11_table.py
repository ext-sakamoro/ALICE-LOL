#!/usr/bin/env python3
"""tests for scripts/g11_table.sh: it fetches ALICE-Law at the pinned revision, refuses the generator or the
checker when its SHA-256 is not the pinned one, runs the generator (--check, or --write) and then the checker
on this repository with that revision read from the fetched checkout, and exits 2 when either step compared
nothing or cannot read what it needs, 1 when either found something, 0 otherwise. The fetched repository is
a local one (ALICE_LAW_URL) whose generator and checker are stand-ins that record their arguments and exit
with the code the test asks for, so these tests need no network.

    python3 scripts/test_g11_table.py
"""
from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SCRIPT = HERE / "g11_table.sh"
BASH = shutil.which("bash")

STUB = """import os, sys
from pathlib import Path
role = Path(sys.argv[0]).name
with open(os.environ["G11_STUB_LOG"], "a", encoding="utf-8") as f:
    f.write(role + " " + " ".join(sys.argv[1:]) + "\\n")
if role == "g11_table_gen.py" and "--write" in sys.argv:
    Path("conformance").mkdir(exist_ok=True)
    Path("conformance/g11.toml").write_text("# generated\\n", encoding="utf-8")
sys.exit(int(os.environ.get("G11_RC_" + role.split(".")[0].upper(), "0")))
"""


def git(cwd: Path, *args: str) -> str:
    return subprocess.run(["git", "-C", str(cwd), *args], check=True, capture_output=True, text=True,
                          encoding="utf-8").stdout.strip()


def sha(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


@unittest.skipIf(BASH is None, "no bash")
class Wrapper(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="g11sh-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.law, self.crate = self.tmp / "law", self.tmp / "crate"
        files = {"scripts/g11_table_check.py": STUB + "# checker\n", "scripts/g11_table_gen.py": STUB + "# generator\n",
                 "conformance/g11-shared-rows.toml": "# catalog\n"}
        for rel, text in files.items():
            p = self.law / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(text.encode("utf-8"))
        git(self.law, "init", "-q")
        git(self.law, "add", "-A")
        git(self.law, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "law")
        self.rev = git(self.law, "rev-parse", "HEAD")
        self.check_sum, self.gen_sum = sha(files["scripts/g11_table_check.py"]), sha(files["scripts/g11_table_gen.py"])
        (self.crate / "scripts").mkdir(parents=True)
        (self.crate / ".github").mkdir()
        shutil.copy(SCRIPT, self.crate / "scripts/g11_table.sh")
        self.pin()
        self.log = self.tmp / "stub.log"

    def pin(self, name="ALICE-Demo", rev=None, check=None, gen=None, drop=None):
        lines = {"name": name, "rev": rev or self.rev, "checker-sha256": check or self.check_sum,
                 "gen-sha256": gen or self.gen_sum}
        text = "".join(f"{k} {v}\n" for k, v in lines.items() if k != drop)
        (self.crate / ".github/g11-table-pin").write_bytes(text.encode("utf-8"))

    def run_sh(self, *args, gen_rc=0, check_rc=0, git_config=None):
        env = dict(os.environ, ALICE_LAW_URL=str(self.law), G11_STUB_LOG=str(self.log),
                   G11_RC_G11_TABLE_GEN=str(gen_rc), G11_RC_G11_TABLE_CHECK=str(check_rc))
        for i, (k, v) in enumerate((git_config or {}).items()):
            env.update({"GIT_CONFIG_COUNT": str(i + 1), f"GIT_CONFIG_KEY_{i}": k, f"GIT_CONFIG_VALUE_{i}": v})
        r = subprocess.run([BASH, "scripts/g11_table.sh", *args], cwd=self.crate, env=env, capture_output=True,
                           text=True, encoding="utf-8")
        calls = self.log.read_text(encoding="utf-8").splitlines() if self.log.exists() else []
        return r.returncode, r.stdout + r.stderr, calls

    def test_both_steps_pass(self):
        rc, out, calls = self.run_sh()
        self.assertEqual(rc, 0, out)
        self.assertEqual([c.split()[0] for c in calls], ["g11_table_gen.py", "g11_table_check.py"])

    def test_the_generator_checks_and_both_read_the_fetched_revision(self):
        _, _, calls = self.run_sh()
        gen, check = calls
        self.assertIn("--repo . --name ALICE-Demo", gen)
        self.assertIn("--check", gen)
        for c in calls:
            self.assertRegex(c, f"--law {self.rev}=\\S+/law(\\s|$)")

    def test_write_writes_the_table_then_checks_it(self):
        rc, out, calls = self.run_sh("--write")
        self.assertEqual(rc, 0, out)
        self.assertIn("--write", calls[0])
        self.assertTrue((self.crate / "conformance/g11.toml").exists())

    def test_a_generator_finding_is_exit_1_and_the_checker_still_runs(self):
        rc, out, calls = self.run_sh(gen_rc=1)
        self.assertEqual(rc, 1, out)
        self.assertEqual(len(calls), 2)

    def test_a_checker_finding_is_exit_1(self):
        self.assertEqual(self.run_sh(check_rc=1)[0], 1)

    def test_a_generator_that_compared_nothing_is_exit_2(self):
        self.assertEqual(self.run_sh(gen_rc=2, check_rc=1)[0], 2)

    def test_a_checker_that_compared_nothing_is_exit_2(self):
        self.assertEqual(self.run_sh(gen_rc=1, check_rc=2)[0], 2)

    def test_a_wrong_checker_sha256_is_exit_2_before_anything_runs(self):
        self.pin(check="0" * 64)
        rc, out, calls = self.run_sh()
        self.assertEqual(rc, 2, out)
        self.assertIn("scripts/g11_table_check.py at", out)
        self.assertEqual(calls, [])

    def test_a_wrong_generator_sha256_is_exit_2_before_anything_runs(self):
        self.pin(gen="0" * 64)
        rc, out, calls = self.run_sh()
        self.assertEqual(rc, 2, out)
        self.assertIn("scripts/g11_table_gen.py at", out)
        self.assertEqual(calls, [])

    def test_a_pin_without_the_generator_sha256_is_exit_2(self):
        self.pin(drop="gen-sha256")
        rc, out, _ = self.run_sh()
        self.assertEqual(rc, 2, out)
        self.assertIn("gen-sha256 <64 hex>", out)

    def test_a_malformed_name_is_exit_2(self):
        self.pin(name="Demo")
        self.assertEqual(self.run_sh()[0], 2)

    def test_a_revision_that_cannot_be_fetched_is_exit_2(self):
        self.pin(rev="f" * 40)
        rc, out, calls = self.run_sh()
        self.assertEqual(rc, 2, out)
        self.assertIn("fetch failed", out)
        self.assertEqual(calls, [])

    def test_a_revision_without_the_catalog_is_exit_2(self):
        git(self.law, "rm", "-q", "conformance/g11-shared-rows.toml")
        git(self.law, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "drop")
        self.pin(rev=git(self.law, "rev-parse", "HEAD"))
        rc, out, _ = self.run_sh()
        self.assertEqual(rc, 2, out)
        self.assertIn("no conformance/g11-shared-rows.toml", out)

    def test_a_git_that_converts_to_crlf_does_not_change_the_compared_bytes(self):
        rc, out, _ = self.run_sh(git_config={"core.autocrlf": "true"})
        self.assertEqual(rc, 0, out)

    def test_an_unknown_argument_is_exit_2(self):
        rc, out, calls = self.run_sh("--check")
        self.assertEqual(rc, 2, out)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
