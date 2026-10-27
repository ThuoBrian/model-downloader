## What this changes

<!-- One or two sentences. Link the issue if there is one: Closes #123 -->

## Why

## How I tested it

<!-- What you ran and what you saw. For download changes, say which model and drive you tried. -->

## Checklist

- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo test` passes
- [ ] README updated if usage, models or sizes changed
- [ ] If the model catalog changed: checksums and sizes come from the Hugging Face or Ollama listing, and the app still picks a sensible default for each memory tier
