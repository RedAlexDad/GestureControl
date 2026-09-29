//! Детектор кистей на официальном MediaPipe Hand Landmarker.
//!
//! Тот же движок, что и в референсе по духу: MediaPipe Tasks сам находит
//! кисть и её 21 точку, без нашей двухступенчатой сборки из детектора
//! ладоней и кропа. Библиотека `libmediapipe.so` и модель `.task` лежат в
//! `models/`; ABI и структуры взяты из заголовков MediaPipe 0.10.35.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::time::Instant;

use gesture_core::{joint, Point, VisionError};
use libloading::Library;

use crate::camera::RgbFrame;
use crate::HandLandmarks;

/// Формат пикселей: RGB, три байта на точку.
const IMAGE_FORMAT_SRGB: c_int = 1;

/// Режим видео: MediaPipe отслеживает кисть между кадрами, а не ищет её
/// заново, поэтому скелет стоит ровнее и точки точнее.
const RUNNING_MODE_VIDEO: c_int = 2;

/// Код успеха `MpStatus`.
const MP_OK: c_int = 0;

/// Настройки детектора MediaPipe.
#[derive(Debug, Clone)]
pub struct MediaPipeConfig {
    /// Путь к `libmediapipe.so`.
    pub library: std::path::PathBuf,
    /// Путь к модели `hand_landmarker.task`.
    pub model: std::path::PathBuf,
    /// Сколько кистей искать.
    pub max_hands: usize,
    /// Порог обнаружения кисти.
    pub min_detection: f32,
    /// Порог присутствия кисти.
    pub min_presence: f32,
}

impl Default for MediaPipeConfig {
    fn default() -> Self {
        MediaPipeConfig {
            library: std::path::PathBuf::from("models/libmediapipe.so"),
            model: std::path::PathBuf::from("models/hand_landmarker.task"),
            max_hands: 2,
            min_detection: 0.5,
            min_presence: 0.5,
        }
    }
}

#[repr(C)]
struct BaseOptions {
    model_asset_buffer: *const c_char,
    model_asset_buffer_count: c_int,
    model_asset_path: *const c_char,
    delegate: c_int,
    host_environment: c_int,
    host_system: c_int,
    host_version: *const c_char,
    ca_bundle_path: *const c_char,
}

type ResultCallback = Option<extern "C" fn(c_int, *const HandLandmarkerResult, *mut c_void, i64)>;

#[repr(C)]
struct HandLandmarkerOptions {
    base_options: BaseOptions,
    running_mode: c_int,
    num_hands: c_int,
    min_hand_detection_confidence: f32,
    min_hand_presence_confidence: f32,
    min_tracking_confidence: f32,
    result_callback: ResultCallback,
}

#[repr(C)]
struct NormalizedLandmark {
    x: f32,
    y: f32,
    z: f32,
    has_visibility: bool,
    visibility: f32,
    has_presence: bool,
    presence: f32,
    name: *mut c_char,
}

#[repr(C)]
struct NormalizedLandmarks {
    landmarks: *mut NormalizedLandmark,
    landmarks_count: u32,
}

#[repr(C)]
struct Category {
    index: c_int,
    score: f32,
    category_name: *mut c_char,
    display_name: *mut c_char,
}

#[repr(C)]
struct Categories {
    categories: *mut Category,
    categories_count: u32,
}

#[repr(C)]
struct Landmark {
    x: f32,
    y: f32,
    z: f32,
    has_visibility: bool,
    visibility: f32,
    has_presence: bool,
    presence: f32,
    name: *mut c_char,
}

#[repr(C)]
struct Landmarks {
    landmarks: *mut Landmark,
    landmarks_count: u32,
}

#[repr(C)]
struct HandLandmarkerResult {
    handedness: *mut Categories,
    handedness_count: u32,
    hand_landmarks: *mut NormalizedLandmarks,
    hand_landmarks_count: u32,
    hand_world_landmarks: *mut Landmarks,
    hand_world_landmarks_count: u32,
}

type CreateFn =
    unsafe extern "C" fn(*mut HandLandmarkerOptions, *mut *mut c_void, *mut *mut c_char) -> c_int;
