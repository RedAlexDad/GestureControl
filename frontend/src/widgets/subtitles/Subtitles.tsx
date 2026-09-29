import { signLabel } from '@/entities/session'
import type { AppMode, Sign } from '@/shared/api/types'

interface SubtitlesProps {
  mode: AppMode
  sign: Sign
  phrase: string
  word: string | null
  recognitionEnabled: boolean
}

/**
 * Крупная строка субтитров: накопленная фраза или текущий жест.
 *
 * В режиме перевода слова приходят из словаря пользователя. Пока жестов
 * не записано, крупным текстом идёт текущий распознанный жест, чтобы было
 * видно: распознавание работает и текст появляется.
 */
export function Subtitles({ mode, sign, phrase, word, recognitionEnabled }: SubtitlesProps) {
  const text = phrase || word || signLabel(sign)
  const empty = !phrase && !word

  const hint = !recognitionEnabled
    ? 'распознавание выключено'
    : mode === 'Translate'
      ? 'показывайте записанные жесты — слова встанут в строку'
      : 'жесты управления: ладонь, кулак, лайк, свайпы'

  return (
    <section className="subtitles" aria-live="polite">
      <span className="subtitles__label">Субтитры</span>
      <p className={empty ? 'subtitles__text subtitles__text--idle' : 'subtitles__text'}>{text}</p>
      <p className="subtitles__hint">{hint}</p>
    </section>
  )
}
