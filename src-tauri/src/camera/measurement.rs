//! Счётчик фактической частоты кадров.
//!
//! Запрошенная у ffmpeg частота и фактическая — разные числа: фильтр fps
//! умеет только дублировать и выбрасывать кадры, но не ускоряет камеру.
//! Поэтому единственный честный ответ на «сколько кадров в секунду» даёт
//! счётчик, а не конфигурация.

use std::time::{Duration, Instant};

/// Считает реальную частоту кадров по стенным часам.
pub struct Measurement {
    /// Начало текущего окна измерения.
    since: Instant,
    /// Сколько кадров было на прошлом пересчёте.
    last_total: u64,
    /// Последняя измеренная частота.
    fps: f64,
}

impl Measurement {
    /// Начинает отсчёт.
    pub fn start() -> Self {
        Measurement {
            since: Instant::now(),
            last_total: 0,
            fps: 0.0,
        }
    }

    /// Отмечает кадр; раз в секунду пересчитывает частоту.
    ///
    /// Отсчёт начинается заново на каждом пересчёте: иначе `elapsed` копится
    /// и частота делится на всё время с начала захвата, из-за чего значение
    /// неуклонно падает вместо ровного.
    pub fn tick(&mut self, total: u64) -> Option<Duration> {
        let elapsed = self.since.elapsed();
        if elapsed < Duration::from_secs(1) {
            return None;
        }
        let counted = total.saturating_sub(self.last_total) as f64;
        self.last_total = total;
        self.since = Instant::now();
        self.fps = counted / elapsed.as_secs_f64();
        Some(elapsed)
    }

    /// Последняя измеренная частота.
    pub fn fps(&self) -> f64 {
        self.fps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_quiet_before_a_second() {
        let mut measured = Measurement::start();
        // Помечаем кадры быстрее, чем идёт секунда: пересчёта быть не должно,
        // иначе частота считалась бы по времени от запуска, а не между кадрами.
        for total in 1..=5u64 {
            assert!(measured.tick(total).is_none());
        }
        assert_eq!(measured.fps(), 0.0);
    }

    #[test]
    fn reports_frames_per_second() {
        let mut measured = Measurement::start();
        // Ждём заведомо больше секунды, чтобы результат не зависел от того,
        // сколько кадров успела прочитать машина.
        std::thread::sleep(Duration::from_millis(1100));
        let elapsed = measured.tick(30).expect("секунда прошла");
        assert!(elapsed >= Duration::from_secs(1));
        // 30 кадров за ~1.1 с — это примерно 27 fps. Диапазон широкий
        // намеренно: точное число зависит от планировщика, а вот порядок
        // величины и явный ноль отличают работающий счётчик от заглушки.
        let fps = measured.fps();
        assert!(fps > 15.0, "частота неправдоподобно низкая: {fps}");
        assert!(fps < 45.0, "частота неправдоподобно высокая: {fps}");
    }

    #[test]
    fn stays_flat_across_seconds() {
        let mut measured = Measurement::start();
        // Две полные секунды подряд: если отсчёт не начинается заново,
        // вторая частота получится в разы ниже первой, потому что
        // знаменатель растёт вместе с временем захвата.
        std::thread::sleep(Duration::from_millis(1050));
        measured.tick(30).expect("первая секунда прошла");
        let first = measured.fps();

        std::thread::sleep(Duration::from_millis(1050));
        measured.tick(60).expect("вторая секунда прошла");
        let second = measured.fps();

        // Окно широкое намеренно: счёт зависит от планировщика, а важен
        // сам факт, что вторая секунда не провалилась в ноль.
        assert!(first > 15.0, "первая частота слишком низкая: {first}");
        assert!(
            second > 15.0,
            "частота затухает со временем: было {first}, стало {second}"
        );
    }
}
