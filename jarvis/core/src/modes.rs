//! Общие интерактивные режимы (Переводчик, RPG-квест, Викторина), доступные из голоса и веб-панели.

use crate::ambient::Ambient;
use crate::llm::Turn;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SpecialMode {
    #[default]
    Normal,
    Chat,
    Translator(String),
    Rpg(String),
    Quiz(String),
    Custom { title: String, prompt: String },
    Tutor { course_id: String, lesson_idx: usize, title: String, prompt: String, bilingual: bool },
    Melody,
    Detective,
    PartyHost,
}

impl SpecialMode {
    pub fn is_game(&self) -> bool {
        matches!(self, SpecialMode::Melody | SpecialMode::Detective | SpecialMode::PartyHost)
    }
}

#[derive(Default)]
struct Inner {
    mode: SpecialMode,
    history: Vec<Turn>,
    started_ambient: bool,
}

pub struct ModeManager {
    inner: Mutex<Inner>,
}

impl ModeManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
        })
    }

    pub fn current(&self) -> SpecialMode {
        self.inner.lock().unwrap().mode.clone()
    }

    pub fn status(&self) -> Value {
        let mode = self.current();
        match mode {
            SpecialMode::Normal => json!({"mode": "normal", "lang": "", "topic": ""}),
            SpecialMode::Chat => json!({"mode": "chat", "lang": "", "topic": ""}),
            SpecialMode::Rpg(topic) => json!({"mode": "rpg", "lang": "", "topic": topic}),
            SpecialMode::Quiz(topic) => json!({"mode": "quiz", "lang": "", "topic": topic}),
            SpecialMode::Custom { title, .. } => json!({"mode": "custom", "lang": "", "topic": title}),
            SpecialMode::Tutor { course_id, lesson_idx, title, bilingual, .. } => json!({
                "mode": "tutor",
                "lang": "",
                "topic": title,
                "course_id": course_id,
                "lesson_idx": lesson_idx,
                "bilingual": bilingual,
            }),
            SpecialMode::Translator(lang) => json!({"mode": "translator", "lang": lang, "topic": ""}),
            SpecialMode::Melody => json!({"mode": "melody", "lang": "", "topic": ""}),
            SpecialMode::Detective => json!({"mode": "detective", "lang": "", "topic": ""}),
            SpecialMode::PartyHost => json!({"mode": "party", "lang": "", "topic": ""}),
        }
    }

    pub fn set_mode(&self, mode: SpecialMode, ambient: Option<&Ambient>) {
        let prev_started = {
            let mut g = self.inner.lock().unwrap();
            let prev_started = g.started_ambient;
            g.mode = mode.clone();
            g.history.clear();
            g.started_ambient = false;
            prev_started
        };
        if prev_started && !mode.is_game() && let Some(amb) = ambient {
            amb.stop();
        }
    }

    pub fn reset(&self, ambient: Option<&Ambient>) -> bool {
        let (prev, stop_amb) = {
            let mut g = self.inner.lock().unwrap();
            let prev = std::mem::replace(&mut g.mode, SpecialMode::Normal);
            g.history.clear();
            let stop_amb = matches!(prev, SpecialMode::Rpg(_) | SpecialMode::Quiz(_) | SpecialMode::Custom { .. }) && g.started_ambient;
            g.started_ambient = false;
            (prev, stop_amb)
        };
        if stop_amb && let Some(amb) = ambient {
            amb.stop();
        }
        prev != SpecialMode::Normal
    }

    pub fn activate(&self, mode: SpecialMode, ambient: Option<&Ambient>) -> String {
        let prev_started = {
            let g = self.inner.lock().unwrap();
            g.started_ambient
        };
        let mut started_amb = false;
        if let Some(amb) = ambient {
            match &mode {
                SpecialMode::Rpg(_) => {
                    if amb.current_id().is_none() || prev_started {
                        started_amb = amb.start("fireplace").is_ok();
                    }
                }
                SpecialMode::Quiz(_) | SpecialMode::Melody | SpecialMode::PartyHost | SpecialMode::Custom { .. } => {
                    if amb.current_id().is_none() || prev_started {
                        started_amb = amb.start("chill").is_ok();
                    }
                }
                SpecialMode::Detective => {
                    if amb.current_id().is_none() || prev_started {
                        started_amb = amb.start("jazz").is_ok();
                    }
                }
                SpecialMode::Normal | SpecialMode::Chat | SpecialMode::Translator(_) | SpecialMode::Tutor { .. } => {
                    if prev_started {
                        amb.stop();
                    }
                }
            }
        }
        let greeting = match &mode {
            SpecialMode::Normal => "Режим выключен, возвращаюсь к обычному общению.".into(),
            SpecialMode::Chat => "Давай поболтаем! Я тебя внимательно слушаю — просто говори, я не буду перебивать и отключаться.".into(),
            SpecialMode::Translator(lang) => {
                format!("Режим переводчика на {lang} включён. Говорите фразу — я сразу переведу.")
            }
            SpecialMode::Rpg(topic) if !topic.trim().is_empty() => format!(
                "[sfx:door] RPG-квест «{}» запущен! Готов начать приключение — скажи «поехали» или опиши своего героя.",
                topic.trim()
            ),
            SpecialMode::Rpg(_) => "[sfx:door] Квест начался! Ты стоишь у входа в старую таверну на краю тёмного леса, внутри трещит камин, а у двери прибита записка с твоим именем. Зайдёшь внутрь или сначала прочтёшь записку?".into(),
            SpecialMode::Quiz(topic) if !topic.trim().is_empty() => format!(
                "[sfx:chime] Викторина на тему «{}» запущена! Скажи «поехали» — и задам первый вопрос.",
                topic.trim()
            ),
            SpecialMode::Quiz(_) => "[sfx:chime] Викторина запущена! Назови любую тему или скажи «обо всём», и поехали к первому вопросу.".into(),
            SpecialMode::Custom { title, .. } => format!(
                "[sfx:chime] Игра «{}» запущена! Скажи «начинаем», и поехали.",
                if title.trim().is_empty() { "Свой сценарий" } else { title.trim() }
            ),
            SpecialMode::Tutor { title, lesson_idx, .. } => format!(
                "[sfx:chime] Начинаем урок {}: «{}». Готов к разминке?",
                lesson_idx + 1,
                title.trim()
            ),
            SpecialMode::Melody => "Музыкальная игра «Угадай мелодию» запущена!".into(),
            SpecialMode::Detective => "[sfx:thunder] Нуарный детектив запущен!".into(),
            SpecialMode::PartyHost => "Режим ведущего игры запущен!".into(),
        };
        {
            let mut g = self.inner.lock().unwrap();
            g.mode = mode;
            g.history.clear();
            g.started_ambient = started_amb;
        }
        greeting
    }

    pub fn history(&self) -> Vec<Turn> {
        self.inner.lock().unwrap().history.clone()
    }

    pub fn push_turn(&self, user: &str, assistant: &str) {
        let mut g = self.inner.lock().unwrap();
        if g.mode == SpecialMode::Normal {
            return;
        }
        g.history.push(Turn {
            role: "user",
            content: user.to_string(),
        });
        g.history.push(Turn {
            role: "assistant",
            content: assistant.to_string(),
        });
        const MAX_ITEMS: usize = 40; // 20 пар реплик
        if g.history.len() > MAX_ITEMS {
            let excess = g.history.len() - MAX_ITEMS;
            g.history.drain(..excess);
        }
    }

    pub fn try_command(&self, command: &str, ambient: Option<&Ambient>) -> Option<String> {
        let t = command.trim().to_lowercase();
        let wants_off = t.contains("выключи")
            || t.contains("закончи")
            || t.contains("выйди")
            || t.contains("хватит")
            || t.contains("останови")
            || t.contains("стоп");
        if wants_off
            && (t.contains("перевод")
                || t.contains("квест")
                || t.contains("рпг")
                || t.contains("викторин")
                || t.contains("квиз")
                || t.contains("игр")
                || t.contains("урок")
                || t.contains("курс")
                || t.contains("блиц")
                || t.contains("обучен")
                || t.contains("репетитор")
                || t.contains("режим"))
        {
            if self.reset(ambient) {
                return Some("Режим выключен, возвращаюсь к обычному общению.".into());
            }
            return None;
        }

        if t.contains("переводчик")
            && (t.contains("включи")
                || t.contains("режим")
                || t.contains("запусти")
                || t.contains("поставь"))
        {
            let langs = [
                ("английск", "английский"),
                ("испанск", "испанский"),
                ("немецк", "немецкий"),
                ("французск", "французский"),
                ("итальянск", "итальянский"),
                ("японск", "японский"),
                ("китайск", "китайский"),
                ("корейск", "корейский"),
                ("турецк", "турецкий"),
                ("польск", "польский"),
                ("португальск", "португальский"),
            ];
            let lang = langs
                .iter()
                .find(|(stem, _)| t.contains(stem))
                .map(|(_, full)| *full)
                .unwrap_or("английский");
            return Some(self.activate(SpecialMode::Translator(lang.into()), ambient));
        }

        if (t.contains("квест") || t.contains("рпг"))
            && (t.contains("запусти")
                || t.contains("сыграем")
                || t.contains("включи")
                || t.contains("начни")
                || t.contains("поиграем"))
        {
            return Some(self.activate(SpecialMode::Rpg(String::new()), ambient));
        }

        if t.contains("давай поболтаем")
            || t.contains("поговори со мной")
            || t.contains("режим диалога")
            || t.contains("свободный диалог")
            || t == "поболтаем"
        {
            return Some(self.activate(SpecialMode::Chat, ambient));
        }

        if (t.contains("викторин") || t.contains("квиз"))
            && (t.contains("запусти")
                || t.contains("сыграем")
                || t.contains("включи")
                || t.contains("начни")
                || t.contains("поиграем"))
        {
            return Some(self.activate(SpecialMode::Quiz(String::new()), ambient));
        }

        None
    }

    pub fn is_bedtime_command(command: &str) -> bool {
        let low = command.trim().to_lowercase();
        (low.contains("сказк") || low.contains("истори"))
            && (low.contains("на ночь") || low.contains("перед сном"))
    }

    pub fn build_prompt(
        mode: &SpecialMode,
        base: &str,
        rules: &str,
        extra: &str,
        whisper_hint: &str,
    ) -> String {
        match mode {
            SpecialMode::Normal => format!("{base}\n{rules}\n{extra}{whisper_hint}"),
            SpecialMode::Chat => format!(
                "{base}\n{rules}\n{extra}\n[Режим «Свободный диалог»: вы с хозяином просто общаетесь как близкие собеседники. Отвечай живо, тепло и естественно, можно задавать лёгкие встречные вопросы по теме, и при просьбе свободно используй любые инструменты колонки.]{whisper_hint}"
            ),
            SpecialMode::Translator(lang) => format!(
                "Ты синхронный голосовой переводчик между русским и языком «{lang}». Если фраза на русском — переведи на «{lang}». Если фраза на «{lang}» — переведи на русский. Отвечай ТОЛЬКО готовым переводом, без кавычек, пояснений и лишних слов."
            ),
            SpecialMode::Rpg(topic) => {
                let setting = if topic.trim().is_empty() {
                    "Тёмное фэнтези".to_string()
                } else {
                    topic.trim().to_string()
                };
                format!(
                    "Ты мастер атмосферного голосового RPG-квеста в сеттинге «{setting}». Веди захватывающее приключение в 2–4 коротких предложения. Когда говорит NPC, пиши с новой строки в формате [Имя NPC]: Реплика. По ситуации добавляй короткие эффекты [sfx:door], [sfx:thunder], [sfx:magic], [sfx:coin], [sfx:bell] или смену обстановки [scene:tavern|forest|storm|ocean]. В конце дай 2–3 ярких выбора действий, без Markdown и списков.{whisper_hint}"
                )
            }
            SpecialMode::Quiz(topic) => {
                let theme_line = if topic.trim().is_empty() {
                    "Если тема ещё не выбрана — спроси или начни с интересного вопроса обо всём.".to_string()
                } else {
                    format!("Тема викторины: «{}». Все вопросы строго по этой теме!", topic.trim())
                };
                format!(
                    "Ты бодрый ведущий голосовой викторины. {theme_line} Задавай интересные короткие вопросы (по одному за раз), проверяй ответ игрока, веди счёт и подбадривай. Отвечай живо в 2 предложения (можешь вставлять [sfx:fanfare] за верный ответ и [sfx:buzz] за ошибку), без Markdown.{whisper_hint}"
                )
            }
            SpecialMode::Custom { title, prompt } => format!(
                "Ты ведущий голосовой игры «{title}». Правила и сеттинг: {prompt}. Отвечай живо, коротко (2–4 предложения), без Markdown и списков, вовлекая игрока.{whisper_hint}"
            ),
            SpecialMode::Tutor { prompt, .. } => format!("{prompt}{whisper_hint}"),
            SpecialMode::Melody | SpecialMode::Detective | SpecialMode::PartyHost => base.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_commands_and_history_work() {
        let m = ModeManager::new();
        assert_eq!(m.current(), SpecialMode::Normal);
        assert!(m.try_command("запусти рпг квест", None).is_some());
        assert_eq!(m.current(), SpecialMode::Rpg(String::new()));
        m.push_turn("иду в таверну", "дверь скрипит");
        assert_eq!(m.history().len(), 2);
        assert!(m.try_command("включи переводчик на немецкий", None).is_some());
        assert_eq!(m.current(), SpecialMode::Translator("немецкий".into()));
        assert!(m.history().is_empty());
        assert!(m.try_command("выключи режим", None).is_some());
        assert_eq!(m.current(), SpecialMode::Normal);
        assert!(ModeManager::is_bedtime_command("расскажи сказку на ночь про звёзды"));
        assert!(!ModeManager::is_bedtime_command("расскажи сказку"));
    }
}