type DetectVideoFn = unsafe extern "C" fn(
    *mut c_void,
    *mut c_void,
    *const c_void,
    i64,
    *mut HandLandmarkerResult,
    *mut *mut c_char,
) -> c_int;
type CloseFn = unsafe extern "C" fn(*mut c_void, *mut *mut c_char) -> c_int;
type CloseResultFn = unsafe extern "C" fn(*mut HandLandmarkerResult);
type ImageCreateFn = unsafe extern "C" fn(
    c_int,
    c_int,
    c_int,
    *const u8,
    c_int,
    *mut *mut c_void,
    *mut *mut c_char,
) -> c_int;
type ImageFreeFn = unsafe extern "C" fn(*mut c_void);
type ErrorFreeFn = unsafe extern "C" fn(*mut c_char);

/// Указатели на функции библиотеки.
struct Api {
    create_image: ImageCreateFn,
    image_free: ImageFreeFn,
    detect_video: DetectVideoFn,
    close_result: CloseResultFn,
    close: CloseFn,
    error_free: ErrorFreeFn,
}

/// Детектор кистей MediaPipe.
pub struct MediaPipeHandDetector {
    // Библиотека держится ради указателей на функции: выгружать её нельзя.
    _library: Library,
    landmarker: *mut c_void,
    api: Api,
    started: Instant,
    last_timestamp: i64,
}

impl std::fmt::Debug for MediaPipeHandDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaPipeHandDetector")
            .field("landmarker", &self.landmarker)
            .finish_non_exhaustive()
    }
}

impl MediaPipeHandDetector {
    /// Загружает библиотеку и создаёт детектор по модели.
    pub fn load(config: &MediaPipeConfig) -> Result<Self, VisionError> {
        let library = unsafe { Library::new(&config.library) }
            .map_err(|e| model_error(format!("{}: {e}", config.library.display())))?;
        let api = unsafe { Api::load(&library) }?;

        let model = CString::new(config.model.to_string_lossy().as_bytes())
            .map_err(|e| model_error(format!("путь к модели: {e}")))?;
        let mut options = HandLandmarkerOptions {
            base_options: BaseOptions {
                model_asset_buffer: std::ptr::null(),
                model_asset_buffer_count: 0,
                model_asset_path: model.as_ptr(),
                delegate: 0,
                host_environment: 0,
                host_system: 0,
                host_version: std::ptr::null(),
                ca_bundle_path: std::ptr::null(),
            },
            running_mode: RUNNING_MODE_VIDEO,
            num_hands: config.max_hands as c_int,
            min_hand_detection_confidence: config.min_detection,
            min_hand_presence_confidence: config.min_presence,
            min_tracking_confidence: 0.5,
            result_callback: None,
        };

        let create: CreateFn = unsafe { load_symbol(&library, b"MpHandLandmarkerCreate\0")? };
        let mut landmarker: *mut c_void = std::ptr::null_mut();
        let mut error: *mut c_char = std::ptr::null_mut();
        let status = unsafe { create(&mut options, &mut landmarker, &mut error) };
        check(status, error, &api, "создание HandLandmarker")?;
        if landmarker.is_null() {
            return Err(model_error("HandLandmarker не создан".into()));
        }

        Ok(MediaPipeHandDetector {
            _library: library,
            landmarker,
            api,
            started: Instant::now(),
            last_timestamp: -1,
        })
    }

    /// Находит кисти в кадре.
    pub fn detect(&mut self, frame: &RgbFrame) -> Result<Vec<HandLandmarks>, VisionError> {
        if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
            return Err(VisionError::EmptyFrame {
                width: frame.width,
                height: frame.height,
            });
        }

        let mut image: *mut c_void = std::ptr::null_mut();
        let mut error: *mut c_char = std::ptr::null_mut();
        let size = frame.pixels.len() as c_int;
        let status = unsafe {
            (self.api.create_image)(
                IMAGE_FORMAT_SRGB,
                frame.width as c_int,
                frame.height as c_int,
                frame.pixels.as_ptr(),
                size,
                &mut image,
                &mut error,
            )
        };
        check(status, error, &self.api, "создание изображения")?;

        // Временная метка должна строго возрастать: MediaPipe ведёт по ней
        // отслеживание между кадрами.
        let timestamp = self.started.elapsed().as_millis() as i64;
        let timestamp = timestamp.max(self.last_timestamp + 1);
        self.last_timestamp = timestamp;

