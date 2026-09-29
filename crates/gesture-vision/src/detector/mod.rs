//! Детектор кистей на ONNX Runtime.
//!
//! Две модели MediaPipe Hands: сначала детектор ладоней находит рамку кисти,
//! затем модель ключевых точек разбирает вырезанную кисть в 21 точку. Точки
//! возвращаются в пикселях исходного кадра — в том же виде, что ждёт ядро.
//!
//! ```text
//!   RgbFrame → ладонь → рамка → кроп 256x256 → 21 точка → HandLandmarks
//! ```

mod palm;

use std::path::{Path, PathBuf};

use gesture_core::{joint, Point, VisionError};
use ort::session::Session;
use ort::value::Tensor;

use crate::camera::RgbFrame;
use crate::HandLandmarks;

pub use palm::{Detection, INPUT_SIZE, NUM_BOXES};

/// Минимальная сторона рамки ладони в пикселях кадра.
///
/// Мелкие срабатывания детектора дают кисть размером в десяток пикселей:
/// ядро такую отбросит, но перед этим зря прогонит модель точек.
const MIN_PALM_PX: f32 = 24.0;

/// Насколько сдвинуть центр кропа от запястья к пальцам, в долях ладони.
///
/// Кроп привязан к запястью, а не к центру рамки: у рамки центр сидит на
/// ладони, и запястье с кончиками оказывались у краёв, из-за чего модель
/// занижала крайние точки. Смещение подобрано так, чтобы кисть заполняла
/// кроп, а запястье оставалось внутри.
const CROP_SHIFT: f32 = 0.8;

/// Источник точек по готовому кадру.
///
/// В отличие от [`crate::LandmarkSource`], который сам владеет потоком
/// кадров, этот источник получает кадр снаружи: так детектор работает на
/// том же кадре, что уходит в превью.
pub trait FrameLandmarkSource {
    /// Находит кисти в кадре.
    fn detect(&mut self, frame: &RgbFrame) -> Result<Vec<HandLandmarks>, VisionError>;
}

/// Настройки детектора.
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Путь к модели детектора ладоней.
    pub palm_model: PathBuf,
    /// Путь к модели ключевых точек.
    pub landmark_model: PathBuf,
    /// Порог уверенности детектора ладоней.
    pub score_threshold: f32,
    /// Порог пересечения рамок при подавлении.
    pub iou_threshold: f32,
    /// Сколько кистей искать в кадре.
    pub max_hands: usize,
    /// Во сколько раз рамка ладони расширяется до кропа кисти.
    pub crop_scale: f32,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        DetectorConfig {
            palm_model: PathBuf::from("models/palm_detection.onnx"),
            landmark_model: PathBuf::from("models/hand_landmark.onnx"),
            score_threshold: 0.5,
            iou_threshold: 0.3,
            max_hands: 2,
            // Кисть примерно вдвое длиннее ладони; кроп с запасом, чтобы
            // в него поместились и запястье, и кончики пальцев.
            crop_scale: 2.8,
        }
    }
}

/// Детектор кистей на двух ONNX-моделях.
pub struct OnnxHandDetector {
    palm: Session,
    landmark: Session,
    palm_input: String,
    landmark_input: String,
    anchors: Vec<(f32, f32)>,
    config: DetectorConfig,
}

impl std::fmt::Debug for OnnxHandDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxHandDetector")
            .field("palm_model", &self.config.palm_model)
            .field("landmark_model", &self.config.landmark_model)
            .field("score_threshold", &self.config.score_threshold)
            .field("max_hands", &self.config.max_hands)
            .finish_non_exhaustive()
    }
}

impl OnnxHandDetector {
    /// Загружает обе модели.
    pub fn load(config: DetectorConfig) -> Result<Self, VisionError> {
        let palm = build_session(&config.palm_model)?;
        let landmark = build_session(&config.landmark_model)?;
        let palm_input = input_name(&palm, &config.palm_model)?;
        let landmark_input = input_name(&landmark, &config.landmark_model)?;
        Ok(OnnxHandDetector {
            palm,
            landmark,
            palm_input,
            landmark_input,
            anchors: palm::anchors(),
            config,
        })
    }

    /// Ищет кисти в кадре.
    pub fn detect(&mut self, frame: &RgbFrame) -> Result<Vec<HandLandmarks>, VisionError> {
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return Err(VisionError::EmptyFrame {
                width: frame.width,
                height: frame.height,
            });
        }

        let full = (0.0, 0.0, frame.width as f32, frame.height as f32);
        let (scores, boxes) = self.run_palm(sample_axis(frame, full, INPUT_SIZE))?;
        let detections = palm::decode(&scores, &boxes, &self.anchors);
        let detections = palm::select(
            detections,
            self.config.score_threshold,
            self.config.iou_threshold,
            self.config.max_hands,
        );

