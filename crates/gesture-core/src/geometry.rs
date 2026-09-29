//! Геометрия кисти и правила распознавания встроенных жестов.
//!
//! Перенос `HandGeometry` из `GestureRecognizer.swift`. Все пороги заданы
//! относительно размера ладони, поэтому распознавание не зависит от длины
//! пальцев, размера руки и расстояния до камеры.

use crate::joint;
use crate::models::Gesture;

/// Точка в плоскости кадра. Ось `y` направлена вниз, как в системе координат
/// изображения; отрицательные координаты означают «точка не найдена».
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const MISSING: Point = Point { x: -1.0, y: -1.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Point { x, y }
    }

    /// Точка найдена детектором.
    pub fn is_valid(&self) -> bool {
        self.x >= 0.0
    }
}

pub fn dist(a: Point, b: Point) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Геометрия кисти в экранных координатах.
#[derive(Debug, Clone)]
pub struct HandGeometry {
    /// 21 точка в порядке [`joint`].
    pub p: [Point; joint::COUNT],
    /// Размер ладони: расстояние от запястья до основания среднего пальца.
    pub size: f32,
    /// Центр ладони: используется для отслеживания движения.
    pub center: Point,
}

impl HandGeometry {
    /// `None`, если точек не 21, часть ладони не найдена или ладонь слишком
    /// мала в кадре (рука слишком далеко).
    pub fn new(points: &[Point]) -> Option<Self> {
        if points.len() != joint::COUNT {
            return None;
        }
        if !joint::PALM.iter().all(|&i| points[i].x >= 0.0) {
            return None;
        }

        let size = dist(points[joint::WRIST], points[joint::MIDDLE_MCP]);
        if size <= 10.0 {
            return None;
        }

        let mut p = [Point::MISSING; joint::COUNT];
        p.copy_from_slice(points);

        let n = joint::PALM.len() as f32;
        let sum_x: f32 = joint::PALM.iter().map(|&i| p[i].x).sum();
        let sum_y: f32 = joint::PALM.iter().map(|&i| p[i].y).sum();
        let center = Point::new(sum_x / n, sum_y / n);

        Some(HandGeometry { p, size, center })
    }

    /// Палец выпрямлен, если его кончик заметно дальше от запястья, чем средний сустав.
    /// `None` — точки не найдены.
    fn is_finger_extended(&self, tip: usize, pip: usize) -> Option<bool> {
        if !self.p[tip].is_valid() || !self.p[pip].is_valid() {
            return None;
        }
        let wrist = self.p[joint::WRIST];
        Some(dist(wrist, self.p[tip]) > dist(wrist, self.p[pip]) * 1.2)
    }

    /// Большой палец отведён, если его кончик далеко от основания указательного.
    fn is_thumb_extended(&self) -> bool {
        if !self.p[joint::THUMB_TIP].is_valid() {
            return false;
        }
        dist(self.p[joint::THUMB_TIP], self.p[joint::INDEX_MCP]) > self.size * 0.65
    }

    /// Кончики большого и указательного пальцев соединены в кольцо.
    fn is_thumb_index_ring(&self) -> bool {
        if !self.p[joint::THUMB_TIP].is_valid() || !self.p[joint::INDEX_TIP].is_valid() {
            return false;
        }
        dist(self.p[joint::THUMB_TIP], self.p[joint::INDEX_TIP]) < self.size * 0.35
    }

