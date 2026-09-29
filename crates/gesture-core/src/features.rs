//! Признаки позы: векторы, устойчивые к размеру руки и положению в кадре.
//!
//! Перенос `HandFeatures` из `SignLibrary.swift`.

use crate::geometry::HandGeometry;
use crate::joint;

/// Признаки позы одной или двух рук.
#[derive(Debug)]
pub struct HandFeatures;

impl HandFeatures {
    /// Длина вектора одной руки: 20 точек × 2 координаты.
    pub const ONE_HAND_LENGTH: usize = (joint::COUNT - 1) * 2;

    /// Признаки позы одной или двух рук.
    ///
    /// Две руки: признаки левой (на экране) руки + правой + положение правой
    /// относительно левой.
    pub fn sign_vector(hands: &[HandGeometry]) -> Option<Vec<f32>> {
        let mut sorted: Vec<&HandGeometry> = hands.iter().collect();
        sorted.sort_by(|a, b| {
            a.center
                .x
                .partial_cmp(&b.center.x)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        if sorted.len() >= 2 {
            let a = sorted[0];
            let b = sorted[1];
            let va = HandFeatures::hand_vector(a)?;
            let vb = HandFeatures::hand_vector(b)?;
            let scale = (a.size + b.size) / 2.0;
            let relative = [
                (b.center.x - a.center.x) / scale,
                (b.center.y - a.center.y) / scale,
            ];
            let mut v = va;
            v.extend_from_slice(&vb);
            v.extend_from_slice(&relative);
            return Some(v);
        }

        let first = sorted.first()?;
        HandFeatures::hand_vector(first)
    }

    /// 20 точек кисти относительно запястья, в размерах ладони.
    ///
    /// Нормировка делает признаки независимыми от положения руки в кадре
    /// и расстояния до камеры.
    pub fn hand_vector(hand: &HandGeometry) -> Option<Vec<f32>> {
        if !hand.p.iter().all(|q| q.x >= 0.0) {
            return None;
        }
        let origin = hand.p[joint::WRIST];
        let mut v = Vec::with_capacity(Self::ONE_HAND_LENGTH);
        for point in &hand.p[1..] {
            v.push((point.x - origin.x) / hand.size);
            v.push((point.y - origin.y) / hand.size);
        }
        Some(v)
    }

    /// Среднее расстояние между соответствующими точками двух векторов
    /// (в размерах ладони).
    ///
    /// Векторы разной длины (разное число рук) не сравниваются: расстояние
    /// бесконечно, иначе один жест одной рукой совпал бы с жестом двумя руками.
    pub fn distance(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() || a.is_empty() {
            return f32::INFINITY;
        }
        let mut sum = 0.0f32;
        let mut i = 0usize;
        while i + 1 < a.len() {
            let dx = a[i] - b[i];
            let dy = a[i + 1] - b[i + 1];
            sum += (dx * dx + dy * dy).sqrt();
            i += 2;
        }
        sum / (a.len() / 2) as f32
    }

    /// Число рук по длине вектора признаков.
    pub fn hand_count(vector_len: usize) -> usize {
        if vector_len > crate::motion::MotionFeatures::ONE_HAND_FRAME_LENGTH {
            2
        } else {
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    /// Ладонь 100 px: точки задаются в её размерах, поэтому вектор читаем.
    fn hand(shift: f32) -> HandGeometry {
        // Запястье сдвинуто вправо от нуля: в этом коде отрицательный `x`
        // означает «точка не найдена», поэтому кисть у левого края кадра
        // считалась бы битой.
        let x = shift + 100.0;
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(x, 0.0);
        p[joint::INDEX_MCP] = Point::new(x - 25.0, -45.0);
        p[joint::MIDDLE_MCP] = Point::new(x, -100.0);
        p[joint::RING_MCP] = Point::new(x + 25.0, -45.0);
        p[joint::LITTLE_MCP] = Point::new(x + 48.0, -30.0);
        for (offset, point) in p[joint::THUMB_CMC..=joint::LITTLE_TIP]
            .iter_mut()
            .enumerate()
        {
            if *point == Point::MISSING {
                let i = (joint::THUMB_CMC + offset) as f32;
                *point = Point::new(x + i * 2.0, -80.0 + i * 3.0);
            }
        }
        HandGeometry::new(&p).unwrap()
    }

    #[test]
    fn one_hand_vector_length() {
        let v = HandFeatures::hand_vector(&hand(0.0)).unwrap();
        assert_eq!(v.len(), HandFeatures::ONE_HAND_LENGTH);
        assert_eq!(v.len(), 40);
    }

    #[test]
    fn vector_ignores_position_in_frame() {
        let a = HandFeatures::hand_vector(&hand(0.0)).unwrap();
        let b = HandFeatures::hand_vector(&hand(500.0)).unwrap();
        assert!(
            HandFeatures::distance(&a, &b) < 1e-4,
            "вектор не зависит от положения"
        );
    }

    #[test]
    fn distance_rejects_different_lengths() {
        assert!(HandFeatures::distance(&[0.0; 40], &[0.0; 82]).is_infinite());
        assert!(HandFeatures::distance(&[], &[]).is_infinite());
    }

    #[test]
    fn distance_of_identical_vectors_is_zero() {
        let a = HandFeatures::hand_vector(&hand(0.0)).unwrap();
        assert!(HandFeatures::distance(&a, &a) < 1e-6);
    }

    #[test]
    fn two_hands_add_relative_position() {
        let a = hand(0.0);
        let b = hand(150.0);
        let two = HandFeatures::sign_vector(&[a, b]).unwrap();
        // 40 + 40 признаков обеих рук + 2 координаты взаимного положения.
        assert_eq!(two.len(), 82);
        // Руки отсортированы по x, поэтому относительное смещение положительно.
        assert!((two[80] - 1.5).abs() < 1e-4, "dx = {}", two[80]);
    }

    #[test]
    fn hand_count_inferred_from_length() {
        assert_eq!(HandFeatures::hand_count(40), 1);
        assert_eq!(HandFeatures::hand_count(82), 2);
    }
}
