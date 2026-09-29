//! Словарь жестов пользователя: обучение, поиск и хранение.
//!
//! Перенос `CustomSign` и `SignLibrary` из `SignLibrary.swift`.
//!
//! Статичные жесты ищутся ближайшим соседом (k-NN, k=3) в пространстве
//! признаков позы. Динамические жесты сравниваются шаблонами по subsequence
//! DTW, после чего кандидаты ранжируются числом совпадений подряд.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::HandFeatures;
use crate::motion::MotionFeatures;

/// Число соседей, учитываемых при поиске статичного жеста.
const KNN: usize = 3;

/// Отбрасываем ли мы жест, если второй кандидат не хуже первого?
/// Сравнение слишком близких жестов признаётся неоднозначным.
const AMBIGUITY_RATIO: f32 = 1.15;

/// Жест пользователя: слово + набранные примеры.
///
/// * `samples` — векторы признаков позы для статичных жестов.
/// * `sequences` — последовательности кадров для динамических.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSign {
    /// Идентификатор для обращения из интерфейса.
    /// В старых файлах может отсутствовать — тогда создаётся новый.
    #[serde(default = "Uuid::new_v4")]
    pub id: Uuid,
    /// Слово, которому соответствует жест.
    pub word: String,
    /// Примеры статичной позы: по одному вектору на кадр.
    #[serde(default)]
    pub samples: Vec<Vec<f32>>,
    /// Примеры динамики: последовательности кадров.
    #[serde(default)]
    pub sequences: Vec<Vec<Vec<f32>>>,
}

impl CustomSign {
    pub fn new(word: &str) -> Self {
        CustomSign {
            id: Uuid::new_v4(),
            word: word.to_string(),
            samples: Vec::new(),
            sequences: Vec::new(),
        }
    }

    pub fn title(&self) -> String {
        format!("Свой жест «{}»", self.word)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty() && self.sequences.is_empty()
    }

    /// Сколько рук использовано в примерах: 1 или 2.
    pub fn hand_count(&self) -> usize {
        if let Some(first) = self.samples.first() {
            return HandFeatures::hand_count(first.len());
        }
        if let Some(frame) = self.sequences.iter().flatten().next() {
            return HandFeatures::hand_count(frame.len());
        }
        1
    }

    /// Шаблоны динамики, приведённые к виду, удобному для DTW.
    pub fn prepared_templates(&self) -> Vec<Vec<Vec<f32>>> {
        self.sequences
            .iter()
            .map(|seq| MotionFeatures::prepare_template(seq))
            .collect()
    }
}

/// Отбирает лучшего кандидата из отсортированного по расстоянию списка.
///
/// Возвращает `None`, если кандидат неоднозначен: когда второй результат
/// почти столь же близок, выбор был бы произвольным.
fn pick_best(mut scored: Vec<(f32, Uuid)>) -> Option<(Uuid, f32)> {
    if scored.is_empty() {
        return None;
    }
    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (distance, id) = scored[0];
    if let Some((second, _)) = scored.get(1) {
        if *second < distance * AMBIGUITY_RATIO {
            return None;
        }
    }
    Some((id, distance))
}

/// Хранилище словаря. Позволяет подставить файловое или сетевое хранилище.
///
/// Трейт требует `Send + Sync`, потому что словарь принадлежит общему
/// состоянию окна: к нему обращается и поток кадров, и поток интерфейса.
pub trait SignStore: Send + Sync {
    fn load(&self) -> Result<Vec<CustomSign>, LibraryError>;
    fn save(&self, signs: &[CustomSign]) -> Result<(), LibraryError>;
}

/// Хранилище в памяти: используется в тестах и как запасной вариант,
/// если файл недоступен.
#[derive(Debug, Default)]
pub struct MemoryStore {
    signs: std::sync::Mutex<Vec<CustomSign>>,
}

impl MemoryStore {
    pub fn new(signs: Vec<CustomSign>) -> Self {
        MemoryStore {
            signs: std::sync::Mutex::new(signs),
        }
    }
}

impl SignStore for MemoryStore {
    fn load(&self) -> Result<Vec<CustomSign>, LibraryError> {
        // Отравленный мьютекс означает панику в holders-потоке. Данные
        // словаря к этому моменту согласованы, поэтому читаем их как есть.
        Ok(self.signs.lock().map(|g| g.clone()).unwrap_or_default())
    }

    fn save(&self, signs: &[CustomSign]) -> Result<(), LibraryError> {
        let mut guard = self.signs.lock().map_err(|_| LibraryError::Lock)?;
        *guard = signs.to_vec();
        Ok(())
    }
}

