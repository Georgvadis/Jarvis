<p align="center">
  <img src="docs/cover.svg" alt="Jarvis — кастомная прошивка и веб-панель «Камин и Стекло» для Яндекс Станции Лайт 2" width="100%">
</p>

<h1 align="center">Jarvis</h1>

<p align="center">
  Кастомная прошивка на Rust (<code>jarvisd</code>) и веб-панель <strong>«Камин и Стекло»</strong> на SvelteKit для Яндекс Станции Лайт 2 (<code>arm64</code>).
</p>

<p align="center">
  <a href="https://github.com/Georgvadis/Jarvis/releases/latest"><img src="https://img.shields.io/github/v/release/Georgvadis/Jarvis?style=flat-square&amp;label=release&amp;color=f2994a&amp;labelColor=181310" alt="Последний релиз"></a>
  <a href="https://github.com/Georgvadis/Jarvis/actions/workflows/release.yml"><img src="https://img.shields.io/github/actions/workflow/status/Georgvadis/Jarvis/release.yml?style=flat-square&amp;label=build&amp;labelColor=181310" alt="Сборка релиза"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-f2994a?style=flat-square&amp;labelColor=181310" alt="Лицензия MIT"></a>
</p>

## Возможности

- **Быстрый голосовой пайплайн**: `openWakeWord`, короткий `200 мс` preroll, отклики `wake-replies`, адаптивный VAD (`380 / 850 мс`) со склейкой пауз, прямой `/stt`, выборочный `web_search`, прогрев `codex.warm`, потоковый HTTP/2 TTS (`fish-audio-tts`) и голосовое переключение профилей.
- **Панель «Камин и Стекло» и PWA**: тёмная и светлая темы, Чат 2.0, `Ctrl+K`, Центр здоровья стека, управление LED-кольцом и жесты кнопками (`+ − + − + − + −` — сброс пароля).
- **Студия (`/studio`)**: локальный NotebookLM, читалка книг и ранобэ, голосовые игры, персональный AI-DJ, заметки `CyberNotes`, чеклисты, расписание рутин и таймеры на LED-дисплее.
- **Гибридная музыка и фон**: YouTube Music по умолчанию, тихая Яндекс Музыка без голоса Алисы, офлайн-эмбиент и интернет-радио с быстрым дакингом.
- **Умный дом и интеграции**: Home Assistant, Xiaomi Home, Smart Life, Яндекс Умный дом, Telegram-бот, «Живое присутствие» по Wi-Fi, ПК-компаньон, постоянная память диалога и плагины (Lua 5.4 / Rust).
- **Независимые релизы `arm64`**: собственные OTA из `Georgvadis/Jarvis/releases` с проверкой `SHA256SUMS` и автооткатом через `jarvis-supervisor` (автообновление ночью выключено по умолчанию).

## Сборка и обновления

```sh
VERSION=1.0.0 REPO=Georgvadis/Jarvis STATION_ARCHES=arm64 sh build.sh
```

Новые релизы собираются в GitHub Actions по тегу `v*` ([`.github/workflows/release.yml`](.github/workflows/release.yml)) и ставятся вручную из панели (**Безопасность → Обновления**) либо загрузкой `jarvisd-arm64`.

---

[MIT License](LICENSE) · Основано на [YaJarvis](https://github.com/newfpv/YaJarvis).
