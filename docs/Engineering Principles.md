---
aliases:
  - Engineering standard
  - Development principles
  - Software quality
tags:
  - authorship/mixed
  - audience/agents
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

Test pure processing independently of the GUI, important boundaries through integration or contract tests, and asynchronous state transitions deterministically. Use controlled fixtures or golden outputs where they carry a meaningful correctness signal, retain focused regression tests for defects, and include manual checks where visible behaviour cannot be judged adequately from numbers alone. [[Testing]] contains the detailed strategy and review checklist.

High coverage should help find untested decisions, edge cases, and failure paths; it is not a reason to write shallow tests which merely execute lines.

## GUI technology

Use egui and eframe for FocalPlane desktop GUIs, including the exposure-curve sub-project. Keep image-processing and curve-evaluation logic independent of egui so it remains directly testable and reusable by Focal Core.

## Related documentation

Use the [[README|documentation home]] for the maintained index. [[Testing]] contains the detailed quality strategy.