/// Хранилище в JSON-файле — замена документов из iOS-версии.
#[derive(Debug)]
pub struct JsonFileStore {
    path: PathBuf,
}

impl JsonFileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        JsonFileStore { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SignStore for JsonFileStore {
    fn load(&self) -> Result<Vec<CustomSign>, LibraryError> {
        // Отсутствующий файл — это пустой словарь, а не ошибка:
        // приложение обязано запускаться при первом запуске.
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let text = std::fs::read_to_string(&self.path).map_err(|e| LibraryError::Io {
            path: self.path.clone(),
            source: e,
        })?;
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        serde_json::from_str(&text).map_err(|e| LibraryError::Format(e.to_string()))
    }

    fn save(&self, signs: &[CustomSign]) -> Result<(), LibraryError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| LibraryError::Io {
                    path: parent.to_path_buf(),
                    source: e,
                })?;
            }
        }
        let text =
            serde_json::to_string_pretty(signs).map_err(|e| LibraryError::Format(e.to_string()))?;
        // Запись через временный файл: при сбое питания старый словарь
        // останется целым.
        let tmp = self.path.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|e| LibraryError::Io {
            path: tmp.clone(),
            source: e,
        })?;
        std::fs::rename(&tmp, &self.path).map_err(|e| LibraryError::Io {
            path: self.path.clone(),
            source: e,
        })?;
        Ok(())
    }
}

/// Словарь пользовательских жестов.
#[derive(Debug)]
pub struct SignLibrary {
    signs: Vec<CustomSign>,
    /// Идемпотентность: готовые шаблоны кешируются, чтобы не пересчитывать
    /// обрезку и прореживание на каждом кадре.
    prepared: BTreeMap<Uuid, Vec<Vec<Vec<f32>>>>,
}

impl SignLibrary {
    /// Загружает словарь из хранилища. Ошибка чтения не блокирует запуск:
    /// словарь остаётся пустым, а данные не перезаписываются до явного
    /// сохранения пользователем.
    pub fn load(store: &dyn SignStore) -> Self {
        let signs = store.load().unwrap_or_default();
        let prepared = signs
            .iter()
            .map(|s| (s.id, s.prepared_templates()))
            .collect();
        SignLibrary { signs, prepared }
    }

    pub fn from_signs(signs: Vec<CustomSign>) -> Self {
        let prepared = signs
            .iter()
            .map(|s| (s.id, s.prepared_templates()))
            .collect();
        SignLibrary { signs, prepared }
    }

    pub fn signs(&self) -> &[CustomSign] {
        &self.signs
    }

    pub fn is_empty(&self) -> bool {
        self.signs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.signs.len()
    }

    /// Полное описание словаря для интерфейса.
    pub fn snapshot(&self) -> Vec<CustomSign> {
        self.signs.clone()
    }

    /// Ищет жест по индексу. `None` — индекс вне диапазона.
    pub fn get(&self, index: usize) -> Option<&CustomSign> {
        self.signs.get(index)
    }

    pub fn index_of_id(&self, id: Uuid) -> Option<usize> {
        self.signs.iter().position(|s| s.id == id)
    }

    /// Добавляет пример жеста. Слово сравнивается без учёта регистра,
    /// поэтому «Привет» и «привет» — один и тот же жест.
    pub fn add(&mut self, word: &str, samples: Vec<Vec<f32>>) -> Uuid {
        match self.index_by_word(word) {
            Some(index) => {
                self.signs[index].samples.extend(samples);
                self.signs[index].id
            }
            None => {
                let sign = CustomSign {
                    id: Uuid::new_v4(),
                    word: word.to_string(),
                    samples,
                    sequences: Vec::new(),
                };
                self.signs.push(sign.clone());
                self.prepared.insert(sign.id, Vec::new());
                sign.id
            }
        }
    }

    /// Добавляет пример динамического жеста.
    ///
    /// Готовый шаблон пополняет кеш: пересчитывать обрезку и прореживание
    /// на каждом кадре незачем.
    pub fn add_sequence(&mut self, word: &str, sequence: Vec<Vec<f32>>) -> Uuid {
        match self.index_by_word(word) {
            Some(index) => {
                let id = self.signs[index].id;
                let template = MotionFeatures::prepare_template(&sequence);
                self.signs[index].sequences.push(sequence);
                self.prepared.entry(id).or_default().push(template);
                id
            }
            None => {
                let sign = CustomSign {
                    id: Uuid::new_v4(),
                    word: word.to_string(),
                    samples: Vec::new(),
                    sequences: vec![sequence.clone()],
                };
                let template = MotionFeatures::prepare_template(&sequence);
                self.signs.push(sign.clone());
                self.prepared.insert(sign.id, vec![template]);
                sign.id
            }
        }
    }

