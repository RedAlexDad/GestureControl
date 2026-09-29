//! Разбор вывода детектора ладоней.
//!
//! Модель отдаёт по одному якорю на выход: оценку, смещение рамки и семь
//! ключевых точек. Сами якоря модель не хранит — они строятся по её
//! головкам: при шаге сетки 8 и 16 на клетку приходится по два якоря, при
//! шаге 32 — шесть. Проверено по числу каналов свёрток (2, 2, 6) и по
//! размеру выхода модели (2944 = 32²·2 + 16²·2 + 8²·6).

/// Размер входного изображения модели.
pub const INPUT_SIZE: usize = 256;

/// Сколько якорей отдаёт модель.
pub const NUM_BOXES: usize = 2944;

/// Сколько чисел приходится на один якорь.
pub const NUM_COORDS: usize = 18;

/// Сколько ключевых точек ладони отдаёт детектор.
pub const NUM_KEYPOINTS: usize = 7;

/// Головки модели: шаг сетки и число якорей на клетку.
const HEADS: [(usize, usize); 3] = [(8, 2), (16, 2), (32, 6)];

/// Один разобранный якорь: оценка, рамка и точки ладони в долях кадра.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Уверенность после сигмоиды, 0…1.
    pub score: f32,
    /// Центр рамки в долях кадра.
    pub center: (f32, f32),
    /// Размер рамки в долях кадра.
    pub size: (f32, f32),
    /// Семь ключевых точек ладони в долях кадра.
    pub keypoints: [(f32, f32); NUM_KEYPOINTS],
}

/// Якоря модели в порядке её выхода.
///
/// Порядок задаёт сама модель: в каждой головке идут клетки сверху вниз и
/// слева направо, а внутри клетки — якоря подряд.
pub fn anchors() -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(NUM_BOXES);
    for (stride, per_cell) in HEADS {
        let cells = INPUT_SIZE / stride;
        for y in 0..cells {
            for x in 0..cells {
                let cx = (x as f32 + 0.5) / cells as f32;
                let cy = (y as f32 + 0.5) / cells as f32;
                for _ in 0..per_cell {
                    out.push((cx, cy));
                }
            }
        }
    }
    out
}

/// Сигмоида, устойчивая к переполнению.
pub fn sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

/// Разбирает выход модели в якоря.
///
/// Смещения рамки и точек заданы в пикселях входа (256), поэтому делятся на
/// [`INPUT_SIZE`] и прибавляются к центру якоря.
pub fn decode(scores: &[f32], boxes: &[f32], anchors: &[(f32, f32)]) -> Vec<Detection> {
    let scale = INPUT_SIZE as f32;
    let mut out = Vec::with_capacity(anchors.len());
    for (index, &(ax, ay)) in anchors.iter().enumerate() {
        let base = index * NUM_COORDS;
        let Some(raw) = boxes.get(base..base + NUM_COORDS) else {
            break;
        };
        let mut keypoints = [(0.0f32, 0.0f32); NUM_KEYPOINTS];
        for (j, point) in keypoints.iter_mut().enumerate() {
            point.0 = raw[4 + 2 * j] / scale + ax;
            point.1 = raw[5 + 2 * j] / scale + ay;
        }
        out.push(Detection {
            score: sigmoid(scores.get(index).copied().unwrap_or(f32::NEG_INFINITY)),
            center: (raw[0] / scale + ax, raw[1] / scale + ay),
            size: (raw[2] / scale, raw[3] / scale),
            keypoints,
        });
    }
    out
}

/// Отбирает лучшие рамки: порог по оценке, подавление пересечений, лимит.
pub fn select(
    mut detections: Vec<Detection>,
    score_threshold: f32,
    iou_threshold: f32,
    max_hands: usize,
) -> Vec<Detection> {
    detections.retain(|det| det.score >= score_threshold);
    detections.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for det in detections {
        if kept.iter().all(|k| iou(k, &det) < iou_threshold) {
            kept.push(det);
        }
    }
    kept.truncate(max_hands);
    kept
}

