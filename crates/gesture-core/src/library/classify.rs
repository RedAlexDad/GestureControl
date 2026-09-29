//! Поиск жеста в словаре: k-NN по позе и DTW по динамике.

use std::collections::BTreeMap;

use uuid::Uuid;

use crate::features::HandFeatures;
use crate::geometry::HandGeometry;
use crate::motion::MotionFeatures;

use super::{CustomSign, SignLibrary};

/// Число соседей, учитываемых при поиске статичного жеста.
const KNN: usize = 3;

/// Отбрасываем ли мы жест, если второй кандидат не хуже первого?
/// Сравнение слишком близких жестов признаётся неоднозначным.
const AMBIGUITY_RATIO: f32 = 1.15;

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

impl SignLibrary {
    /// Ближайший жест по позе.
    ///
    /// Возвращает `None`, если в кадре нет рук, словарь пуст, никто не
    /// попал в порог близости или лучший кандидат неоднозначен.
    pub fn classify_pose(&self, hands: &[HandGeometry], threshold: f32) -> Option<CustomSign> {
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
            // Сравниваем слова, а не отдельные примеры: у слова берём
            // ближайший пример. Иначе два удачных примера одного и того же
            // жеста выглядели бы как неоднозначность между словами и жест
            // отвергался бы.
            let best = sign
                .samples
                .iter()
                .map(|sample| HandFeatures::distance(&vector, sample))
                .fold(f32::INFINITY, f32::min);
            scored.push((best, sign.id));
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
}
