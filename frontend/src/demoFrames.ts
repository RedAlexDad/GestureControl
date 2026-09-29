/** Синтетические кадры для проверки ядра без камеры. */

/**
 * Позы собраны так, чтобы проходить настоящий классификатор.
 *
 * Координаты ладони взяты из теста `open_palm_pauses_recognition` в
 * `gesture-core`, поэтому ладонь точно считается. Пальцы «сгибаются»
 * переносом точек к запястью: при таком движении кончик заведомо
 * ближе к запястью, чем сустав, а именно это проверяет
 * `HandGeometry::is_finger_extended`.
 *
 * Порядок точек — как `gesture_core::joint`: запястье, затем по четыре
 * точки на палец от основания к кончику.
 */

import type { PointSer } from './types'

const WRIST = 0
const THUMB_TIP = 4
const INDEX_MCP = 5
const MIDDLE_MCP = 9
const RING_MCP = 13
const LITTLE_MCP = 17

/** Пальцы кроме большого: основание, затем три точки к кончику. */
const FINGERS = [
  [INDEX_MCP, 6, 7, 8],
  [MIDDLE_MCP, 10, 11, 12],
  [RING_MCP, 14, 15, 16],
  [LITTLE_MCP, 18, 19, 20],
] as const

/** Точки ладони, по которым ядро считает размер и центр кисти. */
const PALM = [WRIST, INDEX_MCP, MIDDLE_MCP, RING_MCP, LITTLE_MCP]

/** Раскрытая ладонь, wrist в (320, 240). Размер ладони — 55 px. */
const PALM_POINTS: PointSer[] = [
  { x: 320, y: 240 },
  { x: 300, y: 235 },
  { x: 282, y: 230 },
  { x: 268, y: 223 },
  { x: 258, y: 215 },
  { x: 295, y: 195 },
  { x: 295, y: 180 },
  { x: 295, y: 170 },
  { x: 295, y: 160 },
  { x: 320, y: 185 },
  { x: 320, y: 170 },
  { x: 320, y: 160 },
  { x: 320, y: 150 },
  { x: 345, y: 195 },
  { x: 345, y: 180 },
  { x: 345, y: 170 },
  { x: 345, y: 160 },
  { x: 368, y: 210 },
  { x: 368, y: 195 },
  { x: 368, y: 185 },
  { x: 368, y: 175 },
]

/**
 * Доля пути от сустава к запястью для сустава, средней и кончика.
 *
 * Чем больше доля, тем ближе точка к запястью: кончик заведомо оказывается
 * ближе к запястью, чем сустав, а именно это и означает согнутый палец.
 */
const CURL_SHARES: readonly [number, number, number] = [0.3, 0.55, 0.75]

/** Сдвигает три точки пальца к запястью: палец оказывается согнут. */
function curl(points: PointSer[], finger: readonly [number, number, number, number]): void {
  const base = points[finger[0]]
  const wrist = points[WRIST]
  if (!base || !wrist) return
  const joints = [finger[1], finger[2], finger[3]]
  for (const [step, index] of joints.entries()) {
    const point = points[index]
    const share = CURL_SHARES[step]
    if (!point || share === undefined) continue
    point.x = base.x + (wrist.x - base.x) * share
    point.y = base.y + (wrist.y - base.y) * share
  }
}

/** Сдвигает кончик большого пальца в заданную точку. */
function thumbTo(points: PointSer[], x: number, y: number): void {
  const tip = points[THUMB_TIP]
  if (tip) {
    tip.x = x
    tip.y = y
  }
}

function pose(): PointSer[] {
  return PALM_POINTS.map((point) => ({ ...point }))
}

/** Ладонь: все четыре пальца вытянуты. */
export function openPalm(): PointSer[] {
  return pose()
}

/**
 * Кулак: пальцы согнуты, большой прижат к основанию указательного.
 *
 * Кончик большого остаётся ниже верхней грани ладони, иначе получится
 * не кулак, а «лайк».
 */
export function fist(): PointSer[] {
  const points = pose()
  for (const finger of FINGERS) curl(points, finger)
  thumbTo(points, 305, 205)
  return points
}

/** «Лайк»: те же согнутые пальцы, но большой поднят вверх. */
export function thumbsUp(): PointSer[] {
  const points = pose()
  for (const finger of FINGERS) curl(points, finger)
  thumbTo(points, 296, 118)
  return points
}

/** «victory»: вытянуты указательный и средний. */
export function victory(): PointSer[] {
  const points = pose()
  curl(points, FINGERS[2]!)
  curl(points, FINGERS[3]!)
  thumbTo(points, 268, 228)
  return points
}

/** Указательный палец: остальные согнуты. */
export function pointing(): PointSer[] {
  const points = pose()
  curl(points, FINGERS[1]!)
  curl(points, FINGERS[2]!)
  curl(points, FINGERS[3]!)
  thumbTo(points, 290, 215)
  return points
}

/**
 * Кисть с выпавшими точками: кончики указательного и среднего закрыты.
 *
 * Отрицательные координаты — соглашение ядра об отсутствующей точке.
 */
export function partialHand(): PointSer[] {
  const points = openPalm()
  for (const index of [8, 12]) {
    const point = points[index]
    if (point) {
      point.x = -1
      point.y = -1
    }
  }
  return points
}

/** Кисть с неверным числом точек: ядро должно её отбросить. */
export function brokenHand(): PointSer[] {
  return openPalm().slice(0, 7)
}

/** Кисть без ладони: `HandGeometry::new` вернёт `None`. */
export function noPalmHand(): PointSer[] {
  const points = openPalm()
  for (const index of PALM) {
    const point = points[index]
    if (point) {
      point.x = -1
      point.y = -1
    }
  }
  return points
}

/**
 * Сколько кадров нужно отправить, чтобы поза была распознана.
 *
 * Один кадр не даёт ничего: автомат на появлении кисти лишь запоминает
 * положение, а жест появляется, только когда поза держится дольше
 * `STILL_TIME` ядра. Двенадцать кадров по 1/30 с покрывают и это время,
 * и запас на `HOLD_TIME` удержания.
 */
export const POSE_FRAMES = 12
