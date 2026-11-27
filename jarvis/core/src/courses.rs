//! Курсы и голосовой репетитор (программа уроков, интервальное повторение, прогресс и конспекты).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Lesson {
    pub title: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub key_points: Vec<String>,
    #[serde(default = "default_status")]
    pub status: String, // "todo" | "in_progress" | "done"
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub completed_at: u64,
}

fn default_status() -> String {
    "todo".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewCard {
    pub front: String,
    pub back: String,
    #[serde(default = "default_interval")]
    pub interval_days: u32,
    #[serde(default)]
    pub next_review: u64,
    #[serde(default)]
    pub hits: u32,
}

fn default_interval() -> u32 {
    1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Course {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub level: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub bilingual: bool,
    #[serde(default)]
    pub current_lesson: usize,
    #[serde(default)]
    pub lessons: Vec<Lesson>,
    #[serde(default)]
    pub cards: Vec<ReviewCard>,
    #[serde(default)]
    pub weak_spots: Vec<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedCourses {
    #[serde(default)]
    courses: Vec<Course>,
}

pub struct CourseStore {
    path: PathBuf,
    courses: Mutex<Vec<Course>>,
}

impl CourseStore {
    pub fn load(data_dir: PathBuf) -> Arc<Self> {
        let path = data_dir.join("courses.json");
        let courses = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PersistedCourses>(&bytes).ok())
            .map(|p| p.courses)
            .unwrap_or_default();
        Arc::new(Self {
            path,
            courses: Mutex::new(courses),
        })
    }

    fn save(&self, list: &[Course]) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let data = PersistedCourses {
            courses: list.to_vec(),
        };
        if let Ok(bytes) = serde_json::to_vec_pretty(&data) {
            let _ = jarvis_station::atomic(&self.path, &bytes);
        }
    }

    pub fn list_json(&self) -> Value {
        let g = self.courses.lock().unwrap();
        json!({ "courses": *g, "now": now_ts() })
    }

    pub fn get(&self, id: &str) -> Option<Course> {
        self.courses.lock().unwrap().iter().find(|c| c.id == id).cloned()
    }

    pub fn find_by_id_or_name(&self, query: &str) -> Option<Course> {
        let g = self.courses.lock().unwrap();
        if g.is_empty() {
            return None;
        }
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return g.first().cloned();
        }
        if let Some(c) = g.iter().find(|c| c.id.eq_ignore_ascii_case(&q)) {
            return Some(c.clone());
        }
        if let Some(c) = g.iter().find(|c| {
            c.title.to_lowercase().contains(&q) || c.subject.to_lowercase().contains(&q)
        }) {
            return Some(c.clone());
        }
        g.first().cloned()
    }

    pub fn match_voice_start(&self, text: &str) -> Option<(Course, usize, bool)> {
        let low = text.trim().to_lowercase();
        let is_blitz = low.contains("блиц") && (low.contains("карточ") || low.contains("слов") || low.contains("экзамен") || low.contains("курс"));
        let is_lesson = low.contains("продолжим урок")
            || low.contains("начнём урок")
            || low.contains("начнем урок")
            || low.contains("запусти урок")
            || low.contains("урок английск")
            || low.contains("урок японск")
            || low.contains("урок немецк")
            || low.contains("вернёмся к") && low.contains("урок")
            || low.contains("вернемся к") && low.contains("урок");
        if !is_blitz && !is_lesson {
            return None;
        }
        let g = self.courses.lock().unwrap();
        if g.is_empty() {
            return None;
        }
        // Ищем курс по ключевому слову из речи или берём первый активный
        let course = g
            .iter()
            .find(|c| {
                let subj = c.subject.to_lowercase();
                let title = c.title.to_lowercase();
                subj.split_whitespace().any(|w| w.chars().count() >= 4 && low.contains(w))
                    || title.split_whitespace().any(|w| w.chars().count() >= 4 && low.contains(w))
            })
            .or_else(|| g.first())?
            .clone();
        // Если назвали конкретный номер урока (1..20)
        let explicit_num = low
            .split(|c: char| !c.is_ascii_digit())
            .filter_map(|s| s.parse::<usize>().ok())
            .find(|&n| (1..=20).contains(&n));
        let idx = explicit_num
            .map(|n| (n - 1).min(course.lessons.len().saturating_sub(1)))
            .unwrap_or(course.current_lesson);
        Some((course, idx, is_blitz))
    }

    pub fn build_blitz_prompt(course: &Course) -> String {
        let list: Vec<String> = course
            .cards
            .iter()
            .take(15)
            .map(|c| format!("«{}» = {}", c.front, c.back))
            .collect();
        let cards_text = if list.is_empty() {
            format!("Придумывай короткие проверочные вопросы по курсу «{}» ({})", course.title, course.level)
        } else {
            format!("Карточки для проверки (спрашивай по одной вразброс): {}", list.join("; "))
        };
        format!(
            "Ты проводишь быстрый 2-минутный блиц-экзамен по курсу «{}» ({}).\n\
             {}\n\
             Правила: задавай по одному короткому вопросу, проверяй ответ ученика, веди счёт (например «3 из 5»), вставляй [sfx:fanfare] за верный ответ и [sfx:buzz] за ошибку, мягко подсказывай правильный вариант. Отвечай в 1–2 коротких предложения, без markdown.",
            course.title, course.level, cards_text
        )
    }

    pub fn create(&self, mut course: Course) -> Result<Course, String> {
        if course.title.trim().is_empty() {
            return Err("Укажите название курса".into());
        }
        if course.lessons.is_empty() {
            return Err("В курсе должен быть хотя бы один урок".into());
        }
        let ts = now_ts();
        if course.id.is_empty() {
            course.id = format!("crs_{}", crate::settings::rand_hex(4));
        }
        course.created_at = ts;
        course.updated_at = ts;
        for c in &mut course.cards {
            if c.next_review == 0 {
                c.next_review = ts;
            }
            if c.interval_days == 0 {
                c.interval_days = 1;
            }
        }
        let mut g = self.courses.lock().unwrap();
        if g.len() >= 16 {
            return Err("Максимум 16 курсов".into());
        }
        g.insert(0, course.clone());
        self.save(&g);
        Ok(course)
    }

    pub fn delete(&self, id: &str) -> Result<Value, String> {
        let mut g = self.courses.lock().unwrap();
        let before = g.len();
        g.retain(|c| c.id != id);
        if g.len() == before {
            return Err("Курс не найден".into());
        }
        self.save(&g);
        Ok(json!({ "courses": *g, "now": now_ts() }))
    }

    pub fn mark_lesson_started(&self, course_id: &str, lesson_idx: usize) -> Result<(Course, usize), String> {
        let mut g = self.courses.lock().unwrap();
        let Some(course) = g.iter_mut().find(|c| c.id == course_id) else {
            return Err("Курс не найден".into());
        };
        if course.lessons.is_empty() {
            return Err("В курсе нет уроков".into());
        }
        let idx = lesson_idx.min(course.lessons.len() - 1);
        if course.lessons[idx].status == "todo" {
            course.lessons[idx].status = "in_progress".into();
        }
        course.current_lesson = idx;
        course.updated_at = now_ts();
        let out = course.clone();
        self.save(&g);
        Ok((out, idx))
    }

    pub fn complete_lesson(
        &self,
        course_id: &str,
        lesson_idx: usize,
        notes: String,
        new_cards: Vec<(String, String)>,
        weak: Vec<String>,
    ) -> Result<Course, String> {
        let mut g = self.courses.lock().unwrap();
        let Some(course) = g.iter_mut().find(|c| c.id == course_id) else {
            return Err("Курс не найден".into());
        };
        if lesson_idx >= course.lessons.len() {
            return Err("Урок не найден".into());
        }
        let ts = now_ts();
        let lesson = &mut course.lessons[lesson_idx];
        lesson.status = "done".into();
        lesson.completed_at = ts;
        if !notes.trim().is_empty() {
            lesson.notes = notes.trim().chars().take(600).collect();
        }
        // Переходим к следующему непройденному уроку
        if let Some(next_idx) = course.lessons.iter().position(|l| l.status != "done") {
            course.current_lesson = next_idx;
        } else {
            course.current_lesson = course.lessons.len().saturating_sub(1);
        }
        for (front, back) in new_cards {
            let f = front.trim();
            let b = back.trim();
            if f.is_empty() || b.is_empty() {
                continue;
            }
            let exists = course
                .cards
                .iter()
                .any(|c| c.front.eq_ignore_ascii_case(f));
            if !exists && course.cards.len() < 120 {
                course.cards.push(ReviewCard {
                    front: f.chars().take(120).collect(),
                    back: b.chars().take(200).collect(),
                    interval_days: 1,
                    next_review: ts + 86_400,
                    hits: 0,
                });
            }
        }
        for w in weak {
            let w = w.trim();
            if !w.is_empty() && !course.weak_spots.iter().any(|x| x.eq_ignore_ascii_case(w)) {
                course.weak_spots.push(w.chars().take(140).collect());
                if course.weak_spots.len() > 12 {
                    course.weak_spots.remove(0);
                }
            }
        }
        course.updated_at = ts;
        let out = course.clone();
        self.save(&g);
        Ok(out)
    }

    pub fn add_card(&self, course_id: &str, front: String, back: String) -> Result<Course, String> {
        let f = front.trim();
        let b = back.trim();
        if f.is_empty() || b.is_empty() {
            return Err("Заполните обе стороны карточки".into());
        }
        let mut g = self.courses.lock().unwrap();
        let Some(course) = g.iter_mut().find(|c| c.id == course_id) else {
            return Err("Курс не найден".into());
        };
        if course.cards.len() >= 120 {
            return Err("Максимум 120 карточек в курсе".into());
        }
        let ts = now_ts();
        course.cards.push(ReviewCard {
            front: f.chars().take(120).collect(),
            back: b.chars().take(200).collect(),
            interval_days: 1,
            next_review: ts,
            hits: 0,
        });
        course.updated_at = ts;
        let out = course.clone();
        self.save(&g);
        Ok(out)
    }

    pub fn review_card(&self, course_id: &str, card_idx: usize, ok: bool) -> Result<Course, String> {
        let mut g = self.courses.lock().unwrap();
        let Some(course) = g.iter_mut().find(|c| c.id == course_id) else {
            return Err("Курс не найден".into());
        };
        let Some(card) = course.cards.get_mut(card_idx) else {
            return Err("Карточка не найдена".into());
        };
        let ts = now_ts();
        if ok {
            card.hits += 1;
            card.interval_days = match card.interval_days {
                0..=1 => 3,
                2..=3 => 7,
                4..=7 => 14,
                _ => 30,
            };
        } else {
            card.interval_days = 1;
        }
        card.next_review = ts + u64::from(card.interval_days) * 86_400;
        course.updated_at = ts;
        let out = course.clone();
        self.save(&g);
        Ok(out)
    }

    pub fn delete_card(&self, course_id: &str, card_idx: usize) -> Result<Course, String> {
        let mut g = self.courses.lock().unwrap();
        let Some(course) = g.iter_mut().find(|c| c.id == course_id) else {
            return Err("Курс не найден".into());
        };
        if card_idx >= course.cards.len() {
            return Err("Карточка не найдена".into());
        }
        course.cards.remove(card_idx);
        course.updated_at = now_ts();
        let out = course.clone();
        self.save(&g);
        Ok(out)
    }

    pub fn build_tutor_prompt(course: &Course, lesson_idx: usize) -> String {
        let lesson = &course.lessons[lesson_idx.min(course.lessons.len() - 1)];
        let ts = now_ts();
        let due_cards: Vec<String> = course
            .cards
            .iter()
            .filter(|c| c.next_review <= ts)
            .take(5)
            .map(|c| format!("«{}» = {}", c.front, c.back))
            .collect();
        let key_pts = if lesson.key_points.is_empty() {
            "—".to_string()
        } else {
            lesson.key_points.join("; ")
        };
        let weak = if course.weak_spots.is_empty() {
            String::new()
        } else {
            format!("\nСлабые места ученика (мягко закрепляй): {}.", course.weak_spots.join("; "))
        };
        let due_str = if due_cards.is_empty() {
            String::new()
        } else {
            format!("\nКарточки на повторение (спроси 2–3 в начале урока): {}.", due_cards.join("; "))
        };
        let lang_note = if course.bilingual {
            "\nЭто языковой урок: произноси иностранные фразы чётко, проси ученика повторять, переводить или отвечать на изучаемом языке. Ошибки исправляй доброжелательно, сразу давая правильный вариант."
        } else {
            ""
        };
        format!(
            "Ты персональный голосовой репетитор на умной колонке.\n\
             Курс: «{}» (предмет: {}, уровень: {}, цель: {}).\n\
             Текущий урок {} из {}: «{}».\n\
             Цель урока: {}.\n\
             Ключевые пункты: {}.{}{}{}\n\n\
             Правила ведения голосового урока:\n\
             1. Говори живо, коротко и понятно (2–4 предложения за ход, без списков и markdown).\n\
             2. Структура: сначала короткая разминка (если есть карточки), затем объяснение одного кусочка темы и сразу практический вопрос или мини-диалог.\n\
             3. Всегда заканчивай ход одним чётким вопросом или заданием для ученика.\n\
             4. Хвали за прогресс и мягко поправляй ошибки.",
            course.title,
            course.subject,
            course.level,
            course.goal,
            lesson_idx + 1,
            course.lessons.len(),
            lesson.title,
            lesson.goal,
            key_pts,
            due_str,
            weak,
            lang_note,
        )
    }

    pub fn build_summary_markdown(course: &Course, lesson_idx: Option<usize>) -> String {
        let done = course.lessons.iter().filter(|l| l.status == "done").count();
        let mut out = format!(
            "📚 Курс: {}\nУровень: {} · Прогресс: {}/{}\n",
            course.title,
            course.level,
            done,
            course.lessons.len()
        );
        if let Some(idx) = lesson_idx.filter(|&i| i < course.lessons.len()) {
            let l = &course.lessons[idx];
            out.push_str(&format!("\nУрок {}: {}\n", idx + 1, l.title));
            if !l.notes.is_empty() {
                out.push_str(&format!("Итог: {}\n", l.notes));
            }
            if !l.key_points.is_empty() {
                out.push_str("Ключевое:\n");
                for kp in &l.key_points {
                    out.push_str(&format!("• {kp}\n"));
                }
            }
        }
        if !course.cards.is_empty() {
            out.push_str("\nСловарь / карточки:\n");
            for c in course.cards.iter().take(15) {
                out.push_str(&format!("• {} — {}\n", c.front, c.back));
            }
        }
        if !course.weak_spots.is_empty() {
            out.push_str(&format!("\nПовторить: {}\n", course.weak_spots.join(", ")));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_course() -> Course {
        Course {
            id: String::new(),
            title: "Английский B1".into(),
            subject: "Английский язык".into(),
            level: "B1".into(),
            goal: "Говорить свободно".into(),
            bilingual: true,
            current_lesson: 0,
            lessons: vec![
                Lesson {
                    title: "Времена Past Simple".into(),
                    goal: "Рассказывать о прошлом".into(),
                    key_points: vec!["did".into(), "went".into()],
                    status: "todo".into(),
                    notes: String::new(),
                    completed_at: 0,
                },
                Lesson {
                    title: "Фразовые глаголы".into(),
                    goal: "Использовать в речи".into(),
                    key_points: vec!["give up".into()],
                    status: "todo".into(),
                    notes: String::new(),
                    completed_at: 0,
                },
            ],
            cards: vec![ReviewCard {
                front: "give up".into(),
                back: "сдаваться".into(),
                interval_days: 1,
                next_review: 0,
                hits: 0,
            }],
            weak_spots: Vec::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn course_crud_srs_and_voice_match() {
        let dir = std::env::temp_dir().join(format!("jarvis-crs-{}", crate::settings::rand_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let store = CourseStore::load(dir.clone());

        let c = store.create(sample_course()).unwrap();
        assert!(c.id.starts_with("crs_"));

        let (started, idx) = store.mark_lesson_started(&c.id, 0).unwrap();
        assert_eq!(idx, 0);
        assert_eq!(started.lessons[0].status, "in_progress");

        let done = store
            .complete_lesson(
                &c.id,
                0,
                "Разобрали Past Simple".into(),
                vec![("went".into(), "ходил".into())],
                vec!["артикли".into()],
            )
            .unwrap();
        assert_eq!(done.lessons[0].status, "done");
        assert_eq!(done.current_lesson, 1);
        assert_eq!(done.cards.len(), 2);
        assert_eq!(done.weak_spots, vec!["артикли".to_string()]);

        let r1 = store.review_card(&c.id, 0, true).unwrap();
        assert_eq!(r1.cards[0].interval_days, 3);
        let r2 = store.review_card(&c.id, 0, true).unwrap();
        assert_eq!(r2.cards[0].interval_days, 7);
        let r3 = store.review_card(&c.id, 0, false).unwrap();
        assert_eq!(r3.cards[0].interval_days, 1);

        let (vc, vidx, blitz) = store.match_voice_start("Аянами, вернёмся к 1 уроку английского").unwrap();
        assert_eq!(vc.id, c.id);
        assert_eq!(vidx, 0);
        assert!(!blitz);

        let (_, _, blitz2) = store.match_voice_start("запусти блиц по карточкам").unwrap();
        assert!(blitz2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
