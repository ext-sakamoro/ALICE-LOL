# Kepler laws: how the valid range was measured

This file covers `laws/spike/kepler_energy_bounded_kdk.law` and `laws/spike/kepler_energy_bounded_dkd.law`. Every number below comes from a script in this directory. Re-run the script to reproduce it.

## Criteria

These are the criteria that the law files state.

- **Growth.** Let the largest relative energy error over periods 2 .. P be divided by the largest error in period 1. This ratio G must be at most 1.2.
- **Order.** Take the mean relative energy error over period 1 of a run with k steps per period, and divide it by the same mean for a run with 2k steps per period. This ratio O must lie in [3, 5]. Here k = n when 2n <= 2000, and k = floor(n / 2) otherwise.
- **Trajectory.** The states after step 1 and after the last step must equal those of the stated method within 1e-7.

The former criteria are shown for comparison:

- growth on the last period only, and
- the max-based order ratio, max error in period 1 with n steps over the same with 2n steps, in [3, 5].

## Cells

A cell is (method, e, n). It is integrated over 40 periods with each of four evaluation orders of 1/r^3:

1. `1/(r2*sqrt(r2))`
2. `1/pow(r2, 1.5)`
3. `1/(r*r*r)` with `r = sqrt(r2)`
4. `1/sqrt(r2*r2*r2)`

A cell passes when it passes under all four orders.

| grid | e | n | cells per method |
|------|---|---|------------------|
| coarse | 0.200, 0.205, .., 0.800 (121 values) | 100 .. 300, 301, 338, .., 1997, and 2000 (248 values) | 30 008 |
| fine | 0.700, 0.701, .., 0.800 (101 values) | 100 .. 200 (101 values) | 10 201 |

The upper bound of e is the first failing e minus 0.02, rounded down to a multiple of 0.005.

Inside the ranges the coarse grid has 28 520 cells for kdk (e <= 0.77) and 28 272 for dkd (e <= 0.765), 56 792 in total. Each is evaluated under the four orders.

## Sweep output

`python3 conformance/kepler_sweep.py --check --rk4` needs `cc` and takes about 1 minute on 10 cores. Its output:

```
kdk coarse: 30008 cells x 4 orders; first failure (e, n): new criteria (0.795, 101), old criteria (0.765, 101)
kdk fine: 10201 cells x 4 orders; first failure (e, n): new criteria (0.794, 101), old criteria (0.764, 101)
kdk: first failure e = 0.794 (n = 101) -> range e <= 0.77, margin 0.024; law file: e <= 0.77 (same)
  coarse, e <= 0.77: 28520 cells, 0 not passing under all 4 orders
    max growth 1.139479 at (e, n) = (0.735, 103); min order 3.620269 at (e, n) = (0.765, 102)
    largest spread over the 4 orders: growth 6.00e-08, order 1.11e-07
    order-dependent verdicts: 0 cells []; inside the range 0
  fine, e <= 0.77: 7171 cells, 0 not passing under all 4 orders
    max growth 1.141104 at (e, n) = (0.733, 101); min order 3.618483 at (e, n) = (0.762, 100)
    largest spread over the 4 orders: growth 7.07e-10, order 3.49e-09
    order-dependent verdicts: 0 cells []; inside the range 0
dkd coarse: 30008 cells x 4 orders; first failure (e, n): new criteria (0.795, 100), old criteria (0.795, 100)
dkd fine: 10201 cells x 4 orders; first failure (e, n): new criteria (0.789, 103), old criteria (0.789, 103)
dkd: first failure e = 0.789 (n = 103) -> range e <= 0.765, margin 0.024; law file: e <= 0.765 (same)
  coarse, e <= 0.765: 28272 cells, 0 not passing under all 4 orders
    max growth 1.049281 at (e, n) = (0.765, 147); min order 3.840499 at (e, n) = (0.765, 100)
    largest spread over the 4 orders: growth 1.33e-07, order 2.05e-07
    order-dependent verdicts: 3 cells [(0.8, 101), (0.8, 103), (0.8, 104)]; inside the range 0
  fine, e <= 0.765: 6666 cells, 0 not passing under all 4 orders
    max growth 1.051141 at (e, n) = (0.764, 145); min order 3.840499 at (e, n) = (0.765, 100)
    largest spread over the 4 orders: growth 9.53e-10, order 3.96e-09
    order-dependent verdicts: 6 cells [(0.793, 101), (0.797, 100), (0.798, 100), (0.8, 101), (0.8, 103), (0.8, 104)]; inside the range 0
rk4 coarse, e <= 0.77: order ratio of means in [13.58, 31.29] over 28520 cells x 4 orders
```

