/**
 * Tessera icons for the Slide engine's commands that kit-editor's rules cannot read (they draw
 * the neutral command mark otherwise): its own vocabulary, by command id. Registered once, when the
 * editor loads (`main.tsx`); ribbons and menus built from engine commands draw them.
 */
import { registerCommandIcons } from '@kition/sdk/editor'
import { ArrowUpDownIcon, CaseUpperIcon, EraserIcon, ListIcon, PaintBucketIcon, ScalingIcon, TextAlignJustifyIcon, TimerIcon, TransitionFadeIcon, TypeIcon } from '@kition/tessera/icons/react'

registerCommandIcons({
  "animation.timing": TimerIcon,
  "arrange.reorder": ArrowUpDownIcon,
  "slide.background": PaintBucketIcon,
  "theme.fonts": TypeIcon,
  "ink.eraser": EraserIcon,
  "format.autofit": ScalingIcon,
  "format.bullets": ListIcon,
  "format.caps": CaseUpperIcon,
  "format.justify": TextAlignJustifyIcon,
  "transition.set": TransitionFadeIcon,
  "transition.timing": TimerIcon,
})
