# 01 — Overview

> **Status:** Scope statement only. Substantive content lands in
> commit 2.

## What this chapter answers

- What kind of language is SOL — what shape of program is it good for?
- How does a SOL file end up being executed — what does the language
  hand off to its host runtime, and what does the host hand back?
- What is SOL deliberately *not* trying to be?
- Which parts of the language are stable today and which are still in
  motion?

## Topics covered

1. **Language identity.** Statically typed, eager, single-threaded
   per session, designed for short orchestration programs rather than
   general application code.
2. **Mental model.** A SOL file declares the *contract* a session
   honors: the types it speaks, the helpers it defines, the external
   endpoints it calls, and the entry function the host invokes.
3. **Execution shape.** A host runtime loads the source, the
   compiler lowers it to a bytecode program, and a VM executes the
   bytecode. Side effects happen via `print` and via calls to
   functions declared `ext` (see chapter 12).
4. **What SOL is not.** Not a general-purpose application language;
   no async; no module system beyond `ext` / `export`; no first-class
   functions; no closures; no exceptions.
5. **Stability surface.** Distinguishes *stable now* (parser
   surface, primitive types, control flow) from *in motion* (richer
   diagnostics, source spans, struct/enum stable ordering, library
   API surface).

## Sources to be cited in the substantive pass

- `mod.rs` (public surface)
- `parser.rs` top-level declaration parser (file shape)
- `analyzer.rs` for the rules that make a program "valid"
- `vm.rs` for what "executing a SOL file" actually means
- `reference/SOL_VISUAL_EDITOR_ANALYSIS.md` and
  `reference/SOL_CRATE_IDE_READINESS_PLAN.md` for the broader
  product framing