    /// Удаляет жест целиком.
    pub fn delete(&mut self, index: usize) -> Option<CustomSign> {
        if index >= self.signs.len() {
            return None;
        }
        let removed = self.signs.remove(index);
        self.prepared.remove(&removed.id);
        Some(removed)
    }

    /// Удаляет жест по идентификатору.
    pub fn delete_id(&mut self, id: Uuid) -> Option<CustomSign> {
        let index = self.index_of_id(id)?;
        self.delete(index)
    }

    /// Очищает словарь.
    pub fn clear(&mut self) {
        self.signs.clear();
        self.prepared.clear();
    }

    /// Сохраняет словарь в хранилище.
    pub fn save(&self, store: &dyn SignStore) -> Result<(), LibraryError> {
        store.save(&self.signs)
    }

    fn index_by_word(&self, word: &str) -> Option<usize> {
        let needle = word.to_lowercase();
        self.signs
            .iter()
            .position(|s| s.word.to_lowercase() == needle)
    }

    /// Ближайший жест по позе.
    ///
    /// Возвращает `None`, если в кадре нет рук, словарь пуст, никто не
    /// попал в порог близости или лучший кандидат неоднозначен.
    pub fn classify_pose(
        &self,
        hands: &[crate::geometry::HandGeometry],
        threshold: f32,
    ) -> Option<CustomSign> {
        let vector = HandFeatures::sign_vector(hands)?;

        // Только жесты с таким же числом рук: векторы разной длины несравнимы.
        let vector_hands = HandFeatures::hand_count(vector.len());
        let mut scored: Vec<(f32, Uuid)> = Vec::new();
        for sign in &self.signs {
            if sign.samples.is_empty()
                || HandFeatures::hand_count(sign.samples[0].len()) != vector_hands
            {
                continue;
            }
            for sample in &sign.samples {
                scored.push((HandFeatures::distance(&vector, sample), sign.id));
            }
        }

        // Слишком далёкие совпадения игнорируем: жест должен быть узнаваем,
        // иначе приложение выдавало бы случайные слова.
        scored.retain(|(distance, _)| *distance < threshold);

        // Только ближайшие соседи.
        scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(KNN);

        let (id, _) = pick_best(scored)?;
        self.signs.iter().find(|s| s.id == id).cloned()
    }

    /// Ближайший жест по динамике.
    ///
    /// Для каждого шаблона DTW ищет, встречается ли движение в потоке кадров.
    /// Затем кандидаты ранжируются по числу подряд идущих совпадений:
    /// так настоящий жест выигрывает у случайно совпавшего отрезка.
    pub fn classify_motion(&self, stream: &[Vec<f32>], threshold: f32) -> Option<CustomSign> {
        if stream.is_empty() {
            return None;
        }

        // Сколько шаблонов подряд совпало для каждого жеста.
        let mut streaks: BTreeMap<Uuid, usize> = BTreeMap::new();
        let mut distances: BTreeMap<Uuid, f32> = BTreeMap::new();

        for sign in &self.signs {
            let templates = self
                .prepared
                .get(&sign.id)
                .map(|t| t.as_slice())
                .unwrap_or(&[]);
            if templates.is_empty() {
                continue;
            }

            let mut streak = 0usize;
            for template in templates {
                if template.is_empty() || template[0].len() != stream[0].len() {
                    continue;
                }
                let distance = MotionFeatures::distance_with_hands(template, stream);
                if distance < threshold {
                    streak += 1;
                    let entry = distances.entry(sign.id).or_insert(distance);
                    *entry = entry.min(distance);
                } else {
                    // Совпадение прервалось: результат уже засчитан.
                    if streak > 0 {
                        let id = sign.id;
                        let recorded = streaks.entry(id).or_insert(0);
                        *recorded = (*recorded).max(streak);
                    }
                    streak = 0;
                }
            }
            if streak > 0 {
                let id = sign.id;
                let recorded = streaks.entry(id).or_insert(0);
                *recorded = (*recorded).max(streak);
            }
        }

        // Сначала побеждает жест с самой длинной серией совпадений,
        // при равенстве — с меньшим расстоянием.
        let mut ranked: Vec<(usize, f32, Uuid)> = streaks
            .into_iter()
            .filter(|(_, streak)| *streak > 0)
            .map(|(id, streak)| {
                let distance = distances.get(&id).copied().unwrap_or(f32::INFINITY);
                (streak, distance, id)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        });

        let (_, _, id) = ranked.into_iter().next()?;
        self.signs.iter().find(|s| s.id == id).cloned()
    }