Summary:

- **Rounding is not the cause of the failures.** Inside both ranges, no verdict depends on the evaluation order. The spread of G and O over the four orders is at most 2.1e-7.
- **The old order ratio failed for a sampling reason.** With odd n, period 1 never samples periapsis, while the 2n run, with an even step count, does. So the max-based ratio fell below 3 at e = 0.764 for kdk.
- **The new order ratio is phase-insensitive.** The ratio of means does not depend on where the steps fall on the orbit.
- **RK4 fails everywhere in the range.** Its order ratio of means is at least 13.58 over the whole kdk range, so the order criterion alone rejects it.

## 50-digit spot check

`conformance/kepler_mp_check.py` needs mpmath. It integrates spot points in double and at 50 digits.

```
kdk e = 0.770 n = 100: G 1.0005103484 O_mean 3.6370113525 O_max 3.6450527548 (50 digits); largest difference to double 4.0e-13
kdk e = 0.770 n = 101: G 1.0389134118 O_mean 3.7334198376 O_max 2.9209083675 (50 digits); largest difference to double 4.6e-12
kdk e = 0.735 n = 103: G 1.1394791190 O_mean 3.7328243919 O_max 3.2924837494 (50 digits); largest difference to double 3.4e-12
kdk e = 0.764 n = 101: G 1.0510225834 O_mean 3.7459708008 O_max 2.9930092973 (50 digits); largest difference to double 4.2e-12
kdk e = 0.794 n = 101: G 1.2185969876 O_mean 3.6620057706 O_max 2.5620024906 (50 digits); largest difference to double 1.3e-10
dkd e = 0.765 n = 100: G 1.0092450640 O_mean 3.8404988954 O_max 3.7413263533 (50 digits); largest difference to double 3.8e-13
dkd e = 0.765 n = 101: G 1.0130921100 O_mean 3.9139285557 O_max 3.9188540456 (50 digits); largest difference to double 9.4e-12
dkd e = 0.789 n = 103: G 1.2401889040 O_mean 4.0699862651 O_max 3.7925935681 (50 digits); largest difference to double 4.2e-03
largest difference over 8 points: 4.2e-03
```

- Inside the ranges, the double and 50-digit statistics agree to within 1e-11.
- The two points that differ more are both outside the ranges:
  - kdk e = 0.794, n = 101: 1.3e-10.
  - dkd e = 0.789, n = 103: 4.2e-3.

## Chaos outside the range: 1 ulp separation

`python3 conformance/kepler_divergence.py --report` runs two trajectories of the same cell, the second starting with x moved by 1 ulp. It prints the largest component separation per run.

