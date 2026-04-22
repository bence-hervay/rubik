# NxNxN Rubik's Cube Solver - Target Specification

This document outline the target structure and functionality for this project.

The goal is to efficiently solve NxNxN Rubik's cubes, even for extremely large N.

# Concepts/abstractions:

puzzle
- facelet: represents an individual sticker, one of 6 options (white, yellow, ...)
- face: represents a face, square grid of facelets with a orientation mod 4 (to handle face moves virtually)
- cube: represents the whole puzzle, 6 faces, full state

face should use a matrix data structure for the grid of facelets, which should use an array data structure, which will have multiple different implementations, some optimise towards fast update operations, and some for reducing memory usage:
- byte: using a full byte for each facelet (wasteful memory, but fast operations)
- half byte: using a nibble (4 bits, half byte) to store a facelet (half memory, bit slower operations)
- third byte: using a byte to store 3 facelets as a base 6 unsigned integer since 6^3<2^8 (third memory as byte), even slower than half byte
These three should have some common abstraction that they implement of course, with a clean set of operations implemented for each

for simulation
- there should be ways of representing a move (which includes direction/amount, layer/depth, axis)
- there should be a place for helpers to handle the geometry of the puzzle, how and where faces are stored, what data a physical layer or piece of the cube corresponts to virtually.

Solver:
There should be a solver infrastructure, with certain solver stages (common abstraction with clean set of functions that each implements).
I will certainly not implement a solution following a human method, since that relies too much on pattern recognition. Instead, I will use things like commutators to put things in the right place:
- without much global effect (important for efficiency of simulation)
- without reliance on patterns, as commutators with certain setups work in a very general range of cases

The stages will look something like:
- center
- edge
- corner
- these might have sub stages (like middle center and general center, middle edge and general edge, maybe a parity if it ever comes up, e.g. for last edge), but these might not be needed

