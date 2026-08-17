# Rust Coding Guidelines
- Do NOT add dependencies unless explicitly told to do so.
- Keep changes minimal, and focus on maintainability.
- Implement exactly what is asked. No extra commands, options, error paths or
  helpers for what might be needed later.
- Most code speaks for itself, do not add comments unless the code itself is unclear.
- Never use em-dashes (—) in comments.
- `cargo fmt` and `cargo clippy` are not installed, so write rustfmt style by
  hand and check the work with `cargo test`.
