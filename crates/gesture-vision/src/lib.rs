//! Слой зрения: кадр камеры → точки кистей.
//!
//! Крейт не содержит детектора: он задаёт контракт
//! [`LandmarkSource`] и приводит его вывод к геометрии ядра. Так ядро
//! остаётся проверяемым тестами, а выбор и замена детектора (MediaPipe,
//! OpenVINO, RTMPose, ONNX через `ort`) не трогает распознавание.
//!
//! Захват камеры вынесен отдельно в [`camera`]: там нет модели, только
//! кадры. Источник кадров и источник точек — разные контракты, потому
//! что камера работает всегда, а модель появится позже.
//!
//! # Подключение детектора
//!
//! ```
//! use gesture_core::Point;
//! use gesture_vision::{HandLandmarks, LandmarkSource};
//!
//! /// Заглушка вместо настоящей модели.
//! struct DemoDetector;
//!
//! impl LandmarkSource for DemoDetector {
//!     fn detect(&mut self) -> Result<Vec<HandLandmarks>, gesture_core::VisionError> {
//!         Ok(Vec::new())
//!     }
//! }
//!
//! let mut detector = DemoDetector;
//! assert!(detector.detect().unwrap().is_empty());
//! # let _ = Point::new(0.0, 0.0);
//! ```

use gesture_core::{joint, HandGeometry, Point, VisionError};

/// Точки одной кисти в пикселях кадра.
///
/// Порядок точек — [`joint`]. Детектор может вернуть не 21 точку: такую
/// кисть [`HandLandmarks::geometry`] отбросит.
#[derive(Debug, Clone, PartialEq)]
pub struct HandLandmarks {
    /// Точки в пикселях исходного кадра.
    pub points: Vec<Point>,
    /// Уверенность детектора в диапазоне 0…1.
    pub score: f32,
}

impl HandLandmarks {
    /// Кисть из готового среза точек.
    pub fn new(points: Vec<Point>, score: f32) -> Self {
        HandLandmarks { points, score }
    }

    /// Геометрия кисти для ядра.
    ///
    /// `None`, если точек не 21, часть ладони вне кадра или ладонь слишком
    /// мала: рука слишком далеко и жест не распознать.
    pub fn geometry(&self) -> Option<HandGeometry> {
        if self.points.len() != joint::COUNT {
            return None;
        }
        HandGeometry::new(&self.points)
    }

    /// Геометрия с явной ошибкой вместо молчаливого отбрасывания.
    ///
    /// Нужен интерфейсу, который показывает пользователю, что произошло.
    pub fn try_geometry(&self) -> Result<HandGeometry, VisionError> {
        if self.points.len() != joint::COUNT {
            return Err(VisionError::HandShape {
                expected: joint::COUNT,
                actual: self.points.len(),
            });
        }
        HandGeometry::new(&self.points).ok_or(VisionError::NoPoints)
    }

    /// Кисть в зеркальном виде — так, как её видит пользователь.
    ///
    /// Камера смотрит на пользователя, поэтому в превью он ожидает движение
    /// влево при движении своей руки влево, а не наоборот.
    pub fn mirrored(&self, frame_width: f32) -> HandLandmarks {
        HandLandmarks {
            points: self
                .points
                .iter()
                .map(|p| mirror_x(*p, frame_width))
                .collect(),
            score: self.score,
        }
    }

    /// Кисть в координатах кадра: из нормализованных координат детектора.
    ///
    /// Многие детекторы отдают точки в диапазоне 0…1; ядру нужны пиксели,
    /// потому что пороги заданы размером ладони в пикселях.
    pub fn from_normalized(
        points: impl IntoIterator<Item = (f32, f32)>,
        width: u32,
        height: u32,
        score: f32,
    ) -> Result<Self, VisionError> {
        if width == 0 || height == 0 {
            return Err(VisionError::EmptyFrame { width, height });
        }
        let points = points
            .into_iter()
            .map(|(x, y)| Point::new(x * width as f32, y * height as f32))
            .collect();
        Ok(HandLandmarks { points, score })
    }
}

/// Отражает точку по вертикальной оси кадра.
fn mirror_x(point: Point, frame_width: f32) -> Point {
    Point::new(frame_width - point.x, point.y)
}

/// Кадр с результатом детекции.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// Момент кадра в секундах от запуска: нужен автомату для расчёта скорости.
    pub timestamp: f32,
    /// Кисти в порядке убывания уверенности.
    pub hands: Vec<HandLandmarks>,
}

impl Frame {
    /// Кадр без кистей.
    pub fn empty(width: u32, height: u32, timestamp: f32) -> Self {
        Frame {
            width,
            height,
            timestamp,
            hands: Vec::new(),
        }
    }

    /// Геометрия всех пригодных кистей: битые и слишком мелкие отброшены.
    pub fn geometries(&self) -> Vec<HandGeometry> {
        self.hands
            .iter()
            .filter_map(HandLandmarks::geometry)
            .collect()
    }

    /// Геометрия с зеркалированием — для превью и жестов управления.
    pub fn mirrored_geometries(&self) -> Vec<HandGeometry> {
        self.hands
            .iter()
            .filter_map(|hand| hand.mirrored(self.width as f32).geometry())
            .collect()
    }
}

/// Источник точек кистей.
///
/// Реализация оборачивает конкретную модель детекции. Ядро об этом не знает.
pub trait LandmarkSource {
    /// Находит кисти в очередном кадре.
    fn detect(&mut self) -> Result<Vec<HandLandmarks>, VisionError>;

