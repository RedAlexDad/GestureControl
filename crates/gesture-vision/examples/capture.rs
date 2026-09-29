//! Проверка захвата камеры без окна приложения.
//!
//! Пример открывает камеру через ffmpeg, читает несколько кадров и печатает
//! их среднюю яркость. Нужен, чтобы проверить устройство и ffmpeg отдельно от
//! Tauri: если кадры идут, проблема не в захвате.
//!
//! ```sh
//! cargo run -p gesture-vision --example capture -- --frames 30
//! cargo run -p gesture-vision --example capture -- --device /dev/video1 --size 320x240
//! ```

use std::process::ExitCode;

use gesture_vision::{CameraConfig, FfmpegCamera, FrameSource, RgbFrame};

/// Параметры разбора командной строки.
struct Options {
    device: String,
    size: (u32, u32),
    fps: u32,
    frames: usize,
    out: Option<String>,
}

/// Разбирает `--ключ значение` и `--ключ=значение`.
fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        device: "/dev/video0".into(),
        size: (640, 480),
        fps: 15,
        frames: 10,
        out: None,
    };
    let mut index = 0;
    while index < args.len() {
        let (key, inline) = match args[index].split_once('=') {
            Some((key, value)) => (key.to_string(), Some(value.to_string())),
            None => (args[index].clone(), None),
        };
        let mut value = || -> Result<String, String> {
            // У `--ключ=значение` значение уже лежит в этом же аргументе,
            // иначе значение — следующий аргумент.
            if let Some(inline) = inline.clone() {
                return Ok(inline);
            }
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{key} требует значения"))
        };
        match key.as_str() {
            "--device" => options.device = value()?,
            "--frames" => {
                options.frames = value()?
                    .parse()
                    .map_err(|_| "--frames нужно целое число".to_string())?
            }
            "--fps" => {
                options.fps = value()?
                    .parse()
                    .map_err(|_| "--fps нужно целое число".to_string())?
            }
            "--size" => options.size = parse_size(&value()?)?,
            "--out" => options.out = Some(value()?),
            other => return Err(format!("неизвестный аргумент {other}")),
        }
        index += 1;
    }
    Ok(options)
}

/// Разбирает размер вида `640x480`.
fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let (width, height) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("размер {text} должен быть вида 640x480"))?;
    let width = width
        .parse()
        .map_err(|_| format!("ширина {width} не число"))?;
    let height = height
        .parse()
        .map_err(|_| format!("высота {height} не число"))?;
    Ok((width, height))
}

/// Средняя яркость кадра: сразу видно, что пришёл не чёрный прямоугольник.
fn brightness(frame: &RgbFrame) -> f64 {
    let sum: u64 = frame
        .pixels
        .chunks(3)
        .map(|pixel| {
            let (red, green, blue) = (pixel[0] as u64, pixel[1] as u64, pixel[2] as u64);
            (red * 77 + green * 150 + blue * 29) / 256
        })
        .sum();
    sum as f64 / (frame.pixels.len() / 3).max(1) as f64
}

fn main() -> ExitCode {
    let options = match parse_args(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(options) => options,
        Err(reason) => {
            eprintln!("{reason}");
            return ExitCode::FAILURE;
        }
    };
    let config = CameraConfig {
        device: options.device.into(),
        size: options.size,
        fps: options.fps,
    };
    let mut camera = match FfmpegCamera::start(config) {
        Ok(camera) => camera,
        Err(error) => {
            eprintln!("не удалось открыть камеру: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "камера {} открыта, кадры {}x{} @ {}fps",
        camera.device().display(),
        options.size.0,
        options.size.1,
        options.fps
    );
    let started = std::time::Instant::now();
    for index in 1..=options.frames {
        let frame = match camera.next_frame() {
            Ok(frame) => frame,
            Err(error) => {
                eprintln!("кадр {index}: {error}");
                return ExitCode::FAILURE;
            }
        };
        println!(
            "кадр {index}: {}x{}, байт {}, яркость {:.1}",
            frame.width,
            frame.height,
            frame.pixels.len(),
            brightness(&frame)
        );
        if let Some(path) = &options.out {
            if index == options.frames {
                match write_ppm(path, &frame) {
                    Ok(()) => println!("кадр сохранён в {path}"),
                    Err(reason) => eprintln!("не удалось сохранить кадр: {reason}"),
                }
            }
        }
    }
    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "прочитано {} кадров за {:.2}с ({:.1} fps)",
        options.frames,
        elapsed,
        options.frames as f64 / elapsed.max(0.001)
    );
    ExitCode::SUCCESS
}

/// Сохраняет кадр в PPM: формат без сжатия, который понимает любой просмотрщик.
fn write_ppm(path: &str, frame: &RgbFrame) -> Result<(), String> {
    use std::fmt::Write as _;
    let mut text = format!("P6\n{} {}\n255\n", frame.width, frame.height);
    // Заголовок и данные нельзя смешивать, поэтому пишем через два буфера.
    write!(text, "").map_err(|error| error.to_string())?;
    let header = text;
    let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
    use std::io::Write as _;
    file.write_all(header.as_bytes())
        .and_then(|()| file.write_all(&frame.pixels))
        .map_err(|error| error.to_string())
}
