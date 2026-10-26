# Implementing a set of laws

You are given a set of law files, one law per file, written in the law-file layout of ALICE-LOL that is described below. The file name is the law name. This document is the program contract and the guide to reading those files. Together with the law files it is the whole specification.

## What to build

One command-line program that implements every law you are given. The programming language is given with the task; nothing in this contract depends on it.

An implementation states its kind, `static` or `reader`; the task says which kind is wanted.

- **`static`**: a program for the law files it is given. It answers for those laws only, giving the answers the files mean. It is checked on those laws; the checks that hand a program a law file of its own do not apply.
- **`reader`**: a program that reads law files when it runs. It must read any law file by the reading rules below (the characters, lines, declarations and numbers) and refuse one that breaks them, and it is checked on everything, including law files of its own. The directory of the law files is given in the environment variable `LOL_LAW_DIR` (the checks always set it); the file of the law `<name>` is `<LOL_LAW_DIR>/<name>.law`, and a law with no such file is an unknown law (exit with status 2, as above).

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
- The text of `rejected` is a reason for people. It is not compared: any short text is correct, and only whether the request was rejected is checked.
- Numbers are JSON numbers in the units the law states. JSON has no NaN or infinity literal, but a number literal too large for a double (for example `1e400`) parses to infinity. A number that is not finite after parsing is not a number: an input of a quantitative law that is not finite is rejected, and an audit measurement that is not finite is not measured. A list input is a JSON array. A list output is a JSON array with one entry per input element, in the same order. The input list may repeat values and may be unsorted. An empty list input is a request like any other: the output is an empty list, not a rejection.
- `-0` and `0` are the same number in an output: either may be written (outputs are compared as numbers, within the law's tolerance).
- Do not print anything else on standard output. Use standard error for diagnostics.
- A request for an unknown law, or a request in which an input of a quantitative law is missing, is an error of the request, not a rejection: exit with status 2 and write nothing on standard output. A missing input is decided before any other check of the inputs: a request with a missing input is an error of the request even when another of its inputs would be rejected. An input whose key is present is not missing, whatever its value: a value of the wrong type for a quantitative law (text, `true` / `false`, `null`, an array or an object where a number is expected) is rejected.
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
| `kind machine` / `kind predicate` / `kind table` / `kind statute` / `kind procedure` / `kind contract` / `kind memory` | a state machine, a predicate over typed inputs, a table of rows tried in order, the clauses of a statute, the steps of an administrative procedure, the obligations of a contract, or one memory of a character (see "Rule laws" below) |
| `claim <text>` | what the law states, in words |
| `input <name> <unit> [range <lo> <hi>]` | an input. With `range`, the law applies only for `lo <= value <= hi`. Any other value is rejected. |
| `param <name> <value> <uncertainty> <unit>` | a named constant |
| `let <name> <unit> = <expr>` | a named intermediate quantity. Later lines may use the name. |
| `output <name> <unit> = <expr>` | the value of the law |
| `tolerance <name> <unit> = <expr>` | the largest accepted absolute deviation of `<name>` from its value. The expression may use the inputs, the `let` names and `<name>` itself (which means the law value). |
| `verdict <text>` | when an implementation conforms, and what is rejected |
| `begin audit` ... `end audit` | an audit block (see below) |

How the lines are read:

- **The `law` line is not read.** It names the law for people. The law name is the file name: a law file without a `law` line, with another name on it, or with a second `law` line is read the same.
- **An audit law has the line `kind audit`.** A law file is an audit law when one of its lines, with its comment removed and the spaces at its two ends trimmed, is exactly `kind audit` (one ASCII space between the two words). `kind  audit`, `kind` and `audit` separated by a tab, `kind Audit`, `kind audits` and `kind audit x` are not that line. A law file without the line `kind audit` that has the line `kind machine`, `kind predicate` or `kind table` (exact in the same way) is a rule law, read as "Rule laws" below says; a law file with the line `kind audit` is an audit law whatever its other lines. A law file of your own (a `reader` is given some, see "What to build") that is none of these is an unknown law: exit with status 2.
- **A line whose keyword is not in the table above is not read** outside the audit block. Some law files carry such lines for people (`source <where the law was taken from>`, `oracle <a worked example>`). Inside the audit block every line must be a clause (see "Audit block").
- **Tokens are separated by spaces or tabs.** Outside the `x-` declarations (below), the words of a line may be separated by one or more spaces or tabs, and spaces at the two ends of a line do not count. `begin audit` and `end audit` are exact: two words separated by one ASCII space. `begin  audit`, `begin` and `audit` separated by a tab, and the same forms of `end audit` open or close no audit block, and the law file then does not read: a request to it is an error of the request.

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
| `x-range <name> <lo> <hi>` | the law applies only for `lo <= name <= hi`, where `name` is a `let` quantity. For a list input this applies to every element: one element outside rejects the whole request. For an empty list it holds (there is no element), and the output is an empty list. The ranges of the other inputs apply whatever the list: an empty list does not make a request with another input outside its range valid. |
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
| `x-piece <name> <unit> <input> <lo> <hi> = <expr>` | a piecewise output. Use the piece whose `[lo, hi]` contains the input. Where two pieces meet, both give the same value. Every name that has a `tolerance` line is an output of every request, so a law with several piecewise names (`isa1976_lower_atmosphere`: `temperature`, `pressure`, `density`) answers all of them, each from its own piece. |
| `x-periodic <name> <period>` | the deviation of `<name>` is measured modulo the period, for example angles modulo 2 pi |
| `x-input <name> <type>` | an input that is not a single number. Its type is written in the form below ("Types of `x-input`"). |
| `x-metric <name> = <derivation>` | a measurement used by the audit block, derived from an `x-input` (see "Derived metrics" below) |
| `x-at-least <metric> <n>` | a metric below `n`, or not measured, counts as 0. This is applied before the audit: `evidence` and `expect` both see the value after it, and a metric that is not measured becomes a measured 0 |

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
- **Expressions are evaluated as written.** Every expression of a quantitative law (each `let`, each output, each `x-range` quantity) is evaluated in double precision, in the order of the file, operation by operation in the order the expression is written, with no fused or contracted operations (no fused multiply-add).
- **Every value of an evaluation is finite.** Every intermediate value (each `let`, each operation and function call of an expression) and every output must be finite. When one is not (an overflow, `ln` or `sqrt` of a negative number, a division by zero, `inf - inf`), the request is rejected (`{"rejected": "non-finite value"}`), even when IEEE arithmetic would carry the value on to a finite result: `1/exp(1000)` is a rejection, not 0. An output is never written as a non-finite number (JSON has none). The law `finite_evaluation_probe` (`y = 1/exp(1000 sin(x))`) is rejected where `1000 sin(x)` exceeds the largest finite argument of `exp` (about 709.78) and accepted elsewhere in its range. Audits are not affected: a measurement that is not finite is not measured (above).
- **Verdicts within an ulp band are not scored.** The value of an elementary function (`exp`, `ln`, `sin`, `cos`, `atan2`, a power other than `^2`) is not fixed by IEEE, and math libraries may differ in its last bits. When a request's finiteness verdict would change if those values moved by up to 64 ulp (the request is that close to a crossing, for example `1000 sin(x)` at the largest finite argument of `exp`), implementations whose libraries round differently may disagree, and such a request is not scored: the corpus and the probes keep at least 64 ulp away from every such crossing. An implementation does not have to compute elementary functions correctly rounded. A crossing reached through `+ - * /` and `sqrt` only (which IEEE rounds correctly everywhere) is not in the band, and is scored.
- **Inputs are never clamped.** An input outside its range is rejected, even when it is outside by one rounding step.
- **No clamping of intermediates.** An intermediate value is never moved onto its domain. A negative radicand of any size is not finite (`sqrt` of a negative number), even a rounding residue of a quantity that is 0 or more exactly, and the request is rejected. `four_bar_rocker_angle` reaches one inside its range: with `lc = 1.7939999999998988`, `lco = 94.027`, `lr = 5.407`, `lg = 90.414`, `theta2 = 0` (the Grashof margin is about 1e-13) the radicand of `across` is -1.8e-12 in double and +1.2e-12 exactly, and the request is rejected. At `theta2 = 0` every operation of the expression is exact or correctly rounded in the same order (`cos 0 = 1`, `sin 0 = 0`, `sqrt(dx^2) = |dx|`), so every implementation computes the same double.
- **`x-reduce ... max over t >= 0`.** The trajectory is infinite, so the program chooses how long to simulate. Choose a horizon long enough that no later maximum can exceed the one you report, for example simulate until the trajectory has turned and is decaying below the reported value. The horizon is your choice: it is not reported and not checked, only the reported maximum is (within the law's tolerance).
- **`x-invariant` lines** are checked by the checker on the trajectory you report. Your program never evaluates them and never rejects a request because of them. Report the trajectory the law asks for; the checker decides whether it satisfies the invariants.

### Audit block

An audit block holds one clause per line:

| clause | meaning |
|--------|---------|
| `audit <name>` | the audit name |
| `evidence <metric>` | the metric must have been measured. It is evidence only when it is a finite number greater than 0. Missing, 0, a negative number, or a value that is not a number means no evidence. |
| `expect <metric> == <value> [within <tol>]` | the metric must equal the value, within the tolerance (default 0) |
| `range <key> <value>...` | the set of known values for `<key>`. The measured list must be the same set. Order and duplicates are ignored, on both sides. |
| `includes <key> <value>...` | the measured list must contain every listed value; it may contain more. Order and duplicates are ignored, on both sides. |

The measurements are the request inputs:

- A **numeric metric** is a JSON number. A value that is not a number (text, `true` / `false`, `null`, an array, an object) counts as not measured, as if the key were missing. A number that is not finite after parsing (an overflowing literal such as `1e400`) is not a number, so it is not measured either.
- A **range key** (the key of a `range` or an `includes` clause) is an array of text, declared by an `x-input` line (`x-input known-mismatches list of text`). A value that does not match the type of that line, including `null`, counts as not measured, so the verdict for that key is `out_of_range`. A range key that no `x-input` line declares is never measured, whatever the request gives. An empty array is a measured, empty set.
- In an audit law the `input` lines describe the measurements for people and are not read: a unit or a `range` on them has no effect. The numeric metrics are the request keys that the clauses name.
- Values are compared as text. Two values are the same only when their text is identical.
- `x-metric` lines define derived metrics (below). A name defined by an `x-metric` line is never read from the request, even when the request has a key of that name. An `x-input` and an `x-metric` may have the same name (`identifier_feature_independent` declares `x-input builds` and `x-metric builds = count(builds)`): the request key is read as the `x-input`, and the metric of that name is the value derived from it, not the value of the key.
- A law file is UTF-8 text. Lines are split on LF only; a CR directly before an LF belongs to that line end. Among control characters only TAB, LF and the CR of a CR LF may appear. Every other control character (U+0000–U+001F except TAB / LF, U+007F–U+009F, a CR that is not directly before an LF, including at the very end of the file), every format character (U+00AD, U+0600–U+0605, U+061C, U+06DD, U+070F, U+0890–U+0891, U+08E2, U+180E, U+200B–U+200F, U+202A–U+202E, U+2060–U+2064, U+2066–U+206F, U+FEFF (a byte-order mark), U+FFF9–U+FFFB, U+110BD, U+110CD, U+13430–U+1343F, U+1BCA0–U+1BCA3, U+1D173–U+1D17A, U+E0001, U+E0020–U+E007F) and U+2028 / U+2029 make the file unreadable, wherever they are (also in a comment): otherwise two readers could split or see the text differently.
- A number in a law file (the `x-at-least` floor, the value and tolerance of an `expect` clause) is ASCII decimal: an optional sign, digits with an optional `.` and fraction or a `.` and digits, and an optional exponent (`[+-]? ([0-9]+ (\. [0-9]*)? | \. [0-9]+) ([eE] [+-]? [0-9]+)?`), and it must be finite as a double. The tolerance of `expect` and the floor of `x-at-least` are not negative (`within -1` and `x-at-least s -1` do not read; `-0` is 0). `2`, `2.0`, `+2`, `2.`, `.5`, `-1`, `2e0` are numbers; `nan`, `inf`, `infinity`, `1_0`, `0x2`, a full-width or non-ASCII digit and `1e400` are not, and a law file with one does not read.
- Each declaration and clause has exactly its tokens: `x-at-least <metric> <number>`, `audit <name>`, `evidence <metric>`, `expect <metric> == <number>` or `expect <metric> == <number> within <number>`, `range <key> <value>...` or `includes <key> <value>...` (at least one value). An extra or a missing token, a line in the audit block that is none of these, a missing or a second `audit <name>` line, an audit block without clauses, an unclosed block or a second `begin audit` block makes the law file unreadable.
- The prefix `x-` is reserved for declarations. A line whose text starts with `x-` in any case or compatibility form (`X-`, a full-width `ｘ` or `－`: any character whose NFKC form is `x` / `X` followed by one whose NFKC form is `-`), also after leading spaces or a tab, is a declaration, and it must be written as `x-` in lowercase from the first column. Write prose that would start a line with `x-` (an "x-ray") in another way.
- A declaration line is the keyword (`x-input`, `x-metric`, `x-at-least`), exactly one ASCII space, and the rest. A line starting with `x-` that is not one of these keywords in this form (a tab or two spaces after the keyword, a misspelt keyword, an unknown `x-` line) does not read, and neither does an `x-at-least` line naming a metric no `x-metric` line defines. A law file with such a line does not read: a request to it is an error of the request.
- A law file declares each `x-input` name and each `x-metric` name once, and gives at most one `x-at-least` line per metric. A law file that repeats one (even with the same text) does not read: a request to it is an error of the request, as for an unknown law.

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
3. **Expect and includes clauses** (the claims), together in file order:
   - `expect`: if the metric is not measured, the verdict is `undecided`; if the metric is outside the tolerance, the verdict is `breaks`. The subject is the metric.
   - `includes`: if the key is not measured, the verdict is `undecided`; if a listed value is not in the measured list, the verdict is `breaks`. The subject is the key.
4. Otherwise the verdict is `supports`, and the subject is `null`.

Output: `{"outputs": {"verdict": "<one of the six words>", "subject": "<name>" or null}}`. Audits never reject, and every measurement is optional, so a missing measurement is not a request error. A request to an audit law without the `inputs` key, or with `"inputs": null`, is the same as `"inputs": {}`: nothing is measured.

## Rule laws

A rule law states a rule of a product instead of a quantity: a **state machine** (`kind machine`), a **predicate** over typed inputs (`kind predicate`) a **table** whose first holding row gives the answer (`kind table`), the clauses of a **statute** (`kind statute`), the steps of an administrative **procedure** (`kind procedure`), the obligations of a **contract** (`kind contract`), or one **memory** of a character (`kind memory`). Its answer is exact: every output is text, `true` / `false`, `null` or a list of texts, and it is compared exactly.

### Reading a rule law file

The characters, the comments, the line ends and the tokens are read as for every law file (above). In addition:

- **Every line must be read.** A line of a rule law file is empty (after its comment is removed), a line for people (`law`, `claim`, `source`, `verdict`, `oracle`, which are not read), the `kind` line, or one of the lines of its kind below. Any other line makes the law file unreadable, and a request to it is an error of the request. A rule law file has exactly one `kind` line.
- **Names** (of constants, inputs, bound parts, states, events, mode values, guards, allowances and table values) are ASCII: a lower-case letter followed by lower-case letters, digits and `_`. A name is declared once in a file, whatever it names. The words `true`, `false`, `and`, `or`, `not`, `state`, `mode`, `shape` and the function names below are not names.
- **`const <name> <type> = <literal>`** declares a named constant. The type is `integer`, `time`, `text`, `bytes` or `bool`, and the literal is one of that type (below). A constant is used by its name in an expression.
- **Literals.** An integer is ASCII digits (`0` or a digit 1-9 followed by digits), at most 2^53 - 1 (9007199254740991). A time literal is written as an integer. A text is `"` followed by characters other than `"` and `\` and then `"`; it has no escapes. A byte string is `0x` followed by an even number of lower-case hex digits (`0x` alone is the empty byte string). `true` and `false` are the two booleans.
- **Types of values.**
  - `integer`: a JSON number that is finite, has an integer value and is at most 2^53 - 1 (9007199254740991) in magnitude (`3` and `3.0` are the same integer).
  - `time`: a number of seconds since 1970-01-01 00:00:00 UTC, read like `integer`. A time is compared only with a time.
  - `text`: a JSON string.
  - `bytes`: a JSON string of lower-case hex digits, of even length (`""` is the empty byte string).
  - `bool`: `true` or `false`.
  - `record(<field>: <type>, ...)`: a JSON object that has every field, each of its type. Other keys are ignored. A field is used as `<input>.<field>`.
  - `money`: a JSON string `<amount> <currency>`, an amount of minor units and a currency, separated by one ASCII space. The amount is `0`, or an optional `-` followed by a digit 1-9 and digits (no `+`, no leading zero), from -2^127 to 2^127 - 1; the currency is three upper-case ASCII letters that the law's currency table knows (below). A JSON number is not money. A money constant is written the same way without the quotes: `const fee money = 1200 JPY`.
- **The currency table.** A law with a `money` input or constant has the line `currencies <id>`, exactly once: the law identifier (64 lower-case hex digits) of the currency table it reads, a table law with the input `x-input code text` whose values are names `e<digits>` (the number of minor-unit digits of a currency: `e0` for JPY, `e2` for USD) and one more value given when a code is unknown. A currency is known when the table's answer for its code is an `e<digits>` value. The table is a law of its own (an edition of a rule set, because currencies change); a `reader` finds it among the law files of `LOL_LAW_DIR`: it is the rule law file whose identifier is the one named. When no file of the directory is that table, a request to the law is an error of the request (status 2). A law without money has no `currencies` line.
  - A value of another JSON type, or text that is not of the form, does not have the type.
- **Every expression is typed when the file is read.** A name that is not declared (in a predicate law: on an earlier line; in a machine law: anywhere in the file), a function or operator applied to values of other types, or a `require` / `when` that is not a boolean makes the law file unreadable.

### Limits of a rule law file

A rule law file within these limits reads (when it is otherwise readable); a file past any of them does not read, and a request to it is an error of the request. A reader checks a limit before it builds what the limit bounds; it does not stop with a crash, cut the file short, or read a part of it.

| limit | value | counts |
|---|---|---|
| size of the file | 4194304 bytes (4 MiB) | the bytes of its UTF-8 text |
| lines read | 100000 | the lines that are read: not the empty lines and the comments |
| tokens of an expression | 256 | the tokens of one `require` / `when` / row / `allow` expression |
| nesting of an expression | 64 | each parenthesised group, `not` and function call is one level inside the levels around it: `(a)` has 1, `not (a)` has 2 |
| characters of a numeral | 40 | an integer, a time or an amount of money with its sign (-2^127, the smallest amount, has 40), and every number of the `example` line's JSON |
| nesting of the `example` JSON | 512 | levels of arrays and objects, the example object itself being level 1 (the same as a request) |

A numeral of more characters is out of the range of every type: in a law file it does not read; in a request, a value with such a numeral does not have its type. The `example` line's JSON is read as a request is (above: RFC 8259, no `NaN` / `Infinity`, the last of a repeated key, no lone surrogate); a JSON text that is not one, or not an object, makes the law file unreadable. The limit on the tokens also bounds a chain: `a and b and c and ...` has at most 128 operands. The other ranges stay as above (integers and times at most 2^53 - 1, amounts from -2^127 to 2^127 - 1, statute times at most 18446744073, JSON input nested at most 512 levels).

### State machine (`kind machine`)

| line | meaning |
|------|---------|
| `mode <value> <value>...` | at most one line: the request input `mode` is one of these values; it does not change |
| `state <name>` / `state <name> initial` | a state; exactly one state is `initial` |
| `event <name>` / `event <name> guard <guard> <guard>...` | an event, and the booleans the request gives with it |
| `on <event> from <state> <state>... to <state>` / `... when <expression>` | a transition: the event, taken in one of the `from` states when the expression holds, leads to the `to` state |
| `allow <name> when <expression>` | an allowance, evaluated after the step |
| `const ...` | a named constant (above) |

A machine law has at least one state, one event and one `on` line, and every name on an `on` line is a declared state or event.

**Expressions** of a machine law: `state == <state>`, `state != <state>`, `mode == <value>`, `mode != <value>`, the guards of the event of the `on` line (only in its `when`), boolean constants, `true` and `false`, combined with `not`, `and`, `or` and parentheses (`not` binds tighter than `and`, and `and` tighter than `or`). In an `allow` line `state` is the state after the step, and a guard is not available.

**A request** gives one step:

```json
{"law": "<name>", "inputs": {"mode": "<value>", "state": "<state>", "event": "<event>", "guards": {"<guard>": true}}}
```

- `state` and `event` (and `mode` when the law has a `mode` line) are required: a request without one of them is an error of the request (exit with status 2).
- A `state`, `event` or `mode` that is not text, or not a declared state, event or mode value, is rejected.
- `guards` is an object. Each guard of the requested event must be in it as `true` or `false`; other keys are ignored. A `guards` value that is not an object, a missing guard of the event and a guard that is not a boolean are rejected. A request without `guards` is the same as `"guards": {}`.

**The step.** The `on` lines are tried in file order. The first one whose event is the requested event, whose `from` states contain the requested state and whose `when` holds (or has none) is taken: the new state is its `to` state. When none is taken the event is refused and the state does not change.

**Output:** `{"outputs": {"state": "<new state>", "transition": "taken" or "refused", "allow": {"<name>": true or false, ...}}}` with one entry per `allow` line (`{}` when there is none).

### Predicate (`kind predicate`)

| line | meaning |
|------|---------|
| `x-input <name> <type>` | an input of the request, with its type (above; a record's fields are of the other types) |
| `x-split <source> "<separator>" <name> <name>...` | the text `<source>` (an input or a part bound earlier) split on every occurrence of the separator (a text literal, not empty) into exactly as many parts as names |
| `x-bind <name> <part> integer` / `time` / `hex` | the part read as an integer or a time (`0`, or an optional `-` followed by a digit 1-9 and digits, at most 2^53 - 1 (9007199254740991) in magnitude: no `+`, no leading zero, no space) or as hex (lower-case hex digits of even length, giving bytes) |
| `use alice-sign hmac_sha256` | the law calls `hmac_sha256` (below) |
| `require <label>: <expression>` | a condition, named by its label |
| `const ...` | a named constant (above) |
| `example <JSON object>` | request inputs for which the predicate holds, written on one line. It is not read by an implementation; the checks start from it, and a law file whose example does not hold, or is not a JSON object, does not read |

A predicate law has at least one `require` line and one `example` line. Each `x-` line here is the keyword, exactly one ASCII space, and the rest, as for the other `x-` lines.

**Expressions** of a predicate law (from the loosest binding to the tightest):

| form | types | value |
|------|-------|-------|
| `a or b`, `a and b`, `not a` | bool | as usual |
| `a == b`, `a != b` | two values of one type (not a record) | equality; bytes and text are equal when they are the same sequence |
| `a < b`, `a <= b`, `a > b`, `a >= b` | two integers, two times, or two money | the order of the numbers; of money, the order of the amounts in minor units |
| `a + b`, `a - b` | two money | the sum or the difference of the amounts, in the same currency |
| `a ++ b` | bytes | the concatenation |
| `len(x)` | text or bytes | the number of bytes (of the UTF-8 form of a text): an integer |
| `prefix(b, n)` | bytes, integer | the first `n` bytes of `b`; all of `b` when `n` is at least its length; empty when `n` is 0 or less |
| `utf8(t)` | text | the UTF-8 bytes of `t` |
| `hmac_sha256(key, message)` | bytes, bytes | HMAC-SHA256 (RFC 2104 with SHA-256): 32 bytes. Only in a law with the `use` line |

Comparisons do not chain (`a < b < c` makes the law file unreadable); parentheses group. `+` and `-` bind tighter than the comparisons and looser than `++`, from left to right.

**Money** is compared and added by its amount in minor units only: two amounts of one currency are compared as integers, and nothing is converted between currencies or between numbers of minor-unit digits. Two money values of different currencies are never compared or added: when both are constants the law file does not read, and when a request's money input has another currency than the value it meets, the request is rejected. A sum or difference outside -2^127 to 2^127 - 1 also rejects the request (it is never wrapped or clamped). `and` and `or` stop at the first operand that decides them: an operation they do not reach is not evaluated and does not reject.

**A request** gives the `x-input` values:

- A request without one of the `x-input` keys is an error of the request (exit with status 2). A missing input is decided before any other check.
- An input value that does not have its type is rejected.

**The answer.** First the `x-split` and `x-bind` lines are applied in file order. When a split does not give exactly its number of parts, or a part does not read as its type, the predicate does not hold and `failed` is `shape`. Then the `require` lines are evaluated in file order; the first that does not hold gives its label as `failed`.

**Output:** `{"outputs": {"valid": true, "failed": null}}` when every line holds, otherwise `{"outputs": {"valid": false, "failed": "<label or shape>"}}`. A predicate never rejects a request whose inputs have their types, except when its money meets another currency or leaves the range of an amount (above).

The time an implementation takes to compare two byte strings should not depend on where they first differ (compare a secret, such as a tag, in constant time). The checks cannot observe this, and nothing is scored on it.

### Table (`kind table`)

| line | meaning |
|------|---------|
| `x-input <name> <type>` | an input of the request, with its type (as in a predicate law) |
| `value <name> <name>...` | exactly one line: the values the table gives (more than one) |
| `row <label>: <value> when <expression>` | a row: its label, the value it gives, and when it holds |
| `otherwise <value>` | exactly one line, after every `row` line: the value when no row holds |
| `const ...` | a named constant (above) |

A table law has at least one `row` line. Every value of the `value` line is given by a `row` or by the `otherwise` line, and a `row` or `otherwise` line gives only a declared value. Labels have a namespace of their own (as the labels of a predicate law): a label is a name that is not one of the words above, and no two rows have the same label. Each `x-input` line is the keyword, exactly one ASCII space, and the rest.

**Expressions** of a table law are those of a predicate law, without `hmac_sha256` (a table law has no `use` line): the inputs and the constants declared on an earlier line, the literals, and the operators and functions of the predicate table above. A `when` expression is a boolean.

**A request** gives the `x-input` values, read as in a predicate law: a request without one of the `x-input` keys is an error of the request (exit with status 2), decided before any other check; an input value that does not have its type is rejected.

**The answer.** The `row` lines are evaluated in file order. The first whose expression holds gives the answer: its value and its label. When none holds the answer is the value of the `otherwise` line, and the row is `null`.

**Output:** `{"outputs": {"value": "<value>", "row": "<label>"}}`, or `{"outputs": {"value": "<value>", "row": null}}` when no row holds. A table never rejects a request whose inputs have their types, except when its money meets another currency or leaves the range of an amount (see "Predicate" above).

### Statute (`kind statute`)

A statute law is a title and clauses. Each clause has a legal nature, may be under an earlier clause, and is in force from a time and optionally until one. Its lines:

| Line | Meaning |
|---|---|
| `title <text>` | the statute's title: the rest of the line, exactly once |
| `clause <label> <kind> [of <clause>] [from <time>] [until <time>]: <text>` | a clause, at least one. The label is a lower-case ASCII letter, then lower-case letters, digits and `_` (any such word, the words of expressions included), and is unique in the law. The kind is one of `obligation`, `prohibition`, `permission`, `definition`, `penalty`, `exception`. `of <clause>` names an earlier clause the clause is under; an `exception` always has one. The optional parts come in this order. `from` is the start (inclusive) and `until` the end (exclusive), time literals of at most 18446744073 (their nanoseconds fit 64 bits); `until` is after the start. Without `from` the start is the edition's start, or 0 for a law that is not an edition. The text is the rest of the line after the first `:`, not empty |

A statute law has no `x-input`, `const` or `currencies` line.

**A request** gives `at`, a time. A request without `at` is an error of the request (exit with status 2). An `at` that is not a time is rejected.

**Output:** `{"outputs": {"in_force": ["<label>", ...]}}`: the labels of the clauses in force at `at` (the start is at or before `at` and the end, if any, is after it), in file order. A time before 1970 is a time: no clause is in force then. A statute never rejects a request whose `at` is a time.

### Procedure (`kind procedure`)

A procedure law is a state machine (its lines, request and answer are those of "State machine" above) over the steps of an administrative procedure. Its states are statuses of a procedure, its events are kinds of step, and the status a step leads to is fixed:

- the states are among `pending`, `in_progress`, `approved`, `rejected`, `completed`, and the initial state is `pending`;
- the events are among `application`, `review`, `approval`, `rejection`, `amendment`, `notification`, `completion`;
- every `on <event> from <state>... to <state>` line goes, from each of its states, where a step of that kind leads: an `approval` to `approved`, a `rejection` to `rejected`, a `completion` to `completed`, and any other step from `pending` to `in_progress` and from any other status to that status.

A file that breaks one of these does not read. The law decides which steps may come in which status (a step with no `on` line for the status is refused); it never decides where a step leads. A procedure law has no `currencies` line.

### Contract (`kind contract`)

A contract law is its parties, the time it was made, conditions and obligations of money. It has one `currencies <id>` line (above: its obligations are money). Its lines:

| Line | Meaning |
|---|---|
| `party <label>` | a party (a label as for a clause: a lower-case ASCII letter, then lower-case letters, digits and `_`) |
| `created <time>` | when the contract was made, exactly once |
| `condition <label> decided by <id>: <description>` | a condition: the identifier (64 lower-case hex digits) of the rule law that decides it, and a description for people after the first `:`, not empty |
| `obligation <label> <party> to <party> <amount> <currency> due <time>` | an obligation, at least one: the first party owes the second (two different parties declared on earlier lines) the money `<amount> <currency>` (written as a money constant, more than 0) by the time `due` |

The labels of parties, conditions and obligations are unique together. Every time is at most 18446744073 and no obligation is due before `created`. The obligations are in one currency, one the currency table knows.

**A request** gives `now`, a time, and optionally `fulfilled` and `met`: lists of the labels of the obligations fulfilled and of the conditions met (without the key, none). A request without `now` is an error of the request (exit with status 2). A `now` that is not a time, a `fulfilled` or `met` that is not a list (`null` included), or a list with an element that is not a label of that list's kind or with a label twice is rejected.

**Output:** `{"outputs": {"status": "<status>", "unfulfilled": ["<label>", ...], "owed": "<amount> <currency>", "conditions_met": true | false}}`. The status is `breached` when an obligation not fulfilled is due before `now` (`now` is after its `due`), otherwise `fulfilled` when every obligation is fulfilled, otherwise `active`. `unfulfilled` is the labels of the obligations not fulfilled, in file order; `owed` is the sum of their amounts (a sum outside -2^127 to 2^127 - 1 rejects the request); `conditions_met` is whether every condition is in `met` (`true` without conditions).

### Memory (`kind memory`)

A memory law is one memory of a character: what kind of memory it is, when it was formed, how it weighs on the character's traits, which other memories it rests on, and when it comes to mind. A program computes the character from the set of memory laws it holds; taking a law out of the set takes its weights out of the computation. Its lines:

| Line | Meaning |
|---|---|
| `category <name>` | the kind of memory, exactly once |
| `formed <integer>` | when it was formed, as an ordinal (an integer literal, at most 2^53 - 1), exactly once |
| `weight <axis> <weight>` | its weight on a trait: `<axis>` is a name, at most one `weight` line per axis; `<weight>` is an integer literal with an optional `-` before it, from -2147483647 to 2147483647 (`-0` is not a weight) |
| `link <id>` | the law identifier (64 lower-case hex digits) of a memory it rests on; each identifier at most once |
| `x-input <name> <type>` | an input of the request, with its type (as in a predicate law, without `money`) |
| `const ...` | a named constant (above, without `money`) |
| `recall when <expression>` | when the memory comes to mind, at most once: a boolean over the inputs and the constants declared on earlier lines (the expressions of a table law) |
| `line <label>: <text>` | a line the character may say when the memory comes to mind: a label (as a row label) unique among the `line` lines, then the text after the first `:`, not empty. Not part of the answer and not part of the identifier |

A memory law has no `currencies` line. A file without `recall` never recalls.

**A request** gives the `x-input` values, read as in a predicate law: a request without one of the `x-input` keys is an error of the request (exit with status 2), decided before any other check; an input value that does not have its type is rejected.

**Output:** `{"outputs": {"recalled": true | false}}`: the value of the `recall` expression (`false` without a `recall` line).

**Identifier.** Every line that changes what a program computes from the law is part of the identifier: the category, `formed`, the weights (in the order of their axis names), the links (in the order of the identifiers), the inputs, the constants and the `recall` expression. The `line` lines are not: rewording what the character says does not make another memory.

### Editions of a rule set (`domain` / `edition`)

A rule law may be one edition of a rule set (a domain such as `legal` or `tax`): the rules in force for a period. Such a file has the five lines below, each exactly once; a file has all five or none of them. They are read before the lines of the kind, in any position of the file, and they are not names of the law.

| line | meaning |
|------|---------|
| `domain <name>` | the rule set: a lower-case ASCII letter followed by lower-case letters, digits, `_` and `-` |
| `edition <name>` | this edition's name in its domain (the same alphabet) |
| `supersedes first` or `supersedes <id>` | the law identifier of the edition this one replaces (64 lower-case hex digits), or `first` for the first edition of the domain |
| `effective <time>` or `effective <time> until <time>` | the edition is in force from the first time (inclusive) to the second (exclusive); a time is an integer literal as above (seconds since 1970-01-01 00:00:00 UTC, at most 2^53 - 1), and the second is after the first |
| `semantics <id>` | the semantics identifier the edition was written against (64 lower-case hex digits). A reader reads the file only when it evaluates that semantics; otherwise the file is unreadable and a request to it is an error of the request. A law with money (a `currencies` line) is evaluated under a semantics of its own, one for a predicate and one for a table, so its edition names that one |

The five lines do not change the answer to a request: a rule law answers as the same file without them. They are part of the law's identity (an edition with another period or another predecessor is another law).

### A later edition written as a delta

The first edition of a domain is a full rule law file. A later edition may be a **delta** (a file ending in `.delta`): the five header lines and operations on the edition it supersedes, and no `kind` line. The edition is the **resolved text** below, read as a rule law file; its identifier is that of the resolved text, and it answers as the resolved text does.

- **Body.** The body of a rule law file is the lines that are read: its lines without their comment, without the empty ones, without the five header lines and without the lines for people (`law`, `claim`, `source`, `verdict`, `oracle`), each without the spaces and tabs at its end, in file order.
- **Resolved text.** The five header lines in the order `domain`, `edition`, `supersedes`, `effective`, `semantics` (the keyword and each value separated by one space), then the body; every line ends with a line feed. For a full file it is built from the file itself; for a delta, from the delta's header and the body of the resolved text of the edition it supersedes after the operations.
- **Key of a body line.** `row <label>:` and `require <label>:` lines: the keyword, one space and the label. `line <label>:` lines: the keyword, one space and the label. `const`, `x-input`, `x-bind`, `allow`, `state`, `event`, `clause`, `party`, `condition`, `obligation`, `weight`, `link` lines: the keyword, one space and the second word. `x-split` lines: the keyword, one space and the source. `on` lines: the words before `to`, separated by one space. Any other line: its keyword. A body with two lines of the same key does not resolve.
- **Operations**, applied in file order (a key never contains `:`, so an operation's key ends at its first `:`):

| line | effect |
|------|--------|
| `replace <key>: <line>` | the body line with that key becomes `<line>`, which has the same key |
| `remove <key>` | the body line with that key is removed |
| `add <line>` | `<line>`, whose key is not in the body, goes after the last body line with the same keyword, or at the end when there is none |
| `add after <key>: <line>` / `add before <key>: <line>` | `<line>` goes after / before the body line with that key |
| `changes <name>` | no change to the body: names an example whose answer this edition changes (see below) |

A delta whose `domain` differs from the superseded edition's, whose `supersedes` is not that edition's identifier, or whose operation names a key that is not in the body (or adds one that is), does not resolve.

## Not specified (do not rely on it)

The checks do not decide these points, and no rule is given for them yet. A program may answer them in any way, and nothing here is scored on them; a program that reads law files when it runs (`reader`) should not depend on any particular answer.

- **The text after `=` on `x-reduce`, `x-output` and `x-step fixed` lines.** It is written for people (`max over t >= 0`, the shape of the reported output). There is no grammar for it yet: how a program would read it, and what it may contain, is open.
- **Whether a program evaluates the `tolerance` lines.** They name the outputs (above). Whether a program computes the tolerance values, or leaves them to the checker, is open.
- **How the reported output of a simulation is linked to its states.** Which `x-state`, at which times (each step of `x-step fixed`, or the times of a list input), forms the `x-output`, when the law does not say it in words, is open.
- **A rate of change that depends on time.** An `x-ode` expression that uses the time itself (not only the states and inputs) is not used by any law, and its meaning is open.
- **Whether the closed form of a simulated law must be finite.** The program reports its simulation, and the closed form is the reference it is judged against. Whether a request is rejected when the closed form is not finite there but the simulation is, is open.
- **Reading a quantitative law file of your own.** A law file of your own without the line `kind audit`, `kind machine`, `kind predicate`, `kind table`, `kind statute`, `kind procedure` or `kind contract` is an unknown law (above: a request to it exits with status 2), so today a `reader` reads only the audit and rule law files it is given. What a `reader` would have to read from a quantitative law file of its own, if the checks gave it one (its units, which no rule checks; its `x-` lines; its expressions), is open.