```
kdk e = 0.77 n =  100: max separation over 80 periods 4.35e-10
kdk e = 0.77 n =  101: max separation over 80 periods 4.02e-12
kdk e = 0.77 n =  102: max separation over 80 periods 1.17e-10
kdk e = 0.77 n =  103: max separation over 80 periods 2.59e-11
kdk e = 0.77 n =  104: max separation over 80 periods 9.11e-10
kdk e = 0.77 n =  105: max separation over 80 periods 7.15e-12
kdk e = 0.77 n =  106: max separation over 80 periods 1.16e-09
kdk e = 0.77 n =  107: max separation over 80 periods 1.25e-11
kdk e = 0.77 n =  131: max separation over 80 periods 2.21e-11
kdk e = 0.77: range 4.0e-12 .. 1.2e-09
dkd e = 0.765 n =  100: max separation over 80 periods 8.08e-12
dkd e = 0.765 n =  101: max separation over 80 periods 2.97e-10
dkd e = 0.765 n =  102: max separation over 80 periods 1.45e-11
dkd e = 0.765 n =  103: max separation over 80 periods 9.00e-10
dkd e = 0.765 n =  104: max separation over 80 periods 8.64e-12
dkd e = 0.765 n =  105: max separation over 80 periods 1.51e-09
dkd e = 0.765 n =  106: max separation over 80 periods 1.36e-11
dkd e = 0.765 n =  107: max separation over 80 periods 1.59e-09
dkd e = 0.765 n =  131: max separation over 80 periods 1.13e-10
dkd e = 0.765: range 8.1e-12 .. 1.6e-09
kdk e = 0.8 n =  100: first period with separation > 0.1: 20
kdk e = 0.8 n =  101: first period with separation > 0.1: 22
kdk e = 0.8 n =  103: first period with separation > 0.1: 21
kdk e = 0.8 n =  105: first period with separation > 0.1: 24
kdk e = 0.8 n =  107: first period with separation > 0.1: 56
kdk e = 0.8: first period > 0.1 in 20 .. 56
dkd e = 0.8 n =  100: first period with separation > 0.1: 25
dkd e = 0.8 n =  101: first period with separation > 0.1: 24
dkd e = 0.8 n =  103: first period with separation > 0.1: 25
dkd e = 0.8 n =  105: first period with separation > 0.1: 33
dkd e = 0.8 n =  107: first period with separation > 0.1: 37
dkd e = 0.8: first period > 0.1 in 24 .. 37
```

- **At the upper bound of e, the orbit stays regular.** Over 80 periods the separation stays at 4.0e-12 .. 1.6e-9 and does not grow to O(1).
- **At e = 0.8, the orbit is chaotic.** The separation passes 0.1 within 20 .. 56 periods for kdk and 24 .. 37 for dkd, and then stays O(1) (up to about 3, the size of the orbit).
- **Consequence for the verdict.** Once the separation is O(1), the growth and order statistics there depend on rounding. This is why the verdict at e about 0.79 .. 0.8 can change with the evaluation order of r^3 alone.

## Must-red controls

`conformance/controls.sh` (see `PROTOCOL.md`):

```
corpus: 317 vectors, 90 in-range Kepler vectors
unchanged                          expect pass        exit 0, pass 317, fail 0, Kepler fail 0/90  ok
ref_bug_6_pow_r3                   expect pass        exit 0, pass 317, fail 0, Kepler fail 0/90  ok
ref_bug_8_sqrt_cubed               expect pass        exit 0, pass 317, fail 0, Kepler fail 0/90  ok
ref_bug_1_drag_factor              expect fail        exit 1, pass 274, fail 43, Kepler fail 0/90  ok
ref_bug_2_isa_range                expect fail        exit 1, pass 315, fail 2, Kepler fail 0/90  ok
ref_bug_3_audit_rule               expect fail        exit 1, pass 313, fail 4, Kepler fail 0/90  ok
ref_bug_9_evidence_not_zero        expect fail        exit 1, pass 312, fail 5, Kepler fail 0/90  ok
ref_bug_10_range_multiset          expect fail        exit 1, pass 315, fail 2, Kepler fail 0/90  ok
ref_bug_4_rk4                      expect kepler-fail exit 1, pass 227, fail 90, Kepler fail 90/90  ok
ref_bug_5_last_step_dropped        expect kepler-fail exit 1, pass 227, fail 90, Kepler fail 90/90  ok
ref_bug_7_state_before_step        expect kepler-fail exit 1, pass 227, fail 90, Kepler fail 90/90  ok
ref_bug_4_rk4_invariants_only      expect kepler-fail exit 1, pass 227, fail 90, Kepler fail 90/90  ok
all controls behave as expected
```

- **What each Kepler check catches:**
  - RK4 (bug 4) fails on the trajectory check, and on the energy criteria alone (last row).
  - A run that drops the last step (bug 5) fails on the length check (n * periods states).
  - A run that reports the state before each step (bug 7) keeps the length and passes the energy criteria. It fails only on the trajectory check, which is why that check is part of the law.
- **What the audit controls catch:** reading evidence as "not 0" (bug 9) fails the 5 vectors with a negative, text, `true` or `null` count. Comparing ranges with their duplicates (bug 10) fails the 2 vectors whose known list repeats a case.
