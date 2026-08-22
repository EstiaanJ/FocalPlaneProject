---
aliases:
  - Focal Lib
  - Photo library
  - Library manager
tags:
  - authorship/mixed
  - audience/human
  - audience/agents
---

# FocalLib

> [!note] Current scope
> FocalLib is not what we are building now. Current development should focus on [[Focal Editor & Focal Core|Focal Editor]]. This note records the intended workflow so the editor does not grow into the library manager by accident.

FocalLib will eventually manage camera import, storage, judging, and the relationship between photographs and the database. Focal Editor must remain independently usable.

## Intended import and judging workflow

1. Insert an SD card into the computer.
2. Launch FocalLib and press Import.
3. Import only new photographs which do not already exist in the library.
4. Optionally clear the memory card, but only after strictly verifying that an uncorrupted RAW copy exists in the library.
5. Show newly imported photographs, along with any previously imported photographs which have not yet been judged.
6. Let the user check blur and other reasons to discard a photograph.
7. Give each photograph one of exactly three workflow outcomes: **Reject**, **Archive**, or **Edit**.

Machine-vision tools may assist with judging, but the user makes the final decision.

Rejected photographs should not be imported again merely because they remain on the SD card. Keep rejected photographs for roughly 30 days before permanent removal.

Five-star ratings, flags, and other elaborate classification systems are not part of the intended in-app workflow. They may eventually be supported for metadata interoperability, but FocalPlane itself has three choices: Reject, Archive, and Edit.

RAW photographs will eventually live in a single managed folder, with the database tracking identity when files are renamed. Edit state may be stored in that database, and a metadata UUID may serve as a backup reference.

This is an incomplete future workflow, not an implementation specification.