    /// Статический жест по положению кисти.
    ///
    /// Порядок проверок значим: более специфичные жесты проверяются раньше
    /// (`ok` до `open_palm`, `call_me` до `fist`).
    pub fn static_gesture(&self) -> Gesture {
        let (Some(index), Some(middle), Some(ring), Some(little)) = (
            self.is_finger_extended(joint::INDEX_TIP, joint::INDEX_PIP),
            self.is_finger_extended(joint::MIDDLE_TIP, joint::MIDDLE_PIP),
            self.is_finger_extended(joint::RING_TIP, joint::RING_PIP),
            self.is_finger_extended(joint::LITTLE_TIP, joint::LITTLE_PIP),
        ) else {
            return Gesture::Idle;
        };

        if self.is_thumb_index_ring() && middle && ring && little {
            return Gesture::Ok;
        }
        if index && middle && ring && little {
            return Gesture::OpenPalm;
        }
        if index && middle && !ring && !little {
            return Gesture::Victory;
        }
        if index && !middle && !ring && !little {
            return Gesture::Pointing;
        }
        if !index && !middle && !ring && little && self.is_thumb_extended() {
            return Gesture::CallMe;
        }
        if !index && !middle && !ring && !little {
            // Поднятый большой палец: кончик заметно выше верхней грани
            // кулака. Проверять отведение большого пальца здесь не нужно —
            // при вертикальном «лайке» кончик прилегает к основанию
            // указательного и выглядит неотведённым.
            let top = self.p[joint::INDEX_MCP].y.min(self.p[joint::WRIST].y);
            if self.p[joint::THUMB_TIP].is_valid()
                && self.p[joint::THUMB_TIP].y < top - self.size * 0.3
            {
                return Gesture::ThumbsUp;
            }
            if !self.is_thumb_extended() {
                return Gesture::Fist;
            }
        }
        Gesture::Idle
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Раскладка «все пальцы сжаты в кулак»: четыре сгиба у запястья,
    /// большой палец прижат к ладони.
    fn fist() -> [Point; joint::COUNT] {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(100.0, 200.0);
        // Основания пальцев.
        p[joint::INDEX_MCP] = Point::new(80.0, 150.0);
        p[joint::MIDDLE_MCP] = Point::new(100.0, 140.0);
        p[joint::RING_MCP] = Point::new(120.0, 150.0);
        p[joint::LITTLE_MCP] = Point::new(135.0, 165.0);
        // Суставы и кончики сгибаются обратно к запястью.
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
            p[pip] = Point::new(base.x - 8.0, base.y - 5.0);
            p[dip] = Point::new(base.x - 2.0, base.y + 5.0);
            p[tip] = Point::new(base.x + 4.0, base.y + 12.0);
        }
        // Большой палец прижат: кончик у основания указательного.
        p[joint::THUMB_CMC] = Point::new(85.0, 190.0);
        p[joint::THUMB_MP] = Point::new(80.0, 175.0);
        p[joint::THUMB_IP] = Point::new(78.0, 165.0);
        p[joint::THUMB_TIP] = Point::new(85.0, 158.0);
        p
    }

