# rubik

`rubik` is a Rust solver and simulator for general `NxNxN` Rubik-style cubes,
with the solver aimed at very large cubes rather than fewest-move 3x3 solving.
The project keeps the cube as facelets, solves by reduction, and has separate
execution and storage backends so the same logical solver can be run in a fully
recorded mode or in a sparse, optimized mode.

## Setup And Run

Install the Rust toolchain, then build and run the main binary from the repo
root:

```sh
cargo run --release -- --n 5 --mode optimized --backend byte
```

The main binary is `rubik` and prints the initial configuration, the scrambled
net, each stage timing, and the solved result. Defaults are `n=5`,
`mode=standard`, `backend=byte`, `scramble-rounds=3`, and `seed=42`.

Useful examples:

```sh
# Fast large-cube path
cargo run --release -- --n 4096 --mode optimized --backend three_bit

# Fully recorded/debuggable path
cargo run --release -- --n 7 --mode standard --backend byte --seed 0xC0FFEE

# Plain ASCII output, useful when redirecting output
cargo run --release -- --n 5 --plain-render
```

CLI options:

```text
-n, --n <N>                     cube side length
-m, --mode <MODE>               standard | optimized
-b, --backend <BACKEND>         byte | nibble | three_bit | third_byte
-r, --scramble-rounds <ROUNDS>  each round is 3*n random layer moves
-s, --seed <SEED>               decimal or 0x-prefixed hex seed
--plain-render                  disable ANSI color rendering
```

For non-rendered benchmark runs, use:

```sh
cargo run --release --bin run_pipeline_no_render -- --n 4096 --mode optimized --backend byte
```

Run the test suite with:

```sh
cargo test
```

## What This Is

This is not a human-style speedsolving project. It is a scalable big-cube
reduction solver:

1. Solve centers.
2. Solve corners.
3. Pair/solve edges.

The cube model is facelet-native. A cube has six `n x n` faces, each face stores
facelets, and piece concepts such as corners and edge wings are derived views
over facelets when a stage needs them. This keeps the simulator able to
represent arbitrary facelet assignments, while still allowing solver stages to
reason about cubies.

The implementation is organized around these layers:

- `src/model`: facelets, faces, cube state, scramble logic, solved checks.
- `src/layout`: matrix storage, line/strip traversal, geometric move planning.
- `src/simulation`: face/axis conventions, derived edge/corner locations, net rendering.
- `src/storage`: interchangeable facelet storage backends.
- `src/algorithms`: center, corner, edge algorithms plus optimized operations.
- `src/solver`: execution context, move statistics, stage contracts, pipeline.
- `src/bin`: benchmark and generation binaries.

## Cube And Move Model

Moves are represented as `(axis, depth, angle)`:

- `X`: left-to-right, `depth=0` is L and `depth=n-1` is R.
- `Y`: down-to-up, `depth=0` is D and `depth=n-1` is U.
- `Z`: back-to-front, `depth=0` is B and `depth=n-1` is F.
- Angles are positive quarter turn, double turn, or negative quarter turn.

Outer face rotations are virtualized. Each face has a rotation value modulo 4,
so turning an outer face does not rewrite all `n*n` stickers. A move updates the
four side strips of the layer and adjusts face metadata for an outer layer. This
is the same core idea that makes very large face turns tractable.

The standard net is:

```text
  U
L F R B
  D
```

with the default color scheme:

```text
U = White, D = Yellow, R = Red, L = Orange, F = Green, B = Blue
```

## Scrambling

The main binary uses uniform random layer scrambles. One scramble round contains
`3*n` moves, and each move chooses:

- axis uniformly from `X`, `Y`, `Z`
- depth uniformly from `0..n`
- angle uniformly from positive, double, negative

The default is 3 rounds, so the default scramble length is `9*n` moves. The RNG
is a deterministic `XorShift64`, which makes benchmark and CLI runs
reproducible from the seed.

The codebase also contains additional internal scramble tools:

- `scramble_layer_sweeps`: visits every layer on every axis per round with
  random angles.
- `scramble_biased_random_layers_with_outer_probability`: chooses outer layers
  with a configured probability.
- `scramble_direct`: writes a reachable random-looking state directly into
  piece orbits; it is used as a validation/reference path, not by the main CLI.

The checked-in scramble statistics compare uniform random layers against layer
sweeps. For `n=20`, 64 trials, the default-style uniform scramble is already
near its long-run distribution by `k=3` rounds:

| k | method | face/color TV | neighbor-pair TV | average scramble time |
|---:|---|---:|---:|---:|
| 1 | uniform random layers | 0.13098 | 0.11876 | 0.0138 ms |
| 1 | layer sweeps | 0.27362 | 0.32677 | 0.0123 ms |
| 3 | uniform random layers | 0.04398 | 0.02474 | 0.0392 ms |
| 3 | layer sweeps | 0.08698 | 0.07459 | 0.0426 ms |
| 8 | uniform random layers | 0.04141 | 0.02312 | 0.1074 ms |
| 8 | layer sweeps | 0.04366 | 0.02311 | 0.0933 ms |

