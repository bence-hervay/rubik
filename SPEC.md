# NxNxN Rubik's Cube Solver - Target Specification

This document defines the target structure and functionality of the project.

The goal is to efficiently solve NxNxN Rubik's cubes, including extremely large values of `N`.

## 1. Scope and design principles

The system must:

- support general `NxNxN` Rubik's cubes
- remain efficient for very large `N`
- use a facelet-native representation throughout the representation, simulation, and solver layers
- treat pieces as derived views over facelets when needed
- support both full simulation and heavily optimized execution paths
- allow representation of arbitrary facelet assignments, including unsolvable states

The solver is **not** intended to follow a human, pattern-recognition-based method. Instead, it should primarily rely on general algorithms, especially commutator-based procedures with setup and unsetup moves, because these:

- have limited unintended global effect
- fit efficient simulation well
- apply across a broad range of cases without depending on pattern recognition

Not every solving procedure must be commutator-based. Some cases, such as a parity fix if needed, may instead be handled by a fixed move sequence.

For large cubes, performance is a first-class concern. In particular:

- the center and edge stages are expected to dominate runtime
- commutator-based stages should exploit opportunities for direct in-place updates of only the affected facelets or derived pieces
- optimized hot paths should prefer lookup tables and pre-generated schedules over repeated geometry calculations

When an algorithm only affects a small number of facelets or derived pieces, directly applying its net effect is generally preferred over fully simulating every move in the sequence.

## 2. Core puzzle model

### 2.1 Facelet

A **facelet** represents an individual sticker and has one of six possible values:

- White
- Yellow
- Red
- Orange
- Green
- Blue

### 2.2 Face

A **face** represents one face of the cube as an `N x N` grid of facelets.

A face must also store an orientation value modulo `4`. This orientation is used to represent face turns virtually, so that outer-layer face rotations do not require `O(N^2)` facelet updates.

### 2.3 Cube

A **cube** represents the full puzzle state and contains:

- six faces
- the full facelet state
- a flag indicating whether the state is reachable from the solved state, that is, whether it is solvable

The cube representation must permit any facelet assignment, including states that are not solvable.

## 3. Storage abstractions

The face grid should use a matrix abstraction built on top of an array abstraction.

The array abstraction must support multiple implementations with the same logical behavior but different performance and memory trade-offs. The matrix abstraction should depend only on that shared array abstraction, not on any particular storage implementation.

The required storage implementations are:

- **byte**: one full byte per facelet; wasteful in memory but fast
- **half byte**: one nibble (4 bits) per facelet; half the memory usage, with somewhat slower operations
- **third_byte**: three facelets packed into one byte as a base-6 unsigned integer, since `6^3 < 2^8`; one third of the byte-based memory usage, with slower operations than the half-byte representation

These storage implementations must share a common abstraction with a clean, consistent set of operations.

In optimized hot paths, the implementation should also allow low-level unchecked or unrolled operations where appropriate, such as raw reads and writes that skip bounds checking and facelet-value validation. Where beneficial, this may also include pointer-based or otherwise very low-overhead access techniques.

## 4. Conventions

### 4.1 Facelet and face order

The canonical facelet order is:

1. White
2. Yellow
3. Red
4. Orange
5. Green
6. Blue

The same order is used for faces when White is `U` and Green is `F`.

### 4.2 Standard net and orientation

Face orientation is an integer in `0..3` modulo `4`, increasing clockwise.

Orientation `0` corresponds to the standard net:

```text
  W
O G R B
  Y
```

Turning a face in the positive direction increments its orientation modulo `4`.

### 4.3 Axes

Axes are defined as follows:

- `X` along `R/L`
- `Y` along `U/D`
- `Z` along `F/B`

### 4.4 Depth

Depth ranges from `0` to `N - 1`.

Depth conventions are:

- `X`: `0 = L`, `N - 1 = R`
- `Y`: `0 = D`, `N - 1 = U`
- `Z`: `0 = B`, `N - 1 = F`

### 4.5 Angle

A move angle is one of:

- positive quarter turn
- negative quarter turn
- double turn

