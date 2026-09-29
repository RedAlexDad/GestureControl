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
            crop_scale: 2.0,
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
        let (scores, boxes) = self.run_palm(sample(frame, full, INPUT_SIZE))?;
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
            let rect = palm_crop(&detection, frame, crop_scale);
            let input = sample(frame, rect, INPUT_SIZE);
            let landmarks = self.run_landmark(input)?;
            hands.push(landmarks_to_hand(&landmarks, rect, detection.score));
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

/// Кроп кисти по найденной ладони: квадрат вокруг центра рамки.
fn palm_crop(detection: &Detection, frame: &RgbFrame, scale: f32) -> (f32, f32, f32, f32) {
    let width = frame.width as f32;
    let height = frame.height as f32;
    let min_side = width.min(height);
    let side = (scale * detection.size.0.max(detection.size.1) * min_side).clamp(24.0, min_side);
    let cx = detection.center.0 * width;
    let cy = detection.center.1 * height;
    let x0 = (cx - side / 2.0).clamp(0.0, (width - side).max(0.0));
    let y0 = (cy - side / 2.0).clamp(0.0, (height - side).max(0.0));
    (x0, y0, side, side)
}

/// Переводит ключевые точки модели в пиксели кадра.
///
/// Точки модели заданы в пикселях входа (256) относительно кропа, поэтому
/// делятся на [`INPUT_SIZE`] и растягиваются на размер кропа.
fn landmarks_to_hand(landmarks: &[f32], rect: (f32, f32, f32, f32), score: f32) -> HandLandmarks {
    let (x0, y0, width, height) = rect;
    let scale = INPUT_SIZE as f32;
    let mut points = Vec::with_capacity(joint::COUNT);
    for index in 0..joint::COUNT {
        let x = x0 + landmarks[3 * index] / scale * width;
        let y = y0 + landmarks[3 * index + 1] / scale * height;
        points.push(Point::new(x, y));
    }
    HandLandmarks::new(points, score)
}

/// Билинейно масштабирует область кадра в квадрат `size` и нормирует в −1…1.
fn sample(frame: &RgbFrame, rect: (f32, f32, f32, f32), size: usize) -> Vec<f32> {
    let (x0, y0, rect_w, rect_h) = rect;
    let width = frame.width as i64;
    let height = frame.height as i64;
    let mut out = vec![0.0f32; size * size * 3];
    for ty in 0..size {
        let sy = y0 + (ty as f32 + 0.5) * rect_h / size as f32 - 0.5;
        let y = sy.floor();
        let fy = sy - y;
        let y0i = y as i64;
        for tx in 0..size {
            let sx = x0 + (tx as f32 + 0.5) * rect_w / size as f32 - 0.5;
            let x = sx.floor();
            let fx = sx - x;
            let x0i = x as i64;
            let base = (ty * size + tx) * 3;
            for channel in 0..3 {
                let v00 = pixel(frame, x0i, y0i, width, height, channel);
                let v10 = pixel(frame, x0i + 1, y0i, width, height, channel);
                let v01 = pixel(frame, x0i, y0i + 1, width, height, channel);
                let v11 = pixel(frame, x0i + 1, y0i + 1, width, height, channel);
                let top = v00 + (v10 - v00) * fx;
                let bottom = v01 + (v11 - v01) * fx;
                let value = top + (bottom - top) * fy;
                out[base + channel] = (value / 255.0 - 0.5) * 2.0;
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