/// Доля пересечения двух рамок.
fn iou(a: &Detection, b: &Detection) -> f32 {
    let (ax0, ay0, ax1, ay1) = corners(a);
    let (bx0, by0, bx1, by1) = corners(b);
    let inter = (ax1.min(bx1) - ax0.max(bx0)).max(0.0) * (ay1.min(by1) - ay0.max(by0)).max(0.0);
    let area_a = (ax1 - ax0) * (ay1 - ay0);
    let area_b = (bx1 - bx0) * (by1 - by0);
    let union = area_a + area_b - inter;
    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

/// Углы рамки: слева-сверху и справа-снизу.
fn corners(d: &Detection) -> (f32, f32, f32, f32) {
    (
        d.center.0 - d.size.0 / 2.0,
        d.center.1 - d.size.1 / 2.0,
        d.center.0 + d.size.0 / 2.0,
        d.center.1 + d.size.1 / 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_match_model_output_size() {
        let anchors = anchors();
        assert_eq!(anchors.len(), NUM_BOXES);
        // Первая клетка головки шага 8: центр (0.5/32, 0.5/32), два якоря.
        let first = anchors[0];
        assert!((first.0 - 0.5 / 32.0).abs() < 1e-6);
        assert!((first.1 - 0.5 / 32.0).abs() < 1e-6);
        assert_eq!(anchors[0], anchors[1]);
        assert_ne!(anchors[1], anchors[2]);
    }

    #[test]
    fn sigmoid_is_bounded_and_centered() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(50.0) > 0.999);
        assert!(sigmoid(-50.0) < 1e-3);
        // Большие по модулю значения не дают NaN.
        assert!(sigmoid(-1000.0).is_finite());
        assert!(sigmoid(1000.0).is_finite());
    }

    #[test]
    fn decode_adds_anchor_offset() {
        let anchors = vec![(0.5, 0.25)];
        let scores = vec![0.0];
        let mut boxes = vec![0.0; NUM_COORDS];
        // Рамка совпадает с якорем, размер — шестая часть входа.
        boxes[2] = 128.0;
        boxes[3] = 64.0;
        let det = decode(&scores, &boxes, &anchors);
        assert_eq!(det.len(), 1);
        assert!((det[0].score - 0.5).abs() < 1e-6);
        assert!((det[0].center.0 - 0.5).abs() < 1e-6);
        assert!((det[0].center.1 - 0.25).abs() < 1e-6);
        assert!((det[0].size.0 - 0.5).abs() < 1e-6);
        assert!((det[0].size.1 - 0.25).abs() < 1e-6);
        assert!((det[0].keypoints[0].0 - 0.5).abs() < 1e-6);
    }

    #[test]
    fn select_keeps_best_and_drops_overlap() {
        let det = |score: f32, cx: f32| Detection {
            score,
            center: (cx, 0.5),
            size: (0.2, 0.2),
            keypoints: [(0.0, 0.0); NUM_KEYPOINTS],
        };
        // Две почти одинаковые рамки и одна далёкая.
        let picked = select(
            vec![det(0.9, 0.5), det(0.8, 0.51), det(0.7, 0.1)],
            0.5,
            0.3,
            2,
        );
        assert_eq!(picked.len(), 2);
        assert!((picked[0].center.0 - 0.5).abs() < 1e-6);
        assert!((picked[1].center.0 - 0.1).abs() < 1e-6);
    }

    #[test]
    fn select_respects_score_threshold() {
        let det = |score: f32| Detection {
            score,
            center: (0.5, 0.5),
            size: (0.2, 0.2),
            keypoints: [(0.0, 0.0); NUM_KEYPOINTS],
        };
        let picked = select(vec![det(0.4), det(0.2)], 0.5, 0.3, 2);
        assert!(picked.is_empty());
    }
}
