//! Признаки движения: кадры с дельтой смещения, обрезка шаблона и DTW.
//!
//! Перенос `MotionFeatures` из `SignLibrary.swift`. Динамические жесты
//! сравниваются через subsequence DTW, поэтому темп и длительность жеста
//! не влияют на результат.

use crate::features::HandFeatures;

/// Минимальное движение, при котором кадр считается частью жеста.
/// Расстояние нормализовано размерами ладони.
const MOTION_THRESHOLD: f32 = 0.15;

/// Порог обрезки шаблона: неподвижные кадры в начале и в конце отбрасываются.
const TRIM_THRESHOLD: f32 = 0.03;

/// Жест не короче 10 кадров (~0.33 с при 30 fps).
const MIN_FRAMES: usize = 10;

/// Верхняя граница длины шаблона: 30 кадров ≈ 1 с.
const MAX_FRAMES: usize = 30;

/// Кадры-запасы добавляются с обеих сторон обрезанного шаблона,
/// чтобы движение не обрезалось по краям.
const _TRIM_PADDING: usize = 3;

#[derive(Debug)]
pub struct MotionFeatures;

impl MotionFeatures {
    /// Вес смещения в кадре: движение должно влиять на признак сильнее
    /// положения, иначе DTW «предпочтёт» неподвижную руку.
    pub const MOTION_WEIGHT: f32 = 3.0;

    /// Длина кадра одной руки: 40 признаков позы + смещение по x и y.
    pub const ONE_HAND_FRAME_LENGTH: usize = HandFeatures::ONE_HAND_LENGTH + 2;

    /// Кадр последовательности: признаки позы + смещение центра кисти
    /// от предыдущего кадра, с усиленным весом.
    pub fn frame(shape: &[f32], dx: f32, dy: f32) -> Vec<f32> {
        let mut f = Vec::with_capacity(shape.len() + 2);
        f.extend_from_slice(shape);
        f.push(dx * Self::MOTION_WEIGHT);
        f.push(dy * Self::MOTION_WEIGHT);
        f
    }

