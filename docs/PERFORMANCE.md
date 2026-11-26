# Performance

## Hook path

Measured on an Apple-silicon macOS machine with a warm release binary, 300
invocations per command, state file present, using Python `subprocess` timing:

| Command | Mean | p50 | p95 |
|---|---:|---:|---:|
| `pretool` (allow path) | 2.19 ms | 2.08 ms | 2.98 ms |
| `prompt` | 2.07 ms | 2.06 ms | 2.21 ms |
| `statusline` | 2.29 ms | 2.25 ms | 2.76 ms |

Process creation accounts for roughly 1.6 ms of each figure; the hook logic and
state read are a small remainder. Measurements vary with machine load.

## Rendering

The status line is written into one pre-sized `String`:

- one allocation for the whole frame;
- no per-line, per-span or per-bar temporary strings;
- `Pct`, `Ratio` and `Duration` implement `Display`, so numbers are formatted
  directly into the frame;
- ANSI prefixes are static strings selected once per accent;
- the terminal width selects the bar cell count, capped at 28 cells.

The interactive panel redraws only on key events, not on a timer.

## Hook logic

- Hooks construct `Config::for_hooks()`, which skips colour, terminal and
  bottom-margin environment reads.
- The weekly pace is computed once when no borrow credit is active.
- The allow path allocates nothing beyond environment/path parsing and returns
  an enum without a message.
- The release profile uses `opt-level = 3`, fat LTO, one codegen unit, symbol
  stripping, and `panic = "abort"`.

`opt-level = 3` and `z` were compared in an interleaved A/B run; the difference
was within measurement noise, and `3` is kept for the hot path and the larger
UI code. The binary is approximately 557 KB.

## Reproducing

```bash
cargo build --release
# time repeated invocations of:
./target/release/usage-pacer pretool
./target/release/usage-pacer statusline
```