        let crop_scale = self.config.crop_scale;
        let min_side = frame.width.min(frame.height) as f32;
        let mut hands = Vec::with_capacity(detections.len());
        for detection in detections {
            // Слишком мелкая рамка — не кисть, а шум детектора.
            if detection.size.0.max(detection.size.1) * min_side < MIN_PALM_PX {
                continue;
            }
            let crop = palm_crop(&detection, frame, crop_scale);
            let landmarks = self.run_landmark(sample_crop(frame, &crop, INPUT_SIZE))?;
            hands.push(landmarks_to_hand(&landmarks, &crop, detection.score));
        }
        Ok(hands)
    }

    /// Прогоняет детектор ладоней и возвращает оценки и рамки.
    fn run_palm(&mut self, input: Vec<f32>) -> Result<(Vec<f32>, Vec<f32>), VisionError> {
        let name = self.palm_input.clone();
        let tensor = Tensor::from_array((
            [1usize, INPUT_SIZE, INPUT_SIZE, 3],
            input.into_boxed_slice(),
        ))
        .map_err(|e| model_error(e.to_string()))?;
        let outputs = self
            .palm
            .run(ort::inputs![name => tensor])
            .map_err(|e| model_error(e.to_string()))?;
        let scores = extract(&outputs, "classificators")?;
        let boxes = extract(&outputs, "regressors")?;
        Ok((scores, boxes))
    }

    /// Прогоняет модель ключевых точек и возвращает 21 точку.
    fn run_landmark(&mut self, input: Vec<f32>) -> Result<Vec<f32>, VisionError> {
        let name = self.landmark_input.clone();
        let tensor = Tensor::from_array((
            [1usize, INPUT_SIZE, INPUT_SIZE, 3],
            input.into_boxed_slice(),
        ))
        .map_err(|e| model_error(e.to_string()))?;
        let outputs = self
            .landmark
            .run(ort::inputs![name => tensor])
            .map_err(|e| model_error(e.to_string()))?;
        extract(&outputs, "ld_21_3d")
    }
}

/// Имя первого входа модели.
fn input_name(session: &Session, path: &Path) -> Result<String, VisionError> {
    session
        .inputs()
        .first()
        .map(|input| input.name().to_string())
        .ok_or_else(|| model_error(format!("у модели {} нет входов", path.display())))
}

/// Копирует выход модели в вектор.
fn extract(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
) -> Result<Vec<f32>, VisionError> {
    outputs
        .get(name)
        .ok_or_else(|| model_error(format!("в модели нет выхода {name}")))?
        .try_extract_tensor::<f32>()
        .map(|(_, data)| data.to_vec())
        .map_err(|e| model_error(e.to_string()))
}

/// Создаёт сессию ONNX Runtime по файлу модели.
fn build_session(path: &Path) -> Result<Session, VisionError> {
    let mut builder =
        Session::builder().map_err(|e| model_error(format!("{}: {e}", path.display())))?;
    builder
        .commit_from_file(path)
        .map_err(|e| model_error(format!("{}: {e}", path.display())))
}

/// Ошибка детектора.
fn model_error(message: String) -> VisionError {
    VisionError::Model(message)
}

/// Кроп кисти, выровненный по ориентации ладони.
///
/// Оси заданы единичными векторами кадра: [`Crop::down`] идёт от среднего
/// пальца к запястью, [`Crop::right`] — поперёк ладони. Так рука в кропе
/// стоит вертикально, как ждёт модель, даже если в кадре она наклонена.
#[derive(Debug, Clone, Copy)]
struct Crop {
    /// Центр кропа в пикселях кадра.
    center: (f32, f32),
    /// Сторона квадрата в пикселях кадра.
    side: f32,
    /// Единичная ось «вправо» в координатах кадра.
    right: (f32, f32),
    /// Единичная ось «вниз» в координатах кадра.
    down: (f32, f32),
}

impl Crop {
    /// Пиксель кадра по локальным координатам кропа.
    ///
    /// `local` — смещения от центра вдоль осей `right` и `down`.
    fn to_frame(self, local: (f32, f32)) -> (f32, f32) {
        (
            self.center.0 + local.0 * self.right.0 + local.1 * self.down.0,
            self.center.1 + local.0 * self.right.1 + local.1 * self.down.1,
        )
    }
}

