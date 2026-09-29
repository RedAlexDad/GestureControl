/** Синтетические кадры для проверки ядра без камеры. */

import { JOINTS, type FrameInput, type PointSer } from './types'

const WIDTH = 640
const HEIGHT = 480

/** Точка скелета кисти, смещённая относительно запястья. */
const SKELETON: readonly (readonly [number, number])[] = [
  [0, 0.12], // запястье
  [-0.16, 0.02],
  [-0.3, -0.02],
  [-0.42, -0.04],
  [-0.52, -0.06], // большой палец
  [-0.12, -0.14],
  [-0.14, -0.34],
  [-0.15, -0.5],
  [-0.16, -0.62], // указательный
  [-0.02, -0.16],
  [-0.02, -0.38],
  [-0.02, -0.56],
  [-0.02, -0.7], // средний
  [0.08, -0.14],
  [0.1, -0.34],
  [0.11, -0.5],
  [0.12, -0.62], // безымянный
  [0.17, -0.1],
  [0.21, -0.26],
  [0.24, -0.4], // мизинец
  [0, 0.1], // основание ладони
]

/** Скелет кисти, повёрнутый на угол и сдвинутый в точку кадра. */
export function hand(cx: number, cy: number, angle: number, scale = 1): PointSer[] {
  const sin = Math.sin(angle)
  const cos = Math.cos(angle)
  return SKELETON.map(([dx, dy]) => ({
    x: cx + (dx * cos - dy * sin) * scale,
    y: cy + (dx * sin + dy * cos) * scale,
  }))
}

/** Раскрытая ладонь: пять пальцев, все точки на месте. */
export function openPalm(cx: number, cy: number, angle: number, scale = 1): PointSer[] {
  return hand(cx, cy, angle, scale)
}

/** Кулак: кончики пальцев собраны к ладони, палец не вытянут. */
export function fist(cx: number, cy: number, angle: number, scale = 1): PointSer[] {
  const curled = hand(cx, cy, angle, scale)
  return curled.map((point, index) => {
    if (index === 0) return point
    const fingers = [1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 13, 14, 15, 17, 18, 19]
    if (!fingers.includes(index)) return point
    const wrist = curled[0]
    if (!wrist) return point
    return {
      x: wrist.x + (point.x - wrist.x) * 0.45,
      y: wrist.y + (point.y - wrist.y) * 0.45,
    }
  })
}

/** Кисть, у которой часть точек не видна: отрицательный `x` — пропуск. */
export function partialHand(cx: number, cy: number, angle: number, scale = 1): PointSer[] {
  const full = fist(cx, cy, angle, scale)
  return full.map((point, index) => (index >= 8 && index <= 12 ? { x: -1, y: -1 } : point))
}

/** Кисть с неверным числом точек: ядро должно её отбросить. */
export function brokenHand(): PointSer[] {
  return hand(WIDTH / 2, HEIGHT / 2, 0).slice(0, 5)
}

/** Кадр из переданных кистей. */
export function frame(time: number, hands: PointSer[][]): FrameInput {
  return { time, hands }
}

/** Рамка кадра: нужна компоновке скелета. */
export const FRAME = { width: WIDTH, height: HEIGHT, joints: JOINTS }