Positive means clockwise when viewed along the positive direction of the corresponding right-handed axis. For example, `X / (N - 1) / positive` is an `R` move.

### 4.6 Move counting

For move counting, a **move** is defined as one layer turned by either `90` or `180` degrees along a specific axis.

This counting rule is distinct from the move representation itself, which must still explicitly represent axis, depth, and angle.

## 5. Move and geometry layer

The simulation and geometry layer must include:

- a representation of a move, including axis, depth, and angle
- helpers for cube geometry
- the mapping between physical layers or derived pieces and the facelet-based virtual representation
- logic describing how faces are stored and how layer turns affect them
- logic describing how move effects and algorithm setups relate to cube geometry

The system is facelet-native throughout. Piece concepts are derived only when needed, for example when identifying an edge's home position.

For performance-sensitive code, repeatedly deriving such relationships geometrically at runtime should be avoided where possible. This geometry layer is also the natural basis for generating the precomputed tables and schedules used by the optimized solver.

## 6. Algorithms

Algorithms are reusable solving procedures used by stages. Many of them will be commutator-based, but the abstraction must also allow non-commutator algorithms.

The base abstraction should be called **algorithm**.

A more specialized abstraction that additionally supports direct optimized application should be called **optimized algorithm**.

### 6.1 Required algorithm functionality

An algorithm must support:

- determining whether a given configuration or parameter set is a valid use of that algorithm
- generating the full move sequence for that use, including setup and unsetup
- standard simulation by applying the generated moves one by one

Relevant parameters may include, for example:

- source faces
- target faces
- helper faces
- layer indices
- orientations
- other case-specific geometric inputs

A validity check must determine whether the requested application:

- performs the intended update
- does not disturb unintended facelets or pieces

For example, a center commutator may reject a configuration in which a required helper face conflicts with a source or target face.

The standard simulation path may live in a shared superclass or shared implementation, since it is independent of the specific algorithm and only depends on the generated move sequence.

### 6.2 Optimized algorithm functionality

An optimized algorithm must additionally support direct application of its effect via in-place updates to only the few affected facelets or derived pieces, rather than by simulating every move in the generated sequence.

This direct application is not required for every algorithm. It is expected mainly where it materially improves performance, especially in large-cube stages such as center and edge solving.

Where optimized direct application exists, it must also provide the move count corresponding to the physical move sequence whose net effect is being applied directly.

## 7. Solver architecture

The solver layer must provide a stage abstraction with a clean, consistent interface.

The top-level stages are expected to be:

- center
- edge
- corner

Sub-stages may exist where useful, for example:

- middle center and general center
- middle edge and general edge
- parity handling if needed, such as for a last-edge case

These sub-stages are optional unless the implementation requires them.

Stages should primarily solve their designated portion of the puzzle by invoking reusable algorithms rather than by embedding all logic directly inside the stage implementation, although direct stage-specific logic is still allowed where appropriate.

The center and edge stages are expected to consume most of the runtime on large cubes and therefore deserve the most optimization attention.

Algorithms are used by stages; they are not themselves stages.

### 7.1 Stage responsibilities

A stage may use algorithms, direct logic, or both. A stage may be responsible for:

- validating that the current cube state is appropriate for the stage
- validating that required previous stages are solved
- inspecting or scanning the current state
- deciding what to solve next
- selecting and applying algorithms or other solving procedures
- solving its assigned portion of the puzzle
- recording move count and, where applicable, move history

Precondition and postcondition checking may be skipped in optimized solves for speed, but they must still be part of the stage contract.

## 8. Execution modes

The project supports two named execution modes.

### 8.1 Standard mode

In **standard** mode:

- algorithms are executed through full move-sequence generation and simulation
- full move history is stored
- move count is available
- geometry-based reasoning may be performed directly where needed
- clarity and debuggability may be prioritized over hot-path speed
- this mode is always supported

### 8.2 Optimized mode

In **optimized** mode:

- supported algorithms and stages use direct in-place updates instead of fully simulating all moves
- only move count is stored
- full move history is not stored
- precondition and postcondition checks may be omitted for speed

