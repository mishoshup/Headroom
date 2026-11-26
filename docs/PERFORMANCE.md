# Performance

## Hook path

Reference measurement on an Apple-silicon macOS machine with a warm release
binary, state file present, 100–200 invocations per command using Python
`subprocess` timing. System load average was 3.76 during the run; `/usr/bin/true`
measured p50 1.67 ms in the same loop.

| Command | Mean | p50 | p95 |
|---|---:|---:|---:|
| `pretool` (allow path) | 2.26 ms | 2.16 ms | 2.63 ms |
| `prompt` | 2.25 ms | 2.22 ms | 2.46 ms |
| `statusline` | 2.47 ms | 2.41 ms | 3.12 ms |

Process creation is about 1.7 ms of each figure. Hook logic and state read add
roughly 0.5–0.75 ms. Measurements vary with machine load; earlier quiet-machine
runs measured approximately 2.1–2.3 ms per command.

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
