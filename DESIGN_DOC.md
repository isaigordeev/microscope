# Microscope Design Document

A Rust terminal text editor with vim philosophy, zen-first UI, and all features
built-in. Architecture inspired by Helix, features driven by personal Neovim
workflow.

---

## Table of Contents

1. [Philosophy](#philosophy)
2. [Crate Architecture](#crate-architecture)
3. [Feature Set](#feature-set)
4. [Theme System](#theme-system)
5. [Key Design Decisions](#key-design-decisions)
6. [Milestones](#milestones)

---

## Philosophy

- **Zen-first**: Centered content, minimal chrome. No statusline by default.
  Line numbers on, togglable. Overlays (picker, hover, diagnostics) appear
  temporarily.
- **Single-tab focus**: One document visible at a time. Splits supported but
  not the primary workflow. Switch documents via fuzzy picker.
- **Classic vim grammar**: Verb + motion + text object (`d2w`, `ci"`, `ya{`).
  Full modal editing: Normal, Insert, Visual, Visual-Line, Visual-Block,
  Operator-Pending.
- **Built-in everything**: No plugin system. All features compiled in.
- **Performance**: Rope buffer, async I/O (tokio), incremental tree-sitter,
  dirty-flag rendering.

---

## Crate Architecture

```
microscope/
├── Cargo.toml                  Workspace manifest
├── DESIGN_DOC.md               This document
├── current_tasks/              Active work items (one .txt per task)
├── ideas/                      Future work, prioritized
│
├── ms-core/                    Text editing primitives (no UI dependency)
│   ├── rope.rs                 Ropey re-export + helpers
│   ├── selection.rs            Selection & Range (multi-cursor aware)
│   ├── transaction.rs          Atomic text changes, undo/redo tree
│   ├── register.rs             Vim registers (named, unnamed, clipboard)
│   ├── syntax.rs               Tree-sitter wrapper
│   ├── movement.rs             Cursor motions (word, paragraph, etc.)
│   ├── textobject.rs           Text objects (iw, i", a{, etc.)
│   ├── surround.rs             Surround operations
│   ├── comment.rs              Comment toggle logic
│   ├── search.rs               Search/replace engine
│   └── indent.rs               Indentation detection & smart indent
│
├── ms-view/                    UI abstractions (no terminal dependency)
│   ├── document.rs             Document: text + syntax + LSP state + git state
│   ├── view.rs                 Viewport into a document (scroll, cursor)
│   ├── editor.rs               Global state: documents, views, config, mode
│   ├── tree.rs                 Split layout
│   ├── theme.rs                Theme system: scope → Style resolver
│   ├── themes/
│   │   ├── vs_dark.toml        Built-in dark theme (embedded)
│   │   └── vs_light.toml       Built-in light theme
│   ├── keymap.rs               Vim keymap trie (mode → key → command)
│   ├── mode.rs                 Vim modes
│   └── config.rs               Editor config + project-local config
│
├── ms-term/                    Terminal UI + application (the binary)
│   ├── main.rs                 Entry point
│   ├── application.rs          Event loop (tokio async)
│   ├── compositor.rs           Layered UI component system
│   ├── commands.rs             All editor commands (vim verbs)
│   ├── input.rs                Key event → command dispatch
│   ├── russian.rs              Cyrillic → Latin key remapping
│   └── ui/
│       ├── editor.rs           Main editor view (zen-centered rendering)
│       ├── picker.rs           Fuzzy picker (files, buffers, grep, symbols)
│       ├── filetree.rs         Sidebar file explorer
│       ├── prompt.rs           Command-line input
│       ├── popup.rs            Hover, signature help, blame
│       ├── completion.rs       LSP completion menu
│       └── diff.rs             Diff/history viewer
│
├── ms-tui/                     Terminal rendering abstraction
│   ├── backend.rs              crossterm backend wrapper
│   ├── buffer.rs               Frame buffer (cell grid)
│   ├── terminal.rs             Terminal sizing, drawing, cursor
│   └── style.rs                Color, Modifier, Style types
│
├── ms-lsp/                     Language Server Protocol client
│   ├── client.rs               Per-server connection (JSON-RPC over stdio)
│   ├── transport.rs            Async message framing
│   └── registry.rs             Server discovery & lifecycle
│
├── ms-tree/                    Filesystem tree model
│   ├── tree.rs                 Tree node structure, expand/collapse
│   ├── walk.rs                 Directory walking (.gitignore-aware)
│   └── sort.rs                 Sorting (dirs first, then alpha)
│
├── ms-git/                     Git integration
│   ├── diff.rs                 Diff provider (gutter signs)
│   ├── blame.rs                Git blame (popup + sidebar)
│   └── history.rs              File/line/commit history
│
└── ms-event/                   Decoupled event/hook system
    ├── hook.rs                 Synchronous event dispatch
    └── debounce.rs             Async debounced events
```

### Dependency Graph

```
ms-term ──→ ms-view ──→ ms-core
  │            │
  │            ├──→ ms-lsp
  │            ├──→ ms-git
  │            └──→ ms-event
  │
  ├──→ ms-tui (rendering only)
  └──→ ms-tree (filesystem tree model)
```

### Key Dependencies

- `ropey` — rope data structure
- `tree-sitter` + language grammars — syntax highlighting
- `crossterm` — terminal I/O
- `tokio` — async runtime (LSP, file I/O)
- `gix` (gitoxide) — git operations
- `nucleo` — fuzzy matching for pickers

---

## Feature Set

### Core Editing
- Classic vim modal editing (Normal, Insert, Visual, V-Line, V-Block, Op-Pending)
- Full verb + motion + text object grammar
- Registers (unnamed, named a-z, system clipboard `"+`)
- Undo/redo tree (not linear)
- Macros (`q` record, `@` playback), marks, dot repeat
- Search (`/`, `?`, `n`, `N`) with incremental highlighting
- Replace (`:s/foo/bar/g`)
- Surround operations (`cs"'`, `ds"`, `ysiw"`)
- Comment toggle (`gcc`, `gc` visual)
- 4-space indent (Rust standard), configurable per-project
- Format on save (LSP-based, for rs/go/json)
- No autopairs

### Navigation
- Fuzzy file finder (ripgrep backend)
- Fuzzy buffer/grep/symbols/diagnostics pickers
- Grep in current file, word-under-cursor grep
- LSP go-to (definition, references, implementation, type-def)
- Breadcrumb/hierarchy display (LSP + tree-sitter)
- Jumplist, command/search history, recent files

### UI
- Zen-first: centered content, minimal chrome
- Line numbers (on by default, togglable)
- Scrolloff: 4 lines
- Soft wrap with `↳` indicator, indented continuation
- File tree sidebar (togglable)
- Rainbow delimiters (tree-sitter)
- VS Code-inspired colorschemes (vs_dark, vs_light)
- No statusline by default
- Splits supported

### LSP
- Servers: clangd, pyright, rust-analyzer, gopls, ts_ls
- Auto-discovery via project root markers
- Completion with snippet support
- Hover docs (`K`), signature help, inlay hints
- Diagnostics (undercurl, no gutter signs)

### Tree-sitter
- Syntax highlighting, rainbow delimiters, comment detection
- Parsers: c, cpp, python, rust, go, lua, js, ts, markdown, json, bash

### Git (4 layers)
1. Gutter signs + hunk navigation (`]c`/`[c`) + stage/unstage/reset
2. Blame popup (`<leader>g`)
3. Blame sidebar (`<leader>gb`)
4. Diff/history viewer (file, line, commit browser)

### Misc
- Session save/restore
- Russian keyboard layout (full Cyrillic → Latin remapping)
- Project-local config (`.microscope.toml`)
- Timestamp insertion

---

## Theme System

Three sources of highlight information feed into the theme:

```
Tree-sitter scopes ("@keyword", "@string") ──┐
LSP semantic tokens ("variable.readonly") ────┤→ theme.resolve(scope) → Style
UI elements ("ui.linenr", "diagnostic.error")─┘
```

LSP semantic tokens override tree-sitter where both exist.

Built-in themes (vs_dark, vs_light) are embedded at compile time.
User themes from `~/.config/microscope/themes/` override built-ins.

### vs_dark palette (active default)

| Element         | Color     |
|-----------------|-----------|
| Background      | `#2F2F2F` |
| Text            | `#D4D4D4` |
| Comments        | `#828282` italic |
| Keywords/Types  | `#569CD6` |
| Strings         | `#CE9178` |
| Numbers         | `#B5CEA8` |
| Errors          | `#F44747` |
| Warnings        | `#CCA700` |
| Selection       | `#3A3D41` |
| Search          | `#264F78` |
| Line numbers    | `#6B6B6B` |
| Directories     | `#87CFFF` |
| Popup bg        | `#252526` |
| Popup selected  | `#0078D4` |

### vs_light palette

| Element         | Color     |
|-----------------|-----------|
| Background      | `#F2F2F2` |
| Text            | `#000000` |
| Comments        | `#828282` italic |
| Keywords        | `#0000FF` |
| Strings         | `#A31515` |
| Numbers         | `#098658` |
| Functions/Types | `#0451A5` |
| Selection       | `#ADD6FF` |

---

## Key Design Decisions

- **Rope buffer** (ropey): O(log n) insert/delete, efficient for large files.
  Same approach as Helix.

- **Async event loop** (tokio): Single-threaded rendering with async I/O for
  LSP, git, and file operations. Like Helix's tokio::select! multiplexing.

- **Compositor pattern**: Z-ordered stack of UI layers. Events propagate top
  to bottom. Same as Helix. Allows picker/popup/completion to overlay editor.

- **Trie-based keymaps**: Key sequences stored in a trie for prefix matching.
  Supports vim's multi-key commands (`gg`, `ci"`, `<leader>gs`).

- **SlotMap for documents/views**: Stable IDs, fast iteration, sparse storage.
  Documents and views referenced by ID, not pointer.

- **No plugin system**: All features built-in. Avoids complexity of runtime
  extension loading. Features toggled via config.

- **Dirty-flag rendering**: Only re-render when state changes. Frame buffer
  diffing to minimize terminal output.

- **Strict linting**: Clippy pedantic + nursery, no unsafe, no unwrap/expect,
  no print to stdout/stderr (except main). Matches ruff strict Python style.

---

## Milestones

### M0 — Foundation ✓
Rope buffer + terminal raw mode + basic rendering.
Open a file, display with line numbers, scroll (j/k), quit (q).

### M1 — Vim Modal Editing
Normal/Insert/Visual modes, motions, operators, text objects, undo/redo,
registers, macros, marks, dot repeat.
Ex command line (`:w`, `:q`, `:wq`, `:q!`, `:s///`, `:%s///`, `:e`, `:set`).
Search prompts (`/`, `?`, `n`, `N`, `*`, `#`) with incremental highlight.

### M2 — File Management
Fuzzy file picker, buffer picker, save/open/close buffers.
`ms-tree` crate: filesystem tree model (walk, sort, .gitignore-aware).
File tree sidebar (nvim-tree style): toggle, expand/collapse, open file.

### M3 — Tree-sitter Highlighting
Syntax highlighting, rainbow delimiters, language detection.

### M4 — LSP
Completion, go-to-definition, hover, diagnostics, format on save, inlay hints.

### M5 — Git Integration
Gutter signs, blame popup, blame sidebar, diff/history viewer.

### M6 — Polish
Themes (vs_dark/vs_light), breadcrumbs, Russian keyboard, session management,
project-local config, zen UI refinements.