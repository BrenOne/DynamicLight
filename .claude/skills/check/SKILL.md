---
name: check
description: Run the CI checks locally (fmt, tests, clippy) and report only failures. Use before committing or opening a PR.
context: fork
agent: rust-checker
---

Прогони проверки CI для текущего рабочего дерева и верни краткий отчёт. $ARGUMENTS
