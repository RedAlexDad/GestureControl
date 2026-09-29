//! Печатает 21 точку каждой кисти для сверки с эталоном.
use gesture_vision::detector::{DetectorConfig, OnnxHandDetector};
use gesture_vision::RgbFrame;
use std::env;
fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).expect("путь к кадру");
    let width: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(640);
    let height: u32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(480);
    let frame = RgbFrame::new(width, height, std::fs::read(path).expect("кадр")).expect("размер");
    let mut detector = OnnxHandDetector::load(DetectorConfig::default()).expect("модели");
    for hand in detector.detect(&frame).expect("детекция") {
        for point in &hand.points {
            print!("{:.1},{:.1} ", point.x, point.y);
        }
        println!("score={:.3}", hand.score);
    }
}
