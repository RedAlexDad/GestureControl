//! Прогон официального MediaPipe Hand Landmarker по сырому RGB-кадру.
//!
//! ```text
//! cargo run -p gesture-vision --example detect_mediapipe -- frame.rgb 720 382
//! ```

use std::env;

use gesture_vision::detector::mediapipe::{MediaPipeConfig, MediaPipeHandDetector};
use gesture_vision::RgbFrame;

fn main() {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).expect("путь к сырому кадру");
    let width: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(640);
    let height: u32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(480);

    let frame = RgbFrame::new(width, height, std::fs::read(path).expect("кадр")).expect("размер");
    let mut detector = MediaPipeHandDetector::load(&MediaPipeConfig::default()).expect("детектор");

    // Пять прогонов: проверяем и точность, и устойчивость между кадрами.
    for pass in 0..5 {
        let hands = detector.detect(&frame).expect("детекция");
        let points = hands
            .first()
            .map(|hand| {
                hand.points
                    .iter()
                    .map(|p| format!("{:.0},{:.0} ", p.x, p.y))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let score = hands.first().map(|hand| hand.score).unwrap_or(0.0);
        println!("проход {pass}: кистей {} score {score:.3}", hands.len());
        if pass == 4 {
            println!("{points}");
        }
    }
}
