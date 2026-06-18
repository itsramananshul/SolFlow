# Internal Notes — Maintainers Only

> **Historical document.** These maintainer notes predate the removal of the
> standalone SOL compiler (the `compiler/` and `runtime/` crates). The
> canonical language now lives in the `sol/` crate (`openprem-sol-v2`); it has
> no type checker and no `E0xxx` or `T90xx` error codes. For the current,
> accurate reference see the rewritten chapters 01 through 23, plus `SPEC.md`,
> `GRAMMAR.md`, and `ERROR_REFERENCE.md`. The text below is kept as a
> historical record.

> **Do not publish this file.** It is excluded by intent from the
> public-facing reading path. The public docs (`README.md` and
> chapters 00 – 19, plus `SPEC.md` / `GRAMMAR.md` /
> `ERROR_REFERENCE.md` / `EXAMPLES.md`) deliberately avoid the
> details below.

This file tracks the maintenance-time context needed to keep the
public docs accurate over time. Editors of the public manual should
read it; nobody else needs to.

## 1. Where the canonical compiler lives

The SOL compiler/runtime is a sibling crate to this repository.
Reading order for documentation work:

| File | Purpose |
|---|---|
| `lexer.rs` | Tokens, keywords, literals, comments |
| `parser.rs` | Grammar; Pratt precedence table around lines 540–558 |
| `analyzer.rs` | Semantic rules; every semantic diagnostic site |
| `bytecode.rs` | Instruction set; emission strategy |
| `vm.rs` | Runtime semantics; every runtime trap |
| `mod.rs` | Library surface |
| `init.rs`, `cli.rs` | Host-runtime load flow (used for chapter 12 only) |

These files were last walked on **2026-05-26**. Any change to the
compiler that affects observable behavior should be reflected in the
public manual *in the same documentation pass*.

## 2. Local mirror of fixtures

The 21 sample `.sol` files mirrored at `reference/sol files/` are a
curated subset of the full test corpus in the compiler crate. The
mirror exists so this repo is self-contained for documentation
purposes. When the upstream corpus changes:

- new positive fixtures relevant to the manual should be added to the
  mirror
- removed fixtures should be removed from the mirror *and* from any
  chapter that cited them

Do not edit mirrored fixtures in this repo — edit upstream and
re-mirror.

## 3. Repository privacy posture

The public-facing documentation must not name the broader product or
expose its architecture. Generic terms used throughout the manual:

| Use this | Not this |
|---|---|
| "controller" / "host runtime" | the specific product or platform name |
| "external function" / "external endpoint" | the product-specific term for a peer call |
| "session configuration" | the product-specific configuration shape |
| "host environment" | the product-specific runtime name |

If a future change requires naming the platform, that decision belongs
to the project lead, not the docs author.

## 4. Snapshot dating for external-runtime sections

Chapter 12 ("Imports, External Functions, and the Host Runtime")
includes snapshot sections describing the wire-format used by one
specific host. Re-date the snapshot every time the snapshot is
re-verified. The current snapshot date is **2026-05-26**.

## 5. Source-citation discipline

Every chapter that lands in commits 2 – 6 must:

- cite at least one source file by name and line range for each
  normative claim
- cite at least one fixture (positive or negative) for each rule
- mark anything not so cited as **Uncertain** and explain what
  evidence would close the gap

Reviewers should reject pull requests that violate this rule.

## 6. Open questions tracker

Live list of open questions appears in
[`00-source-audit.md`, §6](./00-source-audit.md#6-open-questions).
That list is the to-do list for documentation work; close items as
soon as the substantive chapters resolve them.