Source data: [`benchmark/scramble_stats_n20.csv`](benchmark/scramble_stats_n20.csv).

There is also an outer-layer probability sweep in
[`benchmark/scramble_probability_sweep.txt`](benchmark/scramble_probability_sweep.txt).
The analytic balance for biased scrambles is `p = 2/n`: that makes one outer
layer and one inner layer receive the same expected hit count per `k`. On the
requested 5%-95% grid for `n=500..4000`, the best grid value is 5%, but the true
analytic optimum is below that grid for those large sizes.

## Solver Pipeline

### Centers

The center stage uses `CenterReductionStage::western_default()`. It performs a
fixed set of color transfers that place centers into the western color scheme:
red to R, orange to L, green to F, yellow to D, and blue to B. White is left once
the other centers are solved.

For odd cubes, true centers are first aligned by a small BFS over middle-slice
moves. For scalable centers, the stage scans the source and destination center
areas and batches compatible row/column targets. The hot path avoids repeated
geometry derivation by using:

- `CenterCommutatorTable`: all valid destination/helper/angle commutators.
- `GENERATED_CENTER_SCHEDULE`: generated source/destination routes.
- raw storage scans over center rows.
- virtual face rotations for center setup instead of immediately applying every
  setup face turn.

The center commutator is the core operation. In expanded literal form it is:

```text
helper columns^-1
destination face turn
helper rows^-1
destination inverse
helper columns
destination face turn
helper rows
```

The normalized form appends the inverse of the net destination face turn, so its
net effect is a sparse center 3-cycle rather than a 3-cycle plus an outer face
rotation. For each selected `(row, column)` pair, the optimized implementation
applies only the three changed center facelets. Because rows and columns can be
batched, one logical commutator operation can move many center facelets while
still recording the literal move count that would have been used.

### Corners

The public `CornerReductionStage` is the two-cycle corner reducer. It reads the
current reduced corner state, then uses precomputed setup sequences for ordered
corner pairs. It has two canonical recipes:

- a corner swap recipe for placing cubies into home slots
- a paired twist recipe for resolving orientations

The stage builds a literal outer-face sequence and applies it through
`MoveSequenceOperation`. In standard mode those moves are recorded. In optimized
mode the operation still reports the same move count, but it applies the planned
move effects without retaining a move history.

There is also a `CornerSearchStage`. It builds reduced corner permutation and
orientation move tables and solves with iterative deepening using pruning
distances. It is useful as a reference/oracle, but it is not the default stage
used by the main pipeline.

### Edges

The edge stage solves edge wings and odd-cube middle edges using exact sparse
3-cycles.

For each wing orbit, the solver scans 24 positions, assigns current edge color
keys to solved slot targets, decomposes the even assignment into ordered
3-cycles, and asks a prepared orbit planner for a setup to the canonical working
cycle. The canonical wing cycle is represented as an `EdgeThreeCyclePlan`, whose
optimized effect is six sticker updates.

After slot pairing, the stage fixes wing orientation using prepared orientation
operators and a bitmask model over edge slots. Odd cubes also have a middle-edge
orbit. Middle-edge solving uses its own setup table, exact middle-edge cycles,
and parity/precheck operators. After middle-edge work, wing orbits are refreshed
again because middle moves can disturb wing state.

Like centers, the edge stage has a literal move sequence model and a sparse
optimized model. Tests assert that recorded and unrecorded paths reach matching
cube states and move statistics.

## Execution Modes

`standard` mode is the fully recorded path:

- algorithms generate literal move sequences
- moves are applied one by one
- the solution move list is stored
- output reports recorded solution moves

`optimized` mode is the large-cube path:

- optimized operations apply sparse effects directly when available
- only move statistics are recorded
- the full move history is not stored
- hot paths use generated schedules, raw storage reads/writes, prepared plans,
  and virtual rotations

Both modes are intended to have the same final cube effect and move counts. The
test suite includes characterization tests for standard/optimized equivalence.

## Storage Backends

Every face is backed by a `FaceletArray`. The storage backend changes memory
layout, not solver behavior:

| backend | representation | bytes for `n=65536` | bytes for `n=100001` | tradeoff |
|---|---:|---:|---:|---|
| `byte` | 1 byte per facelet | 24.00 GiB | 55.88 GiB | simplest and often fast |
| `nibble` | 4 bits per facelet | 12.00 GiB | 27.94 GiB | half-size byte storage |
| `three_bit` | 3 bits per facelet in `u64` words | 9.00 GiB | 20.96 GiB | compact and benchmark-fast |
| `third_byte` | 3 facelets per byte, base-6 packed | 8.00 GiB | 18.63 GiB | smallest, more div/mod work |

The exact storage estimate is only facelet storage for the six faces. Runtime
also needs stack/heap overhead, solver tables, benchmark process overhead, and
the operating system's own memory headroom.

## Timing And Benchmarks

Benchmark binaries live under `src/bin`:

