use std::path::{Path, PathBuf};

use super::{keys, DetectorSettings, EnvFile, Settings, WindowSettings, ENV_FILE};

/// Собирает настройки из фиксированных пар: без окружения и без файла.
fn settings_from(pairs: &[(&str, &str)]) -> Settings {
    Settings::from_source(|key| {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_string())
    })
}

/// Собирает настройки детектора из фиксированных пар.
fn detector_from(pairs: &[(&str, &str)]) -> DetectorSettings {
    DetectorSettings::from_source(|key| {
        pairs
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_string())
    })
}

#[test]
fn defaults_capture_thirty_frames() {
    let settings = settings_from(&[]);
    assert_eq!(settings.camera.device, "/dev/video0");
    assert_eq!(settings.camera.width, 640);
    assert_eq!(settings.camera.height, 480);
    assert_eq!(settings.camera.fps, 30);
    assert_eq!(settings.preview_size(), (640, 480));
}

#[test]
fn window_defaults_match_previous_config_file() {
    // Числа перенесены из `tauri.conf.json`, поэтому размер окна при
    // первом запуске после переноса не должен измениться.
    let settings = settings_from(&[]);
    assert_eq!(settings.window, WindowSettings::default());
    assert_eq!(settings.window.width, 1180);
    assert_eq!(settings.window.height, 840);
    assert_eq!(settings.window.min_width, 900);
    assert_eq!(settings.window.min_height, 640);
}

#[test]
fn window_size_comes_from_source() {
    // Размер окна перекрывается так же, как параметры камеры.
    let settings = settings_from(&[
        ("GESTURE_WINDOW_WIDTH", "1024"),
        ("GESTURE_WINDOW_HEIGHT", "768"),
        ("GESTURE_WINDOW_MIN_WIDTH", "640"),
        ("GESTURE_WINDOW_MIN_HEIGHT", "480"),
    ]);
    assert_eq!(settings.window.width, 1024);
    assert_eq!(settings.window.height, 768);
    assert_eq!(settings.window.min_width, 640);
    assert_eq!(settings.window.min_height, 480);
}

#[test]
fn window_minimum_larger_than_size_is_fixed() {
    // Минимум больше окна — опечатка: такое окно не открылось бы или сразу
    // схлопнулось. Минимум ужимается до размера окна.
    let settings = settings_from(&[
        ("GESTURE_WINDOW_WIDTH", "800"),
        ("GESTURE_WINDOW_HEIGHT", "600"),
        ("GESTURE_WINDOW_MIN_WIDTH", "1600"),
        ("GESTURE_WINDOW_MIN_HEIGHT", "1200"),
    ]);
    assert_eq!(settings.window.min_width, 800);
    assert_eq!(settings.window.min_height, 600);
}

#[test]
fn broken_window_numbers_fall_back_to_defaults() {
    let settings = settings_from(&[
        ("GESTURE_WINDOW_WIDTH", "много"),
        ("GESTURE_WINDOW_HEIGHT", "0"),
        ("GESTURE_WINDOW_MIN_WIDTH", ""),
    ]);
    assert_eq!(settings.window.width, 1180);
    assert_eq!(settings.window.height, 840);
    assert_eq!(settings.window.min_width, 900);
}

#[test]
fn source_overrides_defaults() {
    let settings = settings_from(&[
        ("GESTURE_CAMERA_DEVICE", "/dev/video2"),
        ("GESTURE_CAMERA_FPS", "15"),
        ("GESTURE_PREVIEW_WIDTH", "160"),
    ]);
    assert_eq!(settings.camera.device, "/dev/video2");
    assert_eq!(settings.camera.fps, 15);
    assert_eq!(settings.preview.width, 160);
    // Незаданная высота осталась от умолчания.
    assert_eq!(settings.preview.height, 480);
}

#[test]
fn broken_numbers_fall_back_to_defaults() {
    // Опечатка в `.env` не должна мешать запуску окна.
    let settings = settings_from(&[
        ("GESTURE_CAMERA_FPS", "тридцать"),
        ("GESTURE_CAMERA_WIDTH", "0"),
        ("GESTURE_PREVIEW_HEIGHT", ""),
    ]);
    assert_eq!(settings.camera.fps, 30);
    assert_eq!(settings.camera.width, 640);
    assert_eq!(settings.preview.height, 480);
}

#[test]
fn camera_config_carries_settings() {
    let config = settings_from(&[
        ("GESTURE_CAMERA_DEVICE", " /dev/video4 "),
        ("GESTURE_CAMERA_WIDTH", "1280"),
        ("GESTURE_CAMERA_HEIGHT", "720"),
        ("GESTURE_CAMERA_FPS", "25"),
    ])
    .camera_config();
    assert_eq!(config.device, PathBuf::from("/dev/video4"));
    assert_eq!(config.size, (1280, 720));
    assert_eq!(config.fps, 25);
}