    /// Раскладка «открытая ладонь»: все пальцы выпрямлены вверх.
    fn open_palm() -> [Point; joint::COUNT] {
        let mut p = [Point::MISSING; joint::COUNT];
        p[joint::WRIST] = Point::new(100.0, 200.0);
        p[joint::INDEX_MCP] = Point::new(75.0, 155.0);
        p[joint::MIDDLE_MCP] = Point::new(100.0, 145.0);
        p[joint::RING_MCP] = Point::new(125.0, 155.0);
        p[joint::LITTLE_MCP] = Point::new(148.0, 170.0);
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
            p[pip] = Point::new(base.x, base.y - 15.0);
            p[dip] = Point::new(base.x, base.y - 25.0);
            p[tip] = Point::new(base.x, base.y - 35.0);
        }
        // Большой палец отведён в сторону.
        p[joint::THUMB_CMC] = Point::new(80.0, 195.0);
        p[joint::THUMB_MP] = Point::new(62.0, 190.0);
        p[joint::THUMB_IP] = Point::new(48.0, 183.0);
        p[joint::THUMB_TIP] = Point::new(38.0, 175.0);
        p
    }

    #[test]
    fn palm_size_and_center() {
        let hand = HandGeometry::new(&fist()).expect("кулак должен быть принят");
        // Расстояние от запястья (100, 200) до основания среднего (100, 140).
        assert!((hand.size - 60.0).abs() < 1e-4, "size = {}", hand.size);
        let cx = (100.0 + 80.0 + 100.0 + 120.0 + 135.0) / 5.0;
        let cy = (200.0 + 150.0 + 140.0 + 150.0 + 165.0) / 5.0;
        assert!((hand.center.x - cx).abs() < 1e-4);
        assert!((hand.center.y - cy).abs() < 1e-4);
    }

    #[test]
    fn rejects_wrong_point_count() {
        assert!(HandGeometry::new(&fist()[..20]).is_none());
    }

    #[test]
    fn rejects_missing_palm_point() {
        let mut p = fist();
        p[joint::RING_MCP] = Point::MISSING;
        assert!(HandGeometry::new(&p).is_none());
    }

    #[test]
    fn rejects_tiny_palm() {
        // Ладонь 5 px — рука слишком далеко от камеры.
        let mut p = fist();
        for &i in &joint::PALM {
            p[i] = Point::new(p[i].x / 20.0, p[i].y / 20.0);
        }
        assert!(HandGeometry::new(&p).is_none());
    }

    #[test]
    fn recognizes_fist() {
        let hand = HandGeometry::new(&fist()).unwrap();
        assert_eq!(hand.static_gesture(), Gesture::Fist);
    }

    #[test]
    fn recognizes_open_palm() {
        let hand = HandGeometry::new(&open_palm()).unwrap();
        assert_eq!(hand.static_gesture(), Gesture::OpenPalm);
    }

    #[test]
    fn recognition_is_scale_invariant() {
        let base = open_palm();
        // Та же ладонь, в 2.5 раза ближе к камере и сдвинутая.
        let scaled: Vec<Point> = base
            .iter()
            .map(|q| Point::new(500.0 + (q.x - 100.0) * 2.5, 400.0 + (q.y - 200.0) * 2.5))
            .collect();
        let hand = HandGeometry::new(&scaled).unwrap();
        assert_eq!(hand.static_gesture(), Gesture::OpenPalm);
    }

    #[test]
    fn recognizes_pointing() {
        let mut p = open_palm();
        // Сгибаем средний, безымянный и мизинец в кулак, оставляя указательный.
        for (mcp, pip, dip, tip) in [
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
            p[pip] = Point::new(base.x - 4.0, base.y + 6.0);
            p[dip] = Point::new(base.x + 2.0, base.y + 12.0);
            p[tip] = Point::new(base.x + 6.0, base.y + 16.0);
        }
        let hand = HandGeometry::new(&p).unwrap();
        assert_eq!(hand.static_gesture(), Gesture::Pointing);
    }

    #[test]
    fn recognizes_thumbs_up() {
        let mut p = fist();
        // Большой палец поднят вертикально: кончик выше кулака примерно
        // на половину ладони, как в настоящем «лайке».
        p[joint::THUMB_CMC] = Point::new(88.0, 195.0);
        p[joint::THUMB_MP] = Point::new(84.0, 170.0);
        p[joint::THUMB_IP] = Point::new(82.0, 145.0);
        p[joint::THUMB_TIP] = Point::new(80.0, 110.0);
        let hand = HandGeometry::new(&p).unwrap();
        assert_eq!(hand.static_gesture(), Gesture::ThumbsUp);
    }

    #[test]
    fn fist_is_not_confused_with_thumbs_up() {
        // В кулаке большой палец лежит поперёк ладони, ниже костяшек.
        let hand = HandGeometry::new(&fist()).unwrap();
        assert_eq!(hand.static_gesture(), Gesture::Fist);
    }

    #[test]
    fn vertical_thumb_beside_knuckles_is_thumbs_up() {
        // Кончик у самой оси указательного пальца: расстояние до
        // INDEX_MCP меньше порога отведения, но палец явно поднят.
        let mut p = fist();
        p[joint::THUMB_CMC] = Point::new(88.0, 195.0);
        p[joint::THUMB_MP] = Point::new(86.0, 170.0);
        p[joint::THUMB_IP] = Point::new(84.0, 145.0);
        p[joint::THUMB_TIP] = Point::new(82.0, 115.0);
        let hand = HandGeometry::new(&p).unwrap();
        assert!(
            !hand.is_thumb_extended(),
            "эта поза нужна как контрпример отведения"
        );
        assert_eq!(hand.static_gesture(), Gesture::ThumbsUp);
    }
}
