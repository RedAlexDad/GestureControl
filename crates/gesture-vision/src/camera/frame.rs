//! Кадр RGB24: пиксели, превью и base64 для интерфейса.

use gesture_core::PipelineError;

/// Алфавит base64 без переводов строк.
const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Кадр камеры: пиксели RGB24, три байта на точку, в порядке слева направо.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbFrame {
    /// Ширина кадра в пикселях.
    pub width: u32,
    /// Высота кадра в пикселях.
    pub height: u32,
    /// Пиксели RGB24: ровно `width * height * 3` байт.
    pub pixels: Vec<u8>,
}

impl RgbFrame {
    /// Кадр заданного размера без единого заполненного пикселя.
    pub fn empty(width: u32, height: u32) -> Self {
        RgbFrame {
            width,
            height,
            pixels: vec![0; frame_bytes(width, height)],
        }
    }

    /// Кадр из готовых пикселей: длина проверяется, лишнее отбрасывается.
    ///
    /// Нужно тестам и источникам, которые считают кадр целиком, а не по
    /// частям: тихо принять кадр неверной длины означает потом искать
    /// причину сдвига картинки вместо того, чтобы остановиться на ошибке.
    pub fn new(width: u32, height: u32, mut pixels: Vec<u8>) -> Result<Self, PipelineError> {
        let expected = frame_bytes(width, height);
        if expected == 0 {
            return Err(PipelineError::Camera(format!(
                "некорректный размер кадра {width}x{height}"
            )));
        }
        pixels.truncate(expected);
        pixels.resize(expected, 0);
        Ok(RgbFrame {
            width,
            height,
            pixels,
        })
    }

    /// Меньшая копия кадра для превью.
    ///
    /// Ближайший сосед, а не усреднение: превью нужно дёшево, а размытие
    /// на нём только мешает разглядеть контуры кисти.
    pub fn resized(&self, width: u32, height: u32) -> RgbFrame {
        let width = width.clamp(1, self.width);
        let height = height.clamp(1, self.height);
        if width == self.width && height == self.height {
            return self.clone();
        }
        let mut pixels = vec![0; frame_bytes(width, height)];
        for y in 0..height {
            let source_row = (y as u64 * self.height as u64 / height as u64) as usize;
            for x in 0..width {
                let source_column = (x as u64 * self.width as u64 / width as u64) as usize;
                let from = (source_row * self.width as usize + source_column) * 3;
                let to = (y as usize * width as usize + x as usize) * 3;
                pixels[to..to + 3].copy_from_slice(&self.pixels[from..from + 3]);
            }
        }
        RgbFrame {
            width,
            height,
            pixels,
        }
    }

    /// Пиксели в base64: так интерфейс получает кадр без кодека на стороне
    /// окна и рисует его через `ImageData`.
    pub fn to_base64(&self) -> String {
        base64_encode(&self.pixels)
    }
}

/// Сколько байт занимает кадр RGB24 такого размера.
pub(super) fn frame_bytes(width: u32, height: u32) -> usize {
    width as usize * height as usize * 3
}

/// Кодирует байты в base64 со стандартным заполнением.
pub(super) fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64[(triple >> 18) as usize & 0x3f] as char);
        out.push(BASE64[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            BASE64[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}