Optimized mode is especially important for stages such as center and edge solving, where the actual intended effect may only involve a small number of pieces and can therefore be applied in constant time with respect to `N`.

Each stage must explicitly specify whether it supports optimized mode or only standard mode.

## 9. Performance strategy

Because optimized direct updates can be very cheap, the overall performance of large-cube solving depends heavily on the efficiency of scanning, planning, and decision-making.

For hot paths in the optimized solver flow, geometry should generally not be recomputed repeatedly at runtime. Instead, the implementation should prefer lookup tables and pre-generated schedules.

Examples include:

- scan orders for centers or edges, including which ones to inspect and in what order
- helper-slot or helper-face choices used by commutators
- mappings between source, target, helper, and the required setup sequence
- derived edge or stripe relationships, such as which opposite facelet to inspect and whether a layer index must be inverted or rotated
- other repeated geometric decisions that are used during solving

The slower logic used to generate move sequences manually may use clean geometry-based calculations. Likewise, scripts that generate lookup tables may use geometry-heavy code, since they are one-time or offline operations and do not need the same level of optimization.

## 10. Validity and solvability

The cube object must be capable of representing arbitrary facelet assignments, including assignments that are unreachable from the solved state.

The cube should expose a flag indicating whether the current state is solvable or reachable from the solved state.

Solver tests should primarily use scrambled, and therefore solvable, states.

Other tests, such as those comparing the effect of standard and optimized algorithm execution, may also use independently randomized facelet assignments.

## 11. Testing and verification

The project should include detailed tests for the full stack.

### 11.1 Storage and matrix tests

Tests should verify that all array and matrix storage implementations:

- behave correctly on manual examples and edge cases
- agree after long sequences of random updates
- preserve the same logical semantics despite different storage layouts

### 11.2 Move and geometry tests

Tests should verify that move simulation and geometry handling correctly incorporate:

- axis
- depth
- direction and angle
- face orientation behavior

### 11.3 Algorithm equivalence tests

For each optimized algorithm, tests should validate correctness over a wide range of cube sizes, for example `N = 1..7`, and over essentially all relevant configurations or parameter combinations.

At minimum, the following property must hold:

- for every configuration that the validity check labels as valid, the optimized in-place application must have exactly the same effect as fully simulating the generated move sequence

Optionally, tests may also check the converse-style negative property:

- for configurations labeled invalid, the effect of the optimized application differs from the effect of simulating the generated move sequence

### 11.4 Stage tests

Stages should be tested for:

- correct applicability by cube size
- correct handling of their preconditions and postconditions
- correct interaction with preceding solved stages where required
- correctness in both supported execution modes

## 12. Stage contract requirements

Each stage must specify:

- applicable cube sizes
- whether it requires previous stages to already be solved
- its preconditions, at least for standard or checked execution
- its postconditions, at least for standard or checked execution
- whether it supports optimized mode or only standard mode

This is especially important for:

- `N = 1`
- `N = 2`
- `N = 3`
- odd `N`
- even `N`

## 13. Summary of intentional terminology used in this document

For consistency, this document uses the following names:

- **algorithm**: the general abstraction for reusable solving procedures
- **optimized algorithm**: an algorithm that also supports direct in-place application
- **standard mode**: full move-sequence simulation with move history
- **optimized mode**: direct in-place execution where supported, with move count only

These names are chosen only to make the specification internally consistent. They do not prevent later renaming.

## 14. Summary of intended layering

The intended project structure is:

1. **Puzzle state layer**
   - facelet
   - face
   - cube

2. **Storage layer**
   - array abstraction
   - matrix abstraction
   - byte / half-byte / third_byte implementations

3. **Simulation and geometry layer**
   - move representation
   - move simulation
   - geometry helpers

4. **Algorithm layer**
   - reusable commutator-based or fixed-sequence algorithms
   - optional optimized direct-update specializations

5. **Solver layer**
   - stage abstraction
   - center / edge / corner stages
   - optional sub-stages

6. **Performance support layer**
   - lookup tables
   - pre-generated schedules
   - fast-path raw operations

This layering is intended to keep the codebase both clean and efficient, while allowing correctness-first implementations and highly optimized solve paths to coexist.