These stages (face and edge certainly as they take up pretty much all time for large cubes) will generally use certain commutators with some setups. It is important to note that we will heavily use opportunities to optimise simulation with in-place updates of the affected facelets rather than fully simulating each move (as it's O(1) instead of O(n)). For stages that use such commutators.

Algorithms/commutators must have the functionality to:
- check whether a given set of parameters for which the step is requested (e.g. face(s), layer(s), orientations and such) is a valid configuration/case/application for the commutator (e.g. save face for source and target might fail for a center commutator), meaning whether it works for it and:
  - would perform the desired update
  - would not disturb/alter unintended facelets
- for a given application/call/configuration/params, perform the commutator as an direct update (in place updates to the few affected facelets), but this is not required for each, maybe just a subset, so algorithm is the root abstract class which only has the move sequence generation, not the in-place update (e.g. for solving the few corners we might not need such functionality), and have some subclass that requires the optimised/inplace/direct/[some good name] update
- for a given config/... generate the move sequence that the commutator entails, this includes the setup and unsetup of course, everything that would be needed to be moved on a physical cube to perform the desired updates on the small number of stickers/pieces
- actually simulate the moves one by one (this can be in the common superclass as it's algorithm/commutator agnostic).
- these commutators are not stages, but instead they're used by stages
- some might not be commutator-based, perhaps calling these algorithm or something more general would be better (e.g. edge parity is more like a fixed move sequence if it's needed for the solve, but it might not be)
- the correctness of the in place update version should be validated via tests that, for each algorithm, should consider pretty much ALL combinations of inputs/configurations for a wide range of cube sizes (e.g. n from 1 to 7) and validates that:
  - in all cases that the config check labels as a valid usage of this algorithm, the in-place update has exactly the same effect as sully simulating the generated move sequence
  - (optionally) also in all case that the config labels as an invalid (not valid) use, the effect of the in-place differs from simulating the move sequence

The stages (might) use these algorithms to solve what they're responsible for, that includes:
- checking if it's a valid state for this stage (e.g. if some edge stage requires faces to already be solved, it should be able to check that it truly is the case), this might be skipped for live/optimised solves, but is still useful for debugging and performing more inspectable/thoroughly checked solves
- inspecting/scanning a scrambled (or partially scrambled) state of the cube and using the algorithms (or otherwise) to solve the stage while recording the move count (optionally the full move history), either via the full move simulation, or via the inplace operations if it's available

Since the inplace operations are very efficient (only require a couple of instructions per solved sticker), the efficiency of this scanning and planning and decision making will also be very important for overall speed. That's why the geometry handling at the relevant hot paths should preferably not be handled there during the solves, but instead, lookup tables and pre-generated schedules should be used (I mean things like when to scan which face center or edge for misplaced/scrambled/uncomplete facelets/pieces and what place to use as helper slots for commutators if such things are needed). The same applies to the algorithms: things like what setup sequence needs to be applied based on geometry for given combinations of source target helper edge or face etc... For the "slow" part, so the one that performs manual move generation, that can do a clean geometry-based determination of these, but the "fast" part should almost always rely on lookup tables rather then geometry calculations in the hot path of a large puzzle's solving. The scripts to generate these lookup-tables can of course use the geometry and should still be clean, but since it's one-time, they don't have to be highly optimised.

Only store/generate the full move sequence/history in "slow" mode. In "fast" mode, only keep track of the number of moves during a solve/stage/sub-stage (of course that means that the optimised algorithm updates need to compute and provide the move count that would be needed to perform the moves that are not actually physically performed because it's optimised, only their total effect which only modifies a few pieces).

There should also be very detailed tests of pretty much everything else, e.g. for the options for storing arrays/matrices that they work as expected in given edge cases and manual examples, and that they agree after a long sequence of random updates, or that the move simulation and geometry incorporates axis, layer, direction correctly.

conventions:
order of facelets:
- White
- Yellow
- Red
- Orange
- Green
- Blue

(same for the face order if white is U and green is F)
Axes: X/Y/Z for orthogonal (along) RL/UD/FB
Depth: 0 - n-1 :
- X: 0=L, n-1=R
- Y: 0=D, n-1=U
- Z: 0=B, n-1=F
Angle: positive/negative/double (positive is clockwise direction along the right handed axis, so X/n-1/Positive is an R move)
Face orientation is a 0-3 integer (mod4), clockwise (so turning in a positive direction increments the oriengation, but mod4 of course), 0 means the orientation visible in the standard net form:
```
  W
O G R B
  Y
```

Face rotation of virtual to that outer layer moves don't require O(n^2) facelet updates

in the fast case, there should be methods/possibilities to do unrolled/unchecked operations (no check bounds or validity of facelet value, just perform the raw reads and writes, potentially with pointers if it's faster)

For move counting, a move is defined a single layer turned by either 90 or 180 deg (any move angle) along a specific axis, but of course there should be a class-like representation for this.

The representation, simulation, and solver(s) are all facelet-native, and "pieces" are derived views over facelets when needed (e.g. for finding edge home positions), but as explained above, even things like "what position of calculation to use to find the other colour of an edge piece that a side facelet is on" should be handled with lookup tables, at least in fast move (I mean for each of the 12 or 24 edges or edge stripes, what other face to look for and whether the layer has to be inverted, or rotated by 90, etc, but I'm over-specifying this, the point is to use lookup tables with quickly utilisable entries rather than expensive geometric calculations).

For the cube, any assignment of facelets is represented, even ones that are completely unsolvable, but there should be a flag for this in the object (like valid or something) that reflects whether it's reachable from a solved state (or solvable). solvers should be tested on scrambled, so solvable states, while some other things (like effect of slow vs fast move simulation of commutators) can be on independently randomly assigned facelet values.

let's call the previously unnamed algorithms/commutators:
- algorithm for the main abstraction
- optimised algorithm for the one that has directly application as well

Let's call the two modes:
- standard: full move sequence simulation by algorithms (always supported)
- optimised: inline updates of just a couple of facelets

move history should be saved in standard mode, but in optimised, only the move count


Stage contracts

Each stage should specify:

- applicable cube sizes
- preconditions (not in optimised mode)
- postconditions (not in optimised mode)
- whether it requires previous stages solved (kind of precondition)
- whether it supports optimised mode or only standard

This is especially important for N=1, N=2, N=3, odd N, and even N.