/// Кроп по найденной ладони, выровненный по её оси.
///
/// Раньше кроп был осевым квадратом вокруг рамки: у наклонённой кисти
/// кончики выпадали из кропа, а модель, обученная на вертикальной руке,
/// ошибалась именно на них. Ориентацию задают запястье и основание
/// среднего пальца, а размер — расстояние между ними.
fn palm_crop(detection: &Detection, frame: &RgbFrame, scale: f32) -> Crop {
    let width = frame.width as f32;
    let height = frame.height as f32;
    let wrist = keypoint(detection, 0, width, height);
    let middle = keypoint(detection, 2, width, height);

    let dx = middle.0 - wrist.0;
    let dy = middle.1 - wrist.1;
    let palm = (dx * dx + dy * dy).sqrt();
    // Ось ладони от среднего пальца к запястью: в кропе она смотрит вниз.
    let down = if palm > 1.0 {
        let inv = 1.0 / palm;
        ((wrist.0 - middle.0) * inv, (wrist.1 - middle.1) * inv)
    } else {
        (0.0, 1.0)
    };
    // Поперёк ладони — это «вниз», повёрнутое на прямой угол в экранных
    // координатах: (y, −x) даёт ось вправо при оси вниз.
    let right = (down.1, -down.0);
    let side = (scale * palm).clamp(MIN_PALM_PX, width.max(height));

    // Центр кропа — на оси ладони, чуть выше запястья к пальцам. Так
    // запястье остаётся у нижнего края, а кончики — у верхнего.
    let shift = CROP_SHIFT * palm;
    let center = (wrist.0 - shift * down.0, wrist.1 - shift * down.1);

    Crop {
        center,
        side,
        right,
        down,
    }
}

/// Ключевая точка ладони в пикселях кадра.
fn keypoint(detection: &Detection, index: usize, width: f32, height: f32) -> (f32, f32) {
    (
        detection.keypoints[index].0 * width,
        detection.keypoints[index].1 * height,
    )
}

/// Переводит ключевые точки модели в пиксели кадра.
///
/// Точки заданы в пикселях входа (256) относительно кропа, поэтому делятся
/// на [`INPUT_SIZE`] и раскладываются по осям кропа.
fn landmarks_to_hand(landmarks: &[f32], crop: &Crop, score: f32) -> HandLandmarks {
    let scale = INPUT_SIZE as f32;
    let half = crop.side / 2.0;
    let mut points = Vec::with_capacity(joint::COUNT);
    for index in 0..joint::COUNT {
        let local = (
            landmarks[3 * index] / scale * crop.side - half,
            landmarks[3 * index + 1] / scale * crop.side - half,
        );
        let (x, y) = crop.to_frame(local);
        points.push(Point::new(x, y));
    }
    HandLandmarks::new(points, score)
}

/// Осевой билинейный кроп: полный кадр для детектора ладоней.
fn sample_axis(frame: &RgbFrame, rect: (f32, f32, f32, f32), size: usize) -> Vec<f32> {
    let (x0, y0, rect_w, rect_h) = rect;
    let step_x = rect_w / size as f32;
    let step_y = rect_h / size as f32;
    fill(frame, size, |tx, ty| {
        (
            x0 + (tx + 0.5) * step_x - 0.5,
            y0 + (ty + 0.5) * step_y - 0.5,
        )
    })
}

/// Повёрнутый кроп кисти: пиксели берутся вдоль осей ладони.
fn sample_crop(frame: &RgbFrame, crop: &Crop, size: usize) -> Vec<f32> {
    let step = crop.side / size as f32;
    let half = crop.side / 2.0;
    fill(frame, size, |tx, ty| {
        crop.to_frame(((tx + 0.5) * step - half, (ty + 0.5) * step - half))
    })
}

/// Заполняет выход размером `size` пикселями кадра по функции выборки.
///
/// Функция `at` принимает индексы выхода `u`, `v` и возвращает координаты
/// кадра; билинейная выборка и нормировка в 0…1 общие для обоих кропов.
/// Модели ждут именно 0…1: с −1…1 головы отдавали сжатый скелет, а признак
/// кисти падал до нуля.
fn fill<F>(frame: &RgbFrame, size: usize, at: F) -> Vec<f32>
where
    F: Fn(f32, f32) -> (f32, f32),
{
    let width = frame.width as i64;
    let height = frame.height as i64;
    let mut out = vec![0.0f32; size * size * 3];
    for ty in 0..size {
        for tx in 0..size {
            let (sx, sy) = at(tx as f32, ty as f32);
            let x = sx.floor();
            let y = sy.floor();
            let fx = sx - x;
            let fy = sy - y;
            let x0i = x as i64;
            let y0i = y as i64;
            let base = (ty * size + tx) * 3;
            for channel in 0..3 {
                let v00 = pixel(frame, x0i, y0i, width, height, channel);
                let v10 = pixel(frame, x0i + 1, y0i, width, height, channel);
                let v01 = pixel(frame, x0i, y0i + 1, width, height, channel);
                let v11 = pixel(frame, x0i + 1, y0i + 1, width, height, channel);
                let top = v00 + (v10 - v00) * fx;
                let bottom = v01 + (v11 - v01) * fx;
                let value = top + (bottom - top) * fy;
                out[base + channel] = value / 255.0;
            }
        }
    }
    out
}

