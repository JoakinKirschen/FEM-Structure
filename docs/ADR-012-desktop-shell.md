# ADR-012: Desktop shell boundary

## Status
Accepted for Pass 12.

## Context
The platform needs an engineer-facing desktop application without coupling UI
state, WPF controls, or operating-system process management to the structural
domain and solver crates.

## Decision
Use a C#/.NET 8 solution with a platform-neutral `Structural.Desktop.Core` and a
thin Windows WPF application.

The shell:
- reads existing JSON artifacts but does not reinterpret engineering values;
- represents navigation as a generic deterministic model tree;
- stores provisional UI assignments in a separate versioned sidecar;
- applies edits through undoable commands;
- exposes actions through a command registry used by the command palette;
- executes Rust CLI operations as cancellable background processes;
- caps captured process output;
- uses built-in `Viewport3D` to avoid committing the platform to a rendering vendor.

WPF-specific types must not appear in `Structural.Desktop.Core`.

## Consequences
The core can be tested on non-Windows hosts. The first viewport is intentionally a
shell and does not yet render authoritative geometry. A later automation API can
replace process invocation and sidecar assignments without replacing the desktop
interaction model.