        let mut result: HandLandmarkerResult = unsafe { std::mem::zeroed() };
        let status = unsafe {
            (self.api.detect_video)(
                self.landmarker,
                image,
                std::ptr::null(),
                timestamp,
                &mut result,
                &mut error,
            )
        };
        if let Err(failure) = check(status, error, &self.api, "распознавание кисти")
        {
            unsafe { (self.api.image_free)(image) };
            return Err(failure);
        }

        let hands = self.collect(&result, frame);
        unsafe {
            (self.api.close_result)(&mut result);
            (self.api.image_free)(image);
        }
        Ok(hands)
    }

    /// Переводит результат MediaPipe в точки кистей.
    fn collect(&self, result: &HandLandmarkerResult, frame: &RgbFrame) -> Vec<HandLandmarks> {
        let width = frame.width as f32;
        let height = frame.height as f32;
        let mut hands = Vec::new();
        for index in 0..result.hand_landmarks_count as usize {
            let group = unsafe { &*result.hand_landmarks.add(index) };
            if group.landmarks_count as usize != joint::COUNT {
                continue;
            }
            let mut points = Vec::with_capacity(joint::COUNT);
            for order in 0..joint::COUNT {
                let landmark = unsafe { &*group.landmarks.add(order) };
                points.push(Point::new(landmark.x * width, landmark.y * height));
            }
            let score = hand_score(result, index);
            hands.push(HandLandmarks::new(points, score));
        }
        hands
    }
}

impl Drop for MediaPipeHandDetector {
    fn drop(&mut self) {
        if !self.landmarker.is_null() {
            let mut error: *mut c_char = std::ptr::null_mut();
            unsafe { (self.api.close)(self.landmarker, &mut error) };
            free_error(&self.api, error);
        }
    }
}

impl Api {
    /// Загружает указатели на функции из библиотеки.
    unsafe fn load(library: &Library) -> Result<Api, VisionError> {
        Ok(Api {
            create_image: load_symbol(library, b"MpImageCreateFromUint8Data\0")?,
            image_free: load_symbol(library, b"MpImageFree\0")?,
            detect_video: load_symbol(library, b"MpHandLandmarkerDetectForVideo\0")?,
            close_result: load_symbol(library, b"MpHandLandmarkerCloseResult\0")?,
            close: load_symbol(library, b"MpHandLandmarkerClose\0")?,
            error_free: load_symbol(library, b"MpErrorFree\0")?,
        })
    }
}

/// Оценка кисти — уверенность классификации левой/правой руки.
fn hand_score(result: &HandLandmarkerResult, index: usize) -> f32 {
    if index >= result.handedness_count as usize {
        return 1.0;
    }
    let group = unsafe { &*result.handedness.add(index) };
    let mut best = 0.0f32;
    for order in 0..group.categories_count as usize {
        let category = unsafe { &*group.categories.add(order) };
        best = best.max(category.score);
    }
    best
}

/// Загружает один символ библиотеки, копируя указатель на функцию.
unsafe fn load_symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, VisionError> {
    let symbol: libloading::Symbol<'_, T> = library.get(name).map_err(|e| {
        model_error(format!(
            "нет символа {}: {e}",
            String::from_utf8_lossy(name)
        ))
    })?;
    Ok(*symbol)
}

/// Проверяет статус вызова и освобождает сообщение об ошибке.
fn check(status: c_int, error: *mut c_char, api: &Api, what: &str) -> Result<(), VisionError> {
    if status == MP_OK {
        return Ok(());
    }
    let message = if error.is_null() {
        format!("код {status}")
    } else {
        let text = unsafe { CStr::from_ptr(error).to_string_lossy().into_owned() };
        let result = format!("{text} (код {status})");
        free_error(api, error);
        result
    };
    Err(model_error(format!("{what}: {message}")))
}

/// Освобождает строку ошибки, выделенную библиотекой.
fn free_error(api: &Api, error: *mut c_char) {
    if !error.is_null() {
        unsafe { (api.error_free)(error) };
    }
}

/// Ошибка детектора.
fn model_error(message: String) -> VisionError {
    VisionError::Model(message)
}
