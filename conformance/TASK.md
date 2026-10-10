# Implementing a set of laws

You are given a set of law files, one law per file, written in the law-file layout of ALICE-LOL that is described below. The file name is the law name. This document is the program contract and the guide to reading those files. Together with the law files it is the whole specification.

## What to build

One command-line program that implements every law you are given. The programming language is given with the task; nothing in this contract depends on it.

### Program contract

- Each run handles one request. The program reads one JSON object from standard input:

  ```json
  {"law": "<law name>", "inputs": { ... }}
  ```

- It writes one JSON object to standard output and exits with status 0. The object is one of these two:

  ```json
  {"outputs": { ... }}
  {"rejected": "<short reason>"}
  ```

- Write `rejected` when the law does not apply to the inputs, for example an input outside its valid range. Do not clamp. Do not extrapolate.
- Numbers are JSON numbers in the units the law states. JSON has no NaN or infinity literal, but a number literal too large for a double (for example `1e400`) parses to infinity. A number that is not finite after parsing is not a number: an input of a quantitative law that is not finite is rejected, and an audit measurement that is not finite is not measured. A list input is a JSON array. A list output is a JSON array with one entry per input element, in the same order. The input list may repeat values and may be unsorted.
- Do not print anything else on standard output. Use standard error for diagnostics.
- A request for an unknown law, or a request in which an input of a quantitative law is missing, is an error of the request, not a rejection: exit with status 2 and write nothing on standard output.
- The request is JSON text as RFC 8259 defines it, encoded in UTF-8. Bytes that are not valid UTF-8, and text that is not JSON, are an error of the request (exit with status 2, write nothing on standard output). In detail:
  - A number is read as a double. A literal too large for a double, whatever its spelling (`1e400`, an integer of 400 digits), reads as infinity, and then the rules for numbers that are not finite apply.
  - `NaN`, `Infinity` and `-Infinity` are not JSON: a request that contains them is an error of the request.
  - When an object has the same key more than once, the last one is read.
  - Arrays and objects nest at most 512 levels deep, counting the request object as level 1. A deeper request is an error of the request.
  - A `\u` escape in the range `D800`–`DFFF` must be part of a surrogate pair (a high one followed by a low one). Any other use is an error of the request.
- A request whose `inputs` is `null`, or that has no `inputs` key, is the same as `"inputs": {}`. An `inputs` value that is not a JSON object (an array, text, a number, `true` or `false`) is an error of the request: exit with status 2 and write nothing on standard output. The same holds for a request that is not a JSON object.

### Rules

- Where a law describes a simulation (a state, a start value and a rate of change), you must simulate it. Write your own step function that advances the state by a time step. Choose the method and the step size, unless the law names the method and the step: then use exactly that method and step. Report what your simulation produces. Do not evaluate the closed-form result directly. The closed form is the reference that your simulated result is judged against, and the tolerance is the allowed deviation.
- Everything else (closed-form evaluations, audits) may be computed directly.
- Use only the standard library of the language. No numerical or physics packages.
- Your program is checked later against the laws on inputs you do not see, including inputs at the edges of the valid ranges and inputs outside them. Test your program against the law statements yourself before you finish.
- When you finish, list any point of a law you found ambiguous and how you resolved it.

### Names of inputs and outputs

- The request `inputs` keys are the names on the `input` and `x-input` lines.
- The response `outputs` keys are:
  - the names that have a `tolerance` line,
  - the name on an `x-output` line,
  - for `kind audit`, `verdict` and `subject` (see "Audit block" below).

## Reading a law file

Each line starts with a keyword. A `#` starts a comment that runs to the end of the line.

| line | meaning |
|------|---------|
| `law <name>` | the law name |
| `kind research` / `kind audit` | a quantitative law, or an audit of measurements |
| `claim <text>` | what the law states, in words |
| `input <name> <unit> [range <lo> <hi>]` | an input. With `range`, the law applies only for `lo <= value <= hi`. Any other value is rejected. |
| `param <name> <value> <uncertainty> <unit>` | a named constant |
| `let <name> <unit> = <expr>` | a named intermediate quantity. Later lines may use the name. |
| `output <name> <unit> = <expr>` | the value of the law |
| `tolerance <name> <unit> = <expr>` | the largest accepted absolute deviation of `<name>` from its value. The expression may use the inputs, the `let` names and `<name>` itself (which means the law value). |
| `verdict <text>` | when an implementation conforms, and what is rejected |
| `begin audit` ... `end audit` | an audit block (see below) |

