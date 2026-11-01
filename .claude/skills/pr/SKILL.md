---
name: pr
description: Commit the current work on a fix/… or feature/… branch and open a PR in BrenOne/DynamicLight (the fork, never upstream mskutin). Use when the user asks to make a PR.
disable-model-invocation: true
---

1. Если ветка `main` — создай `fix/<кратко>` или `feature/<кратко>` (по сути изменений).
2. `cargo fmt --manifest-path src-tauri/Cargo.toml`, затем проверки через агента `rust-checker`. Есть ❌ — исправь и повтори, не коммить сломанное.
3. Коммит: заголовок по-английски в повелительном наклонении (как в `git log`), тело — зачем. Правки только заметок/CLAUDE.md — с `[skip ci]`.
4. `git push -u origin <ветка>`.
5. `gh pr create --repo BrenOne/DynamicLight --base main` — **всегда с `--repo`**, иначе gh может открыть PR в upstream. Описание: что и зачем, как проверить на ленте.
6. Если в CLAUDE.md есть раздел, который изменения делают неверным, — обнови его в том же PR.

$ARGUMENTS
