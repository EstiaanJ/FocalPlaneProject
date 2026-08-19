---
aliases:
  - Engineering standard
  - Development principles
  - Software quality
---

# Engineering Principles

This project uses LLMs for much of the coding, but it must not become a vibe-coded project. Use rigorous software engineering, testing, and review. The human owner remains heavily involved in product and design decisions.

## Priorities

- Correctness is highly valued.
- Maintainability and readability matter as much as making the current feature work.
- Keep the codebase tight. Avoid speculative abstractions, duplicated mechanisms, unnecessary dependencies, and features without a concrete requirement.
- Prefer modular code with explicit responsibilities and well-defined interfaces.
- Aim for high test coverage, with tests chosen for meaningful behaviour rather than coverage numbers alone.
- Make processing semantics and important invariants explicit.
- Optimisation must not silently weaken correctness.

## Human involvement

Agents must not make significant product, interaction, image-processing, colour-science, or architectural decisions silently. When implementation exposes a meaningful choice which is not settled in the documentation, present the options and trade-offs to the human owner.

Routine implementation details may be decided autonomously when they preserve documented behaviour and do not constrain future product decisions. Record non-obvious technical choices close to the code or in the relevant documentation.

Human observation is one of the project's testing vectors, especially for GUI interaction, perceived image quality, preview accuracy, and photographic usefulness. Automated tests do not replace human visual evaluation; human approval does not replace automated correctness tests.

## Testing expectations

- Test pure processing code independently of the GUI.
- Use unit tests for algorithms, invariants, parameter boundaries, and error handling.
- Use integration and contract tests at module and application boundaries.
- Use controlled fixtures and golden outputs where they provide a meaningful correctness signal.
- Test cancellation, stale-result rejection, and asynchronous state transitions rather than relying only on happy-path rendering.
- Keep tests deterministic where practical. Document intentional tolerances and their justification.
- Treat bugs as opportunities to add focused regression tests.
- Include manual visual and interaction checks for behaviour which cannot be judged adequately from numbers alone.

High coverage is a goal, not permission to write shallow tests which merely execute lines. Coverage should help us find untested decisions, edge cases, and failure paths.

## GUI technology

Use egui and eframe for FocalPlane desktop GUIs, including the exposure-curve sub-project. Keep image-processing and curve-evaluation logic independent of egui so it remains directly testable and reusable by Focal Core.

## Related documentation

- [[Testing]] — test strategy and reproducibility
- [[Focal Editor & Focal Core]] — GUI and processing boundary
- [[Focal Core Pipeline]] — modular processing architecture
