import { useMemo } from 'react'

import type { AppState } from '@/shared/api/types'
import { BONES, isVisible, TIPS } from '@/shared/lib/skeleton'

interface HandOverlayProps {
  state: AppState
  width: number
  height: number
}

/**
 * Скелет кистей поверх кадра.
 *
 * Координаты — пиксели кадра, поэтому `viewBox` совпадает с размером
 * превью, а `preserveAspectRatio="none"` тянет SVG вместе с картинкой.
 */
export function HandOverlay({ state, width, height }: HandOverlayProps) {
  const paths = useMemo(
    () =>
      state.engine.hands.map((hand, index) => ({
        index,
        bones: BONES.flatMap(([from, to], order) => {
          const a = hand[from]
          const b = hand[to]
          if (!isVisible(a) || !isVisible(b)) return []
          return [{ key: `${from}-${to}-${order}`, from: a, to: b }]
        }),
        tips: TIPS.flatMap((tip) => {
          const point = hand[tip]
          return isVisible(point) ? [{ key: tip, point }] : []
        }),
        points: hand.flatMap((point, order) =>
          isVisible(point) ? [{ key: order, point }] : [],
        ),
      })),
    [state.engine.hands],
  )

  return (
    <svg
      className="preview__skeleton"
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      role="img"
      aria-label="Скелет кисти поверх кадра"
    >
      {paths.map(({ index, bones, tips, points }) => (
        <g key={index} className="skeleton">
          {bones.map(({ key, from, to }) => (
            <line key={key} x1={from.x} y1={from.y} x2={to.x} y2={to.y} />
          ))}
          {points.map(({ key, point }) => (
            <circle key={key} cx={point.x} cy={point.y} r="4" />
          ))}
          {tips.map(({ key, point }) => (
            <circle key={`tip-${key}`} cx={point.x} cy={point.y} r="6" />
          ))}
        </g>
      ))}
    </svg>
  )
}