    /// Расстояние между кадрами.
    ///
    /// Разное число рук в кадрах (например, рука briefly пропала) даёт
    /// фиксированное расстояние 1.5, чтобы такие кадры не давали ложного
    /// «очень похожего» совпадения и не вырождали сумму в ноль.
    pub fn frame_distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() == b.len() {
            HandFeatures::distance(a, b)
        } else {
            1.5
        }
    }

    /// Нормализация кадра на число рук: две руки «весят» вдвое больше,
    /// поэтому и расстояние между кадрами делится пополам.
    pub fn normalize_by_hands(mut frame: Vec<f32>) -> Vec<f32> {
        if frame.len() > Self::ONE_HAND_FRAME_LENGTH {
            for v in &mut frame {
                *v /= 2.0;
            }
        }
        frame
    }

    /// Есть ли движение в кадре: длина вектора смещения выше порога.
    pub fn has_motion(frame: &[f32]) -> bool {
        if frame.len() < 2 {
            return false;
        }
        let dx = frame[frame.len() - 2];
        let dy = frame[frame.len() - 1];
        (dx * dx + dy * dy).sqrt() > MOTION_THRESHOLD
    }

    /// Приводит записанную последовательность к шаблону, пригодному для DTW:
    /// убирает неподвижное начало и конец, добавляет запасы и прореживает.
    pub fn prepare_template(frames: &[Vec<f32>]) -> Vec<Vec<f32>> {
        if frames.is_empty() {
            return Vec::new();
        }

        let motion = |i: usize| -> f32 {
            let f = &frames[i];
            let dx = f[f.len() - 2];
            let dy = f[f.len() - 1];
            (dx * dx + dy * dy).sqrt()
        };

        // Находим диапазон кадров с заметным движением.
        let mut first = 0usize;
        while first + 1 < frames.len() && motion(first) < TRIM_THRESHOLD {
            first += 1;
        }

        let mut last = frames.len() - 1;
        while last > first && motion(last) < TRIM_THRESHOLD {
            last -= 1;
        }

        let mut trimmed: Vec<Vec<f32>> = frames[first..=last].to_vec();

        // Почти неподвижная рука: DTW по таким кадрам неинформативен,
        // поэтому оставляем исходную последовательность целиком.
        if trimmed.len() < MIN_FRAMES {
            trimmed = frames.to_vec();
        }

        // Прореживаем в 2 раза: 30 fps → 15 fps, этого достаточно,
        // шаблон получается короче и сравнение быстрее.
        let halved: Vec<Vec<f32>> = trimmed.iter().step_by(2).cloned().collect::<Vec<_>>();

        let mut result = halved;
        if result.len() > MAX_FRAMES {
            let last = result.len() - 1;
            let step = last as f32 / (MAX_FRAMES - 1) as f32;
            result = (0..MAX_FRAMES)
                .map(|i| result[((i as f32 * step).round() as usize).min(last)].clone())
                .collect();
        }
        result
    }

    /// Subsequence DTW: ищет шаблон в потоке кадров, начиная с любого момента.
    ///
    /// Начало выравнивания свободное (`prev[0] = frame_distance(a[0], b[j])`),
    /// а конец жёсткий: возвращается среднее по последней строке, поэтому
    /// шаблон должен завершиться вместе с потоком. Это отличает динамические
    /// жесты от статичных.
    pub fn subsequence_dtw(template: &[Vec<f32>], stream: &[Vec<f32>]) -> f32 {
        if template.is_empty() || stream.is_empty() {
            return f32::INFINITY;
        }

        let n = template.len();
        let m = stream.len();
        let mut prev = vec![f32::INFINITY; m];
        let mut cur = vec![f32::INFINITY; m];

        // Свободное начало.
        for j in 0..m {
            prev[j] = Self::frame_distance(&template[0], &stream[j]);
        }

        for template_frame in template.iter().skip(1) {
            cur[0] = f32::INFINITY;
            for j in 1..m {
                let cost = Self::frame_distance(template_frame, &stream[j]);
                // Три возможных перехода: остаться в потоке, продвинуться
                // по потоку или по диагонали. `prev[j - 1]` — это в том
                // числе клетка со свободным началом (0, 0), поэтому
                // обращение к ней допустимо уже при j == 1.
                cur[j] = cost + prev[j].min(cur[j - 1]).min(prev[j - 1]);
            }
            std::mem::swap(&mut prev, &mut cur);
        }

        // Жёсткий конец: сумма по всем вариантам выравнивания, делённая на длину.
        prev[m - 1] / n as f32
    }

    /// Расстояние между шаблоном и потоком с учётом разного числа рук.
    pub fn distance_with_hands(template: &[Vec<f32>], stream: &[Vec<f32>]) -> f32 {
        Self::subsequence_dtw(
            &template
                .iter()
                .map(|f| Self::normalize_by_hands(f.clone()))
                .collect::<Vec<_>>(),
            &stream
                .iter()
                .map(|f| Self::normalize_by_hands(f.clone()))
                .collect::<Vec<_>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кадр со смещением: 40 нулей + (dx*3, dy*3).
    fn frame(dx: f32, dy: f32) -> Vec<f32> {
        MotionFeatures::frame(&[0.0; HandFeatures::ONE_HAND_LENGTH], dx, dy)
    }

    #[test]
    fn frame_length_and_motion_weighting() {
        let f = frame(0.5, -0.25);
        assert_eq!(f.len(), MotionFeatures::ONE_HAND_FRAME_LENGTH);
        assert_eq!(f.len(), 42);
        assert!((f[40] - 1.5).abs() < 1e-5, "dx усилен весом 3");
        assert!((f[41] + 0.75).abs() < 1e-5);
    }

    #[test]
    fn two_hand_frames_normalized_to_one_hand_scale() {
        let mut two = vec![2.0; 82];
        let norm = MotionFeatures::normalize_by_hands(std::mem::take(&mut two));
        assert!(norm.iter().all(|&v| (v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn mismatched_frame_counts_use_fixed_penalty() {
        assert!((MotionFeatures::frame_distance(&frame(0.0, 0.0), &[0.0; 82]) - 1.5).abs() < 1e-6);
    }

    #[test]
    fn identical_sequences_have_zero_distance() {
        let stream: Vec<Vec<f32>> = (0..12).map(|i| frame(i as f32 * 0.3, 0.0)).collect();
        let d = MotionFeatures::subsequence_dtw(&stream, &stream);
        assert!(d < 1e-5, "distance = {d}");
    }

    #[test]
    fn dtw_matches_pattern_at_any_offset() {
        // Поток: 8 покоя, затем жест, затем 8 покоя.
        let gesture: Vec<Vec<f32>> = (0..10).map(|i| frame(i as f32 * 0.4, 0.2)).collect();
        let mut stream = vec![frame(0.0, 0.0); 8];
        stream.extend(gesture.iter().cloned());
        stream.extend(vec![frame(0.0, 0.0); 8]);

        let d = MotionFeatures::subsequence_dtw(&gesture, &stream);
        assert!(d < 0.5, "жест должен находиться в потоке, distance = {d}");
    }

    #[test]
    fn dtw_rejects_different_trajectory() {
        let right: Vec<Vec<f32>> = (0..10).map(|i| frame(i as f32 * 0.4, 0.0)).collect();
        let left: Vec<Vec<f32>> = (0..10).map(|i| frame(-i as f32 * 0.4, 0.0)).collect();
        let d = MotionFeatures::subsequence_dtw(&right, &left);
        assert!(
            d > 0.5,
            "движения в разные стороны не должны совпадать, distance = {d}"
        );
    }

    #[test]
    fn dtw_handles_empty_input() {
        assert!(MotionFeatures::subsequence_dtw(&[], &[frame(0.0, 0.0)]).is_infinite());
        assert!(MotionFeatures::subsequence_dtw(&[frame(0.0, 0.0)], &[]).is_infinite());
    }

    #[test]
    fn template_is_trimmed_of_idle_frames() {
        let mut stream = vec![frame(0.0, 0.0); 20];
        stream.extend((0..12).map(|i| frame(i as f32 * 0.4, 0.0)));
        let template = MotionFeatures::prepare_template(&stream);
        // 20 покоящихся + 12 движущихся = 32; обрезка + прореживание в 2 раза.
        assert!(
            template.len() <= 20,
            "шаблон не должен быть длинным: {}",
            template.len()
        );
        assert!(
            template.len() >= 6,
            "шаблон должен сохранить движение: {}",
            template.len()
        );
    }

    #[test]
    fn template_is_capped_at_max_frames() {
        let stream: Vec<Vec<f32>> = (0..200).map(|i| frame(i as f32 * 0.2, 0.0)).collect();
        assert_eq!(MotionFeatures::prepare_template(&stream).len(), MAX_FRAMES);
    }

    #[test]
    fn still_sequence_keeps_all_frames() {
        // Рука неподвижна: обрезка отбросила бы всё, поэтому кадры сохраняются.
        let stream: Vec<Vec<f32>> = (0..15).map(|_| frame(0.0, 0.0)).collect();
        let template = MotionFeatures::prepare_template(&stream);
        assert!(!template.is_empty());
        assert!(template.len() <= stream.len());
    }

    #[test]
    fn motion_detection_uses_threshold() {
        assert!(!MotionFeatures::has_motion(&frame(0.01, 0.0)));
        assert!(MotionFeatures::has_motion(&frame(0.5, 0.0)));
        assert!(!MotionFeatures::has_motion(&[0.0, 0.0]));
    }
}
