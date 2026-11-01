---
name: release
description: Bump the fork version in every file and prepare the vX.Y.Z tag. Use only when the user asks for a release.
disable-model-invocation: true
argument-hint: vX.Y.Z
---

Версия: $ARGUMENTS (формат `vX.Y.Z`, ≥ 1.0.0).

1. Подними версию во всех файлах — она должна совпадать везде:
   `package.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` (пакет `dynamiclight`), `control/Cargo.toml`, `control/Cargo.lock`, `src-tauri/tauri.conf.json`.
   В lock-файлах правь только запись своего пакета (grep -n по имени), не читай файл целиком.
2. `node scripts/release.cjs version $ARGUMENTS` — должно пройти без ошибок.
3. Проверки через агента `rust-checker`.
4. Коммит `Release $ARGUMENTS`, PR по скиллу `/pr`.
5. После merge в main — **спроси пользователя**, потом: `git checkout main && git pull && git tag $ARGUMENTS && git push origin $ARGUMENTS`. Джоб `release` сам подпишет и опубликует Release.
