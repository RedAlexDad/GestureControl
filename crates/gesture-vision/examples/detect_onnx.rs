//! Прогон детектора по сырому RGB-кадру.
//!
//! Служит для сверки с эталоном: кадр сохраняется скриптом и подаётся сюда.
//!
//! ```text
//! cargo run -p gesture-vision --example detect_onnx -- frame.rgb 720 382
//! ```

use std::env;

use gesture_vision::detector::{DetectorConfig, OnnxHandDetector};
use gesture_vision::RgbFrame;

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).expect("путь к сырому кадру");
    let width: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(640);
    let height: u32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(480);

    let pixels = std::fs::read(path).expect("кадр читается");
    let frame = RgbFrame::new(width, height, pixels).expect("кадр верного размера");
    let mut detector = OnnxHandDetector::load(DetectorConfig::default()).expect("модели загружены");
    let hands = detector.detect(&frame).expect("детекция без ошибок");

    println!("кистей: {}", hands.len());
    for (index, hand) in hands.iter().enumerate() {
        let xs: Vec<f32> = hand.points.iter().map(|point| point.x).collect();
        let ys: Vec<f32> = hand.points.iter().map(|point| point.y).collect();
        let min_x = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max_x = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let min_y = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let max_y = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        println!(
            "кисть {index}: score={:.3} x[{min_x:.0},{max_x:.0}] y[{min_y:.0},{max_y:.0}] wrist=({:.0},{:.0})",
            hand.score, hand.points[0].x, hand.points[0].y
        );
    }
}