/// Пиксель кадра с ограничением координат краями.
fn pixel(frame: &RgbFrame, x: i64, y: i64, width: i64, height: i64, channel: usize) -> f32 {
    let x = x.clamp(0, width - 1) as usize;
    let y = y.clamp(0, height - 1) as usize;
    frame.pixels[(y * frame.width as usize + x) * 3 + channel] as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Кроп без поворота: оси совпадают с осями кадра.
    fn crop() -> Crop {
        Crop {
            center: (100.0, 100.0),
            side: 200.0,
            right: (1.0, 0.0),
            down: (0.0, 1.0),
        }
    }

    #[test]
    fn crop_maps_center_and_corners() {
        let crop = crop();
        assert_eq!(crop.to_frame((0.0, 0.0)), (100.0, 100.0));
        assert_eq!(crop.to_frame((-100.0, -100.0)), (0.0, 0.0));
        assert_eq!(crop.to_frame((100.0, 100.0)), (200.0, 200.0));
    }

    #[test]
    fn landmark_at_input_center_maps_to_crop_center() {
        let crop = crop();
        let mut landmarks = vec![0.0f32; joint::COUNT * 3];
        for index in 0..joint::COUNT {
            landmarks[3 * index] = INPUT_SIZE as f32 / 2.0;
            landmarks[3 * index + 1] = INPUT_SIZE as f32 / 2.0;
        }
        let hand = landmarks_to_hand(&landmarks, &crop, 1.0);
        for point in &hand.points {
            assert!((point.x - 100.0).abs() < 0.5, "x = {}", point.x);
            assert!((point.y - 100.0).abs() < 0.5, "y = {}", point.y);
        }
    }

    #[test]
    fn preprocessing_normalizes_to_zero_one() {
        // Модели ждут 0…1; с −1…1 модель точек отдавала сжатый скелет.
        let black = RgbFrame::new(2, 2, vec![0; 2 * 2 * 3]).expect("кадр");
        let white = RgbFrame::new(2, 2, vec![255; 2 * 2 * 3]).expect("кадр");
        let rect = (0.0, 0.0, 2.0, 2.0);
        assert!(sample_axis(&black, rect, 1)[0].abs() < 1e-6);
        assert!((sample_axis(&white, rect, 1)[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn palm_crop_centers_above_wrist() {
        // Запястье снизу, основание среднего пальца сверху: ладонь 20 px,
        // значит центр должен сместиться на 0.8·20 = 16 px от запястья к
        // пальцам.
        let mut detection = Detection {
            score: 1.0,
            center: (0.5, 0.5),
            size: (0.2, 0.2),
            keypoints: [(0.5, 0.5); palm::NUM_KEYPOINTS],
        };
        detection.keypoints[0] = (0.5, 0.6);
        detection.keypoints[2] = (0.5, 0.4);
        let frame = RgbFrame::new(100, 100, vec![0; 100 * 100 * 3]).expect("кадр");
        let crop = palm_crop(&detection, &frame, 2.0);
        assert!(
            (crop.center.0 - 50.0).abs() < 1e-3,
            "cx = {}",
            crop.center.0
        );
        assert!(
            (crop.center.1 - 44.0).abs() < 1e-3,
            "cy = {}",
            crop.center.1
        );
        assert!((crop.side - 40.0).abs() < 1e-3, "side = {}", crop.side);
    }

    #[test]
    fn palm_crop_orients_upright_hand() {
        // Запястье ниже основания среднего пальца: ось ладони смотрит вниз,
        // а поперёк ладони — вправо.
        let mut detection = Detection {
            score: 1.0,
            center: (0.5, 0.5),
            size: (0.2, 0.2),
            keypoints: [(0.5, 0.5); palm::NUM_KEYPOINTS],
        };
        detection.keypoints[0] = (0.5, 0.6);
        detection.keypoints[2] = (0.5, 0.4);
        let frame = RgbFrame::new(100, 100, vec![0; 100 * 100 * 3]).expect("кадр");
        let crop = palm_crop(&detection, &frame, 2.0);
        assert!(crop.down.0.abs() < 1e-6, "down.x = {}", crop.down.0);
        assert!((crop.down.1 - 1.0).abs() < 1e-6, "down.y = {}", crop.down.1);
        assert!(
            (crop.right.0 - 1.0).abs() < 1e-6,
            "right.x = {}",
            crop.right.0
        );
        assert!(crop.right.1.abs() < 1e-6, "right.y = {}", crop.right.1);
    }
}
