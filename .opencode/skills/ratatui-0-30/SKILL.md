---
name: ratatui-0-30
description: Use when writing or changing TUI rendering code in this repo - ui/, panes, draw, widgets, layout, blocks, lists, tables, scroll state. Triggers on - ratatui, crossterm, tui, pane, widget, render, draw, terminal, alternate screen, frame, layout, block, liststate, table.
---

# ratatui 0.30 / crossterm 0.29 notes

Version-specific API facts. Each of these cost a compile cycle or a subtle bug
in this codebase, so check here before guessing.

## `run` does not take a future

```rust
// WRONG - the closure returns the future, so the terminal restores immediately
ratatui::run(|t| async move { /* ... */ });

// RIGHT - manual init/restore around an async loop
let mut terminal = ratatui::init();
let result = app::run(&mut terminal, session).await;
ratatui::restore();
result
```

`ratatui::init()` is not the poor-man's `run`: it enables raw mode, the
alternate screen, **and installs a panic hook that restores the terminal**. That
panic hook is why a crash mid-frame does not leave the user with a broken shell.

`crossterm` is a direct dependency solely to enable the `event-stream` feature.
Import event types from `ratatui::crossterm` rather than `crossterm` directly, so
the two can never resolve to different crate versions. ratatui's backend feature
`crossterm_0_29` is what makes them the same version.

## Rendering needs `&mut` state

`frame.render_stateful_widget` mutates the state (it adjusts scroll offsets), so
the signature is:

```rust
pub fn draw(frame: &mut Frame<'_>, state: &mut AppState) { .. }
```

Therefore `ListState` lives in `AppState` and is threaded through by `&mut`.
**Do not construct a fresh `ListState` each frame** — ratatui's offset
adjustments are lost, and the selection visibly jitters when you page through a
long list. Persist it; let ratatui own the offset.

## Layout destructures

```rust
let [body, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);
```

`.areas()` returns `[Rect; N]` and destructures cleanly. `Layout::split()` also
exists. Remember the destructured value is an **array, not a tuple** — mixing
the two up is an easy compile error.

Constraints: `Length(n)`, `Percentage(n)`, `Min(n)`, `Max(n)`, `Fill(n)`,
`Ratio(n, d)`. `Fill` is the right default for even splits; `Length` for fixed
chrome like a one-line status bar.

## Borders and text

- `Block::bordered()` is shorthand for `Block::default().borders(Borders::ALL)`.
- `highlight_symbol("")` still **reserves a column** for the selected row, so
  leave that in mind when computing inner widths for content.
- `Span::styled(text, Color)` works because `Color: Into<Style>`, but a variable
  that is sometimes a `Color` and sometimes a `Style` will not compile in an
  `if`/`else`. Return `Color` from the branch and apply `Style::default().fg()`
  outside it.
- `Line::from(line)` where `line` is already a `Line` is a clippy
  `useless_conversion`.
- `.bold()` / `.italic()` on `Style` are inherent methods, so `Stylize` does not
  need importing (and an unused import is a warning, which is a failure here).

## Event loop shape

`EventStream` needs the `event-stream` feature and a tokio reactor, so the loop
lives inside the async main. Merge three sources:

```rust
tokio::select! {
    event = stream.next() => { .. }        // key, resize
    event = worker_rx.recv() => { .. }     // results from spawned tasks
    _ = tokio::time::sleep(TICK_RATE) => Tick,
}
```

A fresh `sleep` per iteration is the idiomatic pattern here — no `interval`
missed-tick bookkeeping needed. Draw **before** awaiting the next event, so the
UI is always showing the result of the previous one.

`KeyEventKind::Release` must be filtered explicitly or key-repeat shows up as
double input.

## Testing renders

`TestBackend` needs no terminal and no real size:

```rust
let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
terminal.draw(|frame| draw(frame, &mut state))?;
let buffer = terminal.backend().buffer().clone();
// buffer[(x, y)].symbol()
```

Render into a `String` grid and assert on substrings. Always include a
1x1 and a wide-terminal case; the 0-width paths in this codebase are real and
`Rect` arithmetic will happily underflow without them.