#[test]
fn env_file_ignores_noise() {
    let file = EnvFile::parse(
        "\n\
             # комментарий\n\
             GESTURE_CAMERA_FPS=30\n\
             \n\
             export GESTURE_CAMERA_DEVICE=/dev/video1\n\
               GESTURE_PREVIEW_WIDTH = 240  \n\
             без_знака_равенства\n\
             =пустой_ключ\n\
             GESTURE_CAMERA_HEIGHT=\"480\"\n\
             GESTURE_PREVIEW_HEIGHT='180'\n",
    );
    assert_eq!(file.get("GESTURE_CAMERA_FPS"), Some("30"));
    assert_eq!(file.get("GESTURE_CAMERA_DEVICE"), Some("/dev/video1"));
    // Пробелы вокруг ключа и значения не попадают в значение.
    assert_eq!(file.get("GESTURE_PREVIEW_WIDTH"), Some("240"));
    // Кавычки снимаются с обоих видов.
    assert_eq!(file.get("GESTURE_CAMERA_HEIGHT"), Some("480"));
    assert_eq!(file.get("GESTURE_PREVIEW_HEIGHT"), Some("180"));
    assert_eq!(file.get("без_знака_равенства"), None);
    assert_eq!(file.get(""), None);
}

#[test]
fn env_file_keeps_hash_inside_value() {
    // Обрезка по `#` сломала бы путь, где такой символ допустим.
    let file = EnvFile::parse("GESTURE_CAMERA_DEVICE=/dev/video#0\n");
    assert_eq!(file.get("GESTURE_CAMERA_DEVICE"), Some("/dev/video#0"));
}

#[test]
fn environment_wins_over_file() {
    // Тот же порядок слоёв, что и в `load`, но на парах: переменная
    // окружения проверяется раньше файла.
    let file = EnvFile::parse("GESTURE_CAMERA_FPS=15\nGESTURE_CAMERA_DEVICE=/dev/video1\n");
    let environment = [("GESTURE_CAMERA_FPS", "30")];
    let settings = Settings::from_source(|key| {
        environment
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| (*value).to_string())
            .or_else(|| file.get(key).map(str::to_string))
    });
    assert_eq!(settings.camera.fps, 30);
    // Переменной нет — значение остаётся из файла.
    assert_eq!(settings.camera.device, "/dev/video1");
}

#[test]
fn describe_mentions_device_and_fps() {
    let text = settings_from(&[("GESTURE_CAMERA_DEVICE", "/dev/video3")]).describe();
    assert!(text.contains("/dev/video3"), "нет устройства: {text}");
    assert!(text.contains("30 fps"), "нет частоты: {text}");
    // Размер окна тоже должен быть в журнале: иначе не видно, что окно
    // взяло не те числа, что просил `.env`.
    assert!(text.contains("окно 1180x840"), "нет размера окна: {text}");
}

#[test]
fn repository_env_file_is_valid() {
    // Файл из репозитория должен разбираться и задавать то, что в нём
    // написано. Путь ищется от каталога теста вверх до корня.
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("у src-tauri есть родитель")
        .join(ENV_FILE);
    if !path.is_file() {
        return;
    }
    let file = EnvFile::load(&path).expect(".env должен читаться");
    assert_eq!(file.get(keys::CAMERA_FPS), Some("30"));
    assert_eq!(file.get(keys::CAMERA_DEVICE), Some("/dev/video0"));
}

#[test]
fn detector_is_enabled_by_default() {
    let settings = detector_from(&[]);
    assert!(settings.enabled);
    assert_eq!(settings.score_threshold, 0.5);
    assert_eq!(settings.max_hands, 2);
    // Модели ищутся в каталоге `models`; имя файла задано умолчанием.
    assert!(settings.palm_model.ends_with("palm_detection.onnx"));
}

#[test]
fn detector_reads_flags_numbers_and_paths() {
    let settings = detector_from(&[
        (keys::DETECTOR_ENABLED, "нет"),
        (keys::DETECTOR_SCORE, "0.7"),
        (keys::DETECTOR_MAX_HANDS, "1"),
        (keys::PALM_MODEL, "/tmp/palm.onnx"),
    ]);
    assert!(!settings.enabled, "«нет» выключает детектор");
    assert_eq!(settings.score_threshold, 0.7);
    assert_eq!(settings.max_hands, 1);
    assert_eq!(settings.palm_model, PathBuf::from("/tmp/palm.onnx"));
}

#[test]
fn detector_falls_back_on_bad_numbers() {
    let settings = detector_from(&[
        (keys::DETECTOR_SCORE, "abc"),
        (keys::DETECTOR_MAX_HANDS, "0"),
    ]);
    assert_eq!(settings.score_threshold, 0.5);
    assert_eq!(settings.max_hands, 2);
}