```sh
cargo run --release --bin stages_benchmark -- --output benchmark/stages.svg --csv-output benchmark/stages.csv
cargo run --release --bin backends_benchmark -- --output benchmark/backends.svg --csv-output benchmark/backends.csv
cargo run --release --bin scramble_stats -- --output benchmark/scramble_stats.svg
cargo run --release --bin scramble_probability_sweep -- --output benchmark/scramble_probability_sweep.svg
```

The stage/backend benchmarks run `run_pipeline_no_render` and parse the
`Finished ... Time: ... ms` lines. The checked-in benchmark files are:

- [`benchmark/stages_13.csv`](benchmark/stages_13.csv)
- [`benchmark/stages_13.svg`](benchmark/stages_13.svg)
- [`benchmark/backends_12.csv`](benchmark/backends_12.csv)
- [`benchmark/backends_12.svg`](benchmark/backends_12.svg)
- [`benchmark/scramble_stats_n20.csv`](benchmark/scramble_stats_n20.csv)
- [`benchmark/scramble_probability_sweep.csv`](benchmark/scramble_probability_sweep.csv)

### Stage Scaling

The checked-in stage benchmark uses `backend=byte`, `mode=optimized`,
`scramble_rounds=3`, `attempts=3`, and `seed=42`. Selected measured rows:

| n | init | scramble | center | corner | edge | full pipeline |
|---:|---:|---:|---:|---:|---:|---:|
| 1024 | 0.014 s | 0.117 s | 0.284 s | 1.758 ms | 0.102 s | 0.519 s |
| 2048 | 0.051 s | 0.541 s | 1.597 s | 4.177 ms | 0.213 s | 2.407 s |
| 4096 | 0.223 s | 3.958 s | 8.267 s | 10.467 ms | 0.421 s | 12.879 s |
| 8192 | 0.836 s | 15.738 s | 33.297 s | 18.864 ms | 0.967 s | 50.858 s |

Power-law fits over the measured rows `n >= 1024`:

| stage | fitted exponent |
|---|---:|
| init | `O(n^1.99)` |
| scramble | `O(n^2.41)` |
| corner | `O(n^1.16)` |
| edge | `O(n^1.07)` |
| center | `O(n^2.30)` |

The center stage dominates solve time at large `n`; scramble can dominate the
full pipeline because the benchmark scramble applies many full layer moves.

### Backend Comparison

The checked-in backend benchmark uses `n=4096`, `mode=optimized`,
`scramble_rounds=8`, `trials=3`, and `seed=42`:

| backend | init | scramble | center | corner | edge | full pipeline |
|---|---:|---:|---:|---:|---:|---:|
| `byte` | 0.212 s | 2.295 s | 26.595 s | 10.737 ms | 0.381 s | 29.493 s |
| `nibble` | 0.459 s | 2.208 s | 29.148 s | 8.890 ms | 0.382 s | 32.205 s |
| `three_bit` | 0.779 s | 2.004 s | 25.845 s | 9.130 ms | 0.394 s | 29.031 s |
| `third_byte` | 0.542 s | 3.189 s | 35.887 s | 12.884 ms | 0.386 s | 40.016 s |

In that run, `three_bit` was the fastest full pipeline and `third_byte` was the
smallest but slowest. Backend choice is a memory/time tradeoff: if memory is not
tight, `byte` is simple and competitive; if memory is tight, `three_bit` is the
best current compromise from the checked-in numbers.

### Large-Cube Extrapolations

These extrapolations use the same fit method as `stages_benchmark`: fit each
measured stage independently on `n >= 1024` from
[`benchmark/stages_13.csv`](benchmark/stages_13.csv), then sum the stage
predictions.

| n | solve only: center + corner + edge | full pipeline: init + scramble + solve |
|---:|---:|---:|
| `2^16 = 65536` | 1.21 h | 1.94 h |
| `100001` | 3.20 h | 5.22 h |

The previous public large-cube reference for this project is
[ShellPuppy/RCube](https://github.com/ShellPuppy/RCube), which targets very
large cubes and advertises support up to `65536`. Against the prior `2^16`
comparison point of a 6 hour solve, this project's extrapolated `2^16`
solve-only time is about 1.21 hours, roughly 5x faster. Including initialization
and the benchmark scramble, the extrapolated full pipeline is about 1.94 hours.

These are extrapolated, not measured, numbers. They are based on a measured
range up to `n=8192`; actual runs at `n=65536` or `n=100001` depend heavily on
memory bandwidth, backend choice, CPU, allocator behavior, and whether the run
includes scrambling.

## Notes For Development

- Generated center routes come from:
  `cargo run --bin generate_center_schedule -- src/algorithms/centers/generated_center_schedule.rs`
- `RUBIK_PROGRESS=1` enables progress bars when stderr supports them; the main
  binary also enables progress automatically for `n >= 1000` on an interactive
  terminal.
- `RUBIK_EDGE_PROFILE=1` prints edge preparation/solve timing breakdowns to
  stderr.
- `cargo test` covers storage roundtrips, move geometry, sparse commutators,
  edge 3-cycles, stage contracts, and standard/optimized equivalence.