    /// Диагностика: сколько примеров у каждого жеста.
    pub fn stats(&self) -> Vec<(String, usize, usize)> {
        self.signs
            .iter()
            .map(|s| (s.word.clone(), s.samples.len(), s.sequences.len()))
            .collect()
    }
}

/// Ошибки работы со словарём.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("не удалось прочитать файл {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("файл словаря повреждён: {0}")]
    Format(String),
    /// Хранилище занято: другой поток упал, удерживая блокировку.
    #[error("хранилище жестов заблокировано другим потоком")]
    Lock,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{HandGeometry, Point};
    use crate::joint;

    /// Пустой словарь для теста.
    fn empty_library() -> SignLibrary {
        SignLibrary::from_signs(Vec::new())
    }

    /// Кисть с заданным положением большого пальца.
    fn hand(thumb_up: bool) -> HandGeometry {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(200.0, 300.0);
        p[joint::INDEX_MCP] = Point::new(175.0, 255.0);
        p[joint::MIDDLE_MCP] = Point::new(200.0, 245.0);
        p[joint::RING_MCP] = Point::new(225.0, 255.0);
        p[joint::LITTLE_MCP] = Point::new(248.0, 270.0);
        for point in p[joint::THUMB_CMC..=joint::LITTLE_TIP].iter_mut() {
            if *point == Point::MISSING {
                *point = Point::new(200.0, 220.0);
            }
        }
        // Большой палец: поднят или прижат к ладони.
        p[joint::THUMB_TIP] = Point::new(
            if thumb_up { 150.0 } else { 190.0 },
            if thumb_up { 200.0 } else { 265.0 },
        );
        HandGeometry::new(&p).unwrap()
    }

    /// Поза с заданным положением большого пальца: у одного слова
    /// палец поднят, у другого опущен.
    fn pose(thumb_up: bool) -> Vec<f32> {
        HandFeatures::hand_vector(&hand(thumb_up)).unwrap()
    }

    /// Последовательность кадров, смещающихся в заданном направлении.
    fn stream(dx: f32, frames: usize) -> Vec<Vec<f32>> {
        (0..frames)
            .map(|i| MotionFeatures::frame(&pose(true), dx * i as f32, 0.0))
            .collect()
    }

    #[test]
    fn add_creates_sign_and_merges_by_case_insensitive_word() {
        let mut lib = empty_library();
        let first = lib.add("Привет", vec![pose(true)]);
        assert_eq!(lib.len(), 1);

        let second = lib.add("привет", vec![pose(true)]);
        assert_eq!(first, second, "регистр не влияет на объединение");
        assert_eq!(lib.len(), 1);
        assert_eq!(lib.get(0).unwrap().samples.len(), 2);
    }

    #[test]
    fn classify_pose_matches_learned_word() {
        let mut lib = empty_library();
        lib.add("да", vec![pose(true)]);
        lib.add("нет", vec![pose(false)]);

        assert_eq!(lib.classify_pose(&[hand(true)], 0.4).unwrap().word, "да");
        assert_eq!(lib.classify_pose(&[hand(false)], 0.4).unwrap().word, "нет");
    }

    #[test]
    fn classify_pose_rejects_unrelated_pose() {
        let mut lib = empty_library();
        lib.add("да", vec![pose(true)]);

        // Совершенно другая поза: пальцы сжаты в кулак и опущены к
        // запястью, большой палец лежит поперёк ладони. Отличается сразу
        // десяток точек, а не одна — иначе среднее расстояние по 20 точкам
        // осталось бы в пределах порога.
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(200.0, 300.0);
        p[joint::INDEX_MCP] = Point::new(175.0, 255.0);
        p[joint::MIDDLE_MCP] = Point::new(200.0, 245.0);
        p[joint::RING_MCP] = Point::new(225.0, 255.0);
        p[joint::LITTLE_MCP] = Point::new(248.0, 270.0);
        p[joint::THUMB_CMC] = Point::new(183.0, 292.0);
        for (mcp, pip, dip, tip) in [
            (
                joint::INDEX_MCP,
                joint::INDEX_PIP,
                joint::INDEX_DIP,
                joint::INDEX_TIP,
            ),
            (
                joint::MIDDLE_MCP,
                joint::MIDDLE_PIP,
                joint::MIDDLE_DIP,
                joint::MIDDLE_TIP,
            ),
            (
                joint::RING_MCP,
                joint::RING_PIP,
                joint::RING_DIP,
                joint::RING_TIP,
            ),
            (
                joint::LITTLE_MCP,
                joint::LITTLE_PIP,
                joint::LITTLE_DIP,
                joint::LITTLE_TIP,
            ),
        ] {
            let base = p[mcp];
            p[pip] = Point::new(base.x - 3.0, base.y + 17.0);
            p[dip] = Point::new(base.x + 3.0, base.y + 30.0);
            p[tip] = Point::new(base.x + 9.0, base.y + 41.0);
        }
        p[joint::THUMB_MP] = Point::new(172.0, 288.0);
        p[joint::THUMB_IP] = Point::new(163.0, 278.0);
        p[joint::THUMB_TIP] = Point::new(155.0, 268.0);
        let hand = HandGeometry::new(&p).unwrap();

        assert!(
            lib.classify_pose(&[hand], 0.3).is_none(),
            "далёкая поза не должна узнаваться"
        );
    }

    #[test]
    fn classify_motion_matches_direction() {
        let mut lib = empty_library();
        lib.add_sequence("вправо", stream(2.0, 14));
        lib.add_sequence("влево", stream(-2.0, 14));

        assert_eq!(
            lib.classify_motion(&stream(2.0, 14), 0.6).unwrap().word,
            "вправо"
        );
        assert_eq!(
            lib.classify_motion(&stream(-2.0, 14), 0.6).unwrap().word,
            "влево"
        );
    }

    #[test]
    fn classify_motion_ignores_still_stream() {
        let mut lib = empty_library();
        lib.add_sequence("вправо", stream(2.0, 14));
        let still: Vec<Vec<f32>> = (0..14)
            .map(|_| MotionFeatures::frame(&pose(true), 0.0, 0.0))
            .collect();
        assert!(lib.classify_motion(&still, 0.6).is_none());
    }

    #[test]
    fn delete_removes_sign() {
        let mut lib = empty_library();
        let id = lib.add("да", vec![pose(true)]);
        assert!(lib.delete_id(id).is_some());
        assert!(lib.is_empty());
        assert!(
            lib.delete(0).is_none(),
            "повторное удаление ничего не делает"
        );
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::default();
        let mut lib = empty_library();
        lib.add("да", vec![pose(true)]);
        lib.save(&store).unwrap();

        let restored = SignLibrary::load(&store);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored.get(0).unwrap().word, "да");
    }

    #[test]
    fn json_store_round_trip_preserves_id() {
        let dir = std::env::temp_dir().join(format!("gctrl-test-{}", Uuid::new_v4()));
        let path = dir.join("signs.json");
        let store = JsonFileStore::new(&path);

        // Отсутствующий файл — пустой словарь, а не ошибка.
        assert!(SignLibrary::load(&store).is_empty());

        let mut lib = empty_library();
        let id = lib.add("да", vec![pose(true)]);
        lib.add_sequence("вправо", stream(2.0, 12));
        lib.save(&store).unwrap();

        let restored = SignLibrary::load(&store);
        assert_eq!(restored.len(), 2);
        assert_eq!(restored.get(0).unwrap().id, id, "идентификатор сохраняется");
        assert_eq!(restored.get(1).unwrap().sequences.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn json_store_creates_missing_directory() {
        let dir = std::env::temp_dir().join(format!("gctrl-test-{}", Uuid::new_v4()));
        let store = JsonFileStore::new(dir.join("nested").join("signs.json"));
        let mut lib = empty_library();
        lib.add("да", vec![pose(true)]);
        lib.save(&store).expect("каталог создаётся автоматически");
        assert!(store.path().exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hand_count_reported_from_samples() {
        let mut lib = empty_library();
        lib.add("да", vec![pose(true)]);
        assert_eq!(lib.get(0).unwrap().hand_count(), 1);
    }

    #[test]
    fn stats_report_sample_counts() {
        let mut lib = empty_library();
        lib.add("да", vec![pose(true), pose(true)]);
        lib.add_sequence("вправо", stream(2.0, 12));
        assert_eq!(
            lib.stats(),
            vec![("да".to_string(), 2, 0), ("вправо".to_string(), 0, 1)]
        );
    }
}