### Expressions

- Numbers, names, `+ - * / ^`, unary minus, parentheses, and the functions `exp`, `ln`, `sqrt`, `sin`, `cos` (one argument each), `atan2(y, x)`, `min(a, b, ...)` and `max(a, b, ...)`.
- `atan2(y, x)` is the angle of the point `(x, y)`, in (-pi, pi]. `y` and `x` have the same unit; the result is a pure number. The sign of a zero argument is ignored (`-0` is read as `0`), so:

  | y | x | atan2(y, x) |
  |---|---|---|
  | `0` or `-0` | `x < 0` | `+pi` (never `-pi`) |
  | `0` or `-0` | `x > 0` | `0` |
  | `0` or `-0` | `0` or `-0` | `0` |
  | `y > 0` | `0` or `-0` | `+pi/2` |
  | `y < 0` | `0` or `-0` | `-pi/2` |
- `min` and `max` take two or more arguments, all with the same unit, and give that unit. Of equal arguments the first one is the result (this decides the sign of a zero).
- A call with another number of arguments is not part of the language.
- `^` binds tighter than unary minus (`-2^2 = -4`) and is right associative. Its exponent is a constant.
- Angles are in radians. Unit `1` means a pure number.
- Number literals carry no unit.

### Units

Units are products and quotients of the symbols `m km cm mm kg g s ms min h A K mol cd Hz N kN Pa kPa MPa bar J kJ W L mL`, with integer powers, for example `m/s^2` or `J/(mol*K)`. All the laws use the units as written. No conversion is needed.

### Extension lines (`x-`)

These lines state things the core layout above cannot express.

| line | meaning |
|------|---------|
| `x-list <input>` | the input is a list of values. The law holds for every element. The output with a tolerance is a list aligned with it. A value that is not an array, or an array with an element that is not a number, is rejected. |
| `x-integer <input>` | the input must be an integer. Otherwise it is rejected. A number with an integer value satisfies it whatever its JSON spelling (`100` and `100.0` are both integers). |
| `x-range <name> <lo> <hi>` | the law applies only for `lo <= name <= hi`, where `name` is a `let` quantity. For a list input this applies to every element. |
| `x-range <name> > <value>` | the law applies only for `name > value` |
| `x-state <name> <unit>` | a simulated state variable |
| `x-initial <name> = <expr>` | the state at time 0 |
| `x-ode <name> <unit> = <expr>` | the rate of change `d<name>/dt` of a state |
| `x-method free` | the integration method and the internal step size are your choice |
| `x-method kdk` | velocity Verlet (kick-drift-kick); one step of size h: `v += (h/2) a(x)`, `x += h v`, `v += (h/2) a(x)`. Use exactly this method; any other method does not conform |
| `x-method dkd` | position Verlet (drift-kick-drift); one step of size h: `x += (h/2) v`, `v += h a(x)`, `x += (h/2) v`. Use exactly this method; any other method does not conform |
| `x-step fixed <name>` | the integration uses exactly the step `<name>`. One reported entry is produced per step. |
| `x-reduce <name> = <text>` | the reported quantity is a reduction over the simulated trajectory |
| `x-output <name> = <text>` | the reported output and its shape |
| `x-invariant <text>` | a condition on the reported trajectory that the checker verifies (see below) |
| `x-piece <name> <unit> <input> <lo> <hi> = <expr>` | a piecewise output. Use the piece whose `[lo, hi]` contains the input. Where two pieces meet, both give the same value. |
| `x-periodic <name> <period>` | the deviation of `<name>` is measured modulo the period, for example angles modulo 2 pi |
| `x-input <name> <type>` | an input that is not a single number. Its type is written in the form below ("Types of `x-input`"). |
| `x-metric <name> = <derivation>` | a measurement used by the audit block, derived from an `x-input` (see "Derived metrics" below) |
| `x-at-least <metric> <n>` | a metric below `n`, or not measured, counts as 0 |

### Types of `x-input`

```
type  := number | integer | text | list of <type> | record(<field>, <field>, ...)
field := <name>: <type>  |  <name>: optional <type>
```