    /// Размер кадра, ожидаемый детектором.
    ///
    /// Нужен источникам, работающим с нормализованными координатами.
    fn frame_size(&self) -> (u32, u32) {
        (640, 480)
    }
}

/// Источник, который не находит ничего.
///
/// Подходит для запуска интерфейса без камеры и для тестов: жесты просто
/// не появляются, а приложение остаётся рабочим.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoLandmarks;

impl LandmarkSource for NoLandmarks {
    fn detect(&mut self) -> Result<Vec<HandLandmarks>, VisionError> {
        Ok(Vec::new())
    }
}

/// Источник, отдающий заранее заданные кадры.
///
/// Позволяет прогонять конвейер целиком в тестах без камеры и модели.
#[derive(Debug, Default, Clone)]
pub struct ScriptedLandmarks {
    frames: std::collections::VecDeque<Vec<HandLandmarks>>,
    size: (u32, u32),
}

impl ScriptedLandmarks {
    pub fn new(size: (u32, u32)) -> Self {
        ScriptedLandmarks {
            frames: std::collections::VecDeque::new(),
            size,
        }
    }

    /// Добавляет кадр в очередь воспроизведения.
    pub fn push(&mut self, hands: Vec<HandLandmarks>) -> &mut Self {
        self.frames.push_back(hands);
        self
    }

    /// Осталось ли что воспроизводить.
    pub fn has_frames(&self) -> bool {
        !self.frames.is_empty()
    }
}

impl LandmarkSource for ScriptedLandmarks {
    fn detect(&mut self) -> Result<Vec<HandLandmarks>, VisionError> {
        Ok(self.frames.pop_front().unwrap_or_default())
    }

    fn frame_size(&self) -> (u32, u32) {
        self.size
    }
}

pub mod camera;

pub use camera::{CameraConfig, FfmpegCamera, FrameSource, RgbFrame};

#[cfg(test)]
mod tests {
    use super::*;

    /// Кисть с центром ладони в точке, остальные точки не важны.
    ///
    /// Смещения точек ладони подобраны так, чтобы их среднее совпало с
    /// `(x, y)`: именно среднее ладони становится центром `HandGeometry`.
    fn hand(x: f32, y: f32) -> HandLandmarks {
        let mut points = vec![Point::new(x, y - 28.0); joint::COUNT];
        points[joint::WRIST] = Point::new(x, y + 72.0);
        points[joint::MIDDLE_MCP] = Point::new(x, y + 12.0);
        HandLandmarks::new(points, 0.9)
    }

    #[test]
    fn geometry_requires_exact_point_count() {
        let short = HandLandmarks::new(vec![Point::new(10.0, 10.0); 20], 0.9);
        assert!(short.geometry().is_none());
        assert!(matches!(
            short.try_geometry(),
            Err(VisionError::HandShape {
                expected: 21,
                actual: 20
            })
        ));
    }

    #[test]
    fn geometry_rejects_tiny_hand() {
        // Ладонь 5 px: рука слишком далеко.
        let mut points = vec![Point::new(100.0, 100.0); joint::COUNT];
        points[joint::WRIST] = Point::new(100.0, 100.0);
        points[joint::MIDDLE_MCP] = Point::new(100.0, 95.0);
        let tiny = HandLandmarks::new(points, 0.9);
        assert!(tiny.geometry().is_none());
        assert!(matches!(tiny.try_geometry(), Err(VisionError::NoPoints)));
    }

    #[test]
    fn normalized_coordinates_scale_to_pixels() {
        let raw: Vec<(f32, f32)> = vec![(0.5, 0.5); joint::COUNT];
        let hands = HandLandmarks::from_normalized(raw, 640, 480, 0.8).unwrap();
        assert_eq!(hands.points[0], Point::new(320.0, 240.0));
    }

    #[test]
    fn empty_frame_size_rejected() {
        let raw: Vec<(f32, f32)> = vec![(0.5, 0.5); joint::COUNT];
        assert!(matches!(
            HandLandmarks::from_normalized(raw, 0, 480, 0.8),
            Err(VisionError::EmptyFrame {
                width: 0,
                height: 480
            })
        ));
    }

    #[test]
    fn mirroring_flips_x_only() {
        let mirrored = hand(100.0, 200.0).mirrored(640.0);
        let geometry = mirrored.geometry().unwrap();
        assert!((geometry.center.x - 540.0).abs() < 0.01);
        assert!((geometry.center.y - 200.0).abs() < 0.01);
    }

    #[test]
    fn frame_keeps_only_valid_hands() {
        let broken = HandLandmarks::new(vec![Point::new(1.0, 1.0); 3], 0.9);
        let frame = Frame {
            width: 640,
            height: 480,
            timestamp: 0.0,
            hands: vec![broken, hand(100.0, 100.0)],
        };
        let geometries = frame.geometries();
        assert_eq!(geometries.len(), 1);
        assert!((geometries[0].center.x - 100.0).abs() < 0.01);
    }

    #[test]
    fn no_landmarks_source_is_quiet() {
        let mut source = NoLandmarks;
        assert!(source.detect().unwrap().is_empty());
        assert_eq!(source.frame_size(), (640, 480));
    }

    #[test]
    fn scripted_source_replays_then_drains() {
        let mut source = ScriptedLandmarks::new((320, 240));
        source.push(vec![hand(50.0, 50.0)]);
        assert!(source.has_frames());
        assert_eq!(source.detect().unwrap().len(), 1);
        assert!(source.detect().unwrap().is_empty());
        assert!(!source.has_frames());
    }
}
