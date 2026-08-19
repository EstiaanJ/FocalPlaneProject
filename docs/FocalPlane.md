---
aliases:
  - Focal Plane
  - Project overview
---

# FocalPlane

> [!important] Rewrite
> This project is a rewrite of the predecessor project at `/home/estiaan/code/FocalPlane`. The old Focal Plane editor in [[Folder Structure#OLD_EDITOR|OLD_EDITOR]] is a GUI reference only. I like the GUI as it is, with some exceptions, and keeping it avoids too much rewriting of code. I do not want to carry over the old processing pipeline or its film-photography model.

FocalPlane is a Rust photo-editing suite built primarily for my own workflow and my Fujifilm X-T5. Broader usefulness is welcome but secondary. I eventually want to support photos from my iPhone as well.

I used to love Lightroom 5.5, but I want to make something new rather than just rehash it. I have never had a photography workflow that really worked for me. Darktable and RawTherapee try to do everything for everyone, yet many of their modules feel like they require a degree in colour science. Darktable also feels as though it forces you to use its photo-management system even when you only want to edit one photograph. I do not want that.

I value speed and simplicity despite powerful controls. I want a polished tool with a narrow focus: simple controls for some things and more complex controls where I care about controlling them well.

## Applications and boundaries

The suite separates jobs that other photo applications often combine:

- [[Focal Editor & Focal Core|Focal Editor]] edits and styles photographs. It must work independently of any library.
- [[FocalLib]] will eventually import, organise, judge, and track photographs. It is not the current focus.
- A separate preset editor may eventually contain the nerdy computer-science and colour-science details. It is post-MVP and should not drive current development.

The current experimental applications are intended to discover useful interactions before integration. [[Architecture Decisions|FocalCore is the one production processing architecture]], while FocalCurve and FocalPlot remain independently runnable visual harnesses and sources of reusable GUI widgets.

FocalPlane is not Photoshop, Krita, or GIMP. Its focus is photographic, global adjustments rather than brushes, painting, retouching, masks, or local adjustments.

## Editing workflow

The editor must support both careful work on one photograph and fast work across a group from the same scene. Some photographs may deserve a long individual edit; in other cases I want to correct and process one photograph, then efficiently apply relevant settings to the others.

The editor must not require an import or library step. A user should be able to open and edit one photograph directly.

[[Presets and Saved Edits|Presets, saved edit state, and copied adjustments]] are related but not interchangeable. That distinction is especially important for productive batch work.

## Product values

- Personal needs first.
- Correctness, a responsive UI, and polished tools with a narrow focus.
- Fast, simple editing without hiding useful power.
- Global photographic edits, not brush tools.
- Dense information on desktop without tiny controls that are difficult to reach.
- Keyboard-friendly workflows whose controls remain discoverable.
- A standalone editor which does not require FocalLib.
- No forced catalogue and no single mandatory saved-edit format.
- Rigorous, human-directed software engineering rather than vibe coding.
- A tight, modular, readable, and maintainable codebase with high test coverage.

## GUI principles

Lightroom and the iPhone gallery editor are the closest existing references for the interaction style, although they are proprietary and can only be used as inspiration.

- Never allow the scroll wheel to change a slider value. Scrolling is for scrolling, or for zooming over a photograph view.
- Do not use anaemically thin scroll bars.
- Do not make controls so small that they are difficult to reach with the mouse.
- Information should be dense; controls should have enough room to remain usable.
- Hotkeys matter, especially for rapid judging and editing, but mouse-free use is not a strict requirement.
- Controls and hotkeys must be discoverable.

## Project documentation

- [[README|Documentation home]]
- [[MVP]] — prototype target and features explicitly deferred beyond MVP
- [[Focal Editor & Focal Core]] — editor, processing pipeline, and engineering conventions
- [[Focal Core Pipeline]] — opinionated module order, experimentation, and preview execution
- [[Presets and Saved Edits]] — Save versus Export, presets, edit state, and copying
- [[Sliders]] — current controls and interaction rules
- [[FocalLib]] — later library-management workflow
- [[Testing]] — reference implementation and reproducibility
- [[Engineering Principles]] — correctness, maintainability, testing, and human involvement
- [[Vectorscope Research]] — vectorscope algorithm research and FocalPlane design direction
- [[Folder Structure]] — repository layout, old code, and external references
- [[Open Questions]] — decisions deliberately left unanswered
- [[Architecture Decisions]] — decisions settled by the human owner
- [[Clean Architecture Migration]] — instructions for the next architectural phase