- `number` is a JSON number that is finite after parsing. `integer` is such a number with an integer value (`3` and `3.0`). `text` is a JSON string. `list of <type>` is a JSON array whose every element has the type. `record(...)` is a JSON object that has every field that is not `optional`, and each field that is present has its type. Other keys of the object are ignored.
- `optional` means the field may be absent. A field that is present must have its type: `null` is not absent.
- **A value that does not match its type does not match as a whole.** One element of a list, or one field of one record, that does not match makes the whole input not match. Such an input is not measured in an audit (as if the key were missing), and it is rejected in a quantitative law.

### Rules that apply to every law

- **Valid ranges on derived quantities.** An `x-range` quantity is computed from the inputs by evaluating its `let` expression as written, operation by operation in the order of the expression, in ordinary double precision. Both bounds are inclusive (`lo <= q <= hi`) and there is no slack: a value just outside a bound is rejected. The `>` form is strict.
- **Inputs are never clamped.** An input outside its range is rejected, even when it is outside by one rounding step.
- **Rounding residue in an intermediate.** A quantity computed from valid inputs may come out slightly outside its mathematical domain by rounding, for example a radicand of `-1e-17` that is 0 exactly. Such a residue in an intermediate may be set to the domain boundary (here 0). This applies to intermediates only, never to an input.
- **`x-reduce ... max over t >= 0`.** The trajectory is infinite, so the program chooses how long to simulate. Choose a horizon long enough that no later maximum can exceed the one you report, for example simulate until the trajectory has turned and is decaying below the reported value.
- **`x-invariant` lines** are checked by the checker on the trajectory you report. Your program never evaluates them and never rejects a request because of them. Report the trajectory the law asks for; the checker decides whether it satisfies the invariants.

### Audit block

An audit block holds one clause per line:

| clause | meaning |
|--------|---------|
| `audit <name>` | the audit name |
| `evidence <metric>` | the metric must have been measured. It is evidence only when it is a finite number greater than 0. Missing, 0, a negative number, or a value that is not a number means no evidence. |
| `expect <metric> == <value> [within <tol>]` | the metric must equal the value, within the tolerance (default 0) |
| `range <key> <value>...` | the set of known values for `<key>`. The measured list must be the same set. Order and duplicates are ignored, on both sides. |

The measurements are the request inputs:

- A **numeric metric** is a JSON number. A value that is not a number (text, `true` / `false`, `null`, an array, an object) counts as not measured, as if the key were missing. A number that is not finite after parsing (an overflowing literal such as `1e400`) is not a number, so it is not measured either.
- A **range key** is an array of text. A value that is not an array of text, including `null`, counts as not measured, so the verdict for that key is `out_of_range`. An empty array is a measured, empty set.
- Values are compared as text. Two values are the same only when their text is identical.
- `x-metric` lines define derived metrics (below). A name defined by an `x-metric` line is never read from the request, even when the request has a key of that name.

### Derived metrics

```
derivation := count(<input>) | distinct(<input>[].<field>) | distinct(set(<input>[].<field>))
```

- `count(<input>)`: the number of elements of the list input.
- `distinct(<input>[].<field>)`: the number of different values of the field over the elements. Values are compared with their type and exactly: the text `"1"` and the text `"1.0"` differ, and the text `"1"` and the number `1` differ.
- `distinct(set(<input>[].<field>))`: the same, where each element's field (a list) is read as a set: order and duplicates are ignored, so `["b", "a", "a"]` and `["a", "b"]` are the same set. An empty list is a set too.
- When the field is `optional` and one element does not have it, the metric is not measured.
- When the input is not measured (absent, or not matching its type), every metric derived from it is not measured.
- Over an empty list, `count` and `distinct` are 0.

The verdict is found by checking in this fixed order, whatever the order of the lines:

1. **Evidence clauses**, in file order. The first failing clause gives `no_evidence`, and the subject is the metric.
2. **Range clauses**, in file order:
   - If the key is not measured, the verdict is `out_of_range`.
   - If the key is measured with a different set of values, the verdict is `parameter_update`.
   - The subject is the key.
3. **Expect clauses**, in file order:
   - If the metric is not measured, the verdict is `undecided`.
   - If the metric is outside the tolerance, the verdict is `breaks`.
   - The subject is the metric.
4. Otherwise the verdict is `supports`, and the subject is `null`.

Output: `{"outputs": {"verdict": "<one of the six words>", "subject": "<name>" or null}}`. Audits never reject, and every measurement is optional, so a missing measurement is not a request error. A request to an audit law without the `inputs` key, or with `"inputs": null`, is the same as `"inputs": {}`: nothing is measured.
