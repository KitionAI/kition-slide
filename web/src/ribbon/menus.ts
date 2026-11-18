/** Native ribbon menu paths, plus the native flyout/pane paths belonging to each tab. */
export const RIBBON_MENUS: Record<string, string[]> = {
  home: ['Home', 'Edit', 'Format'],
  insert: ['Insert'],
  draw: [],
  design: ['Design', 'Format Background'],
  transitions: ['Transitions'],
  animations: ['Animations', 'Animation Pane'],
  slideShow: ['Slide Show'],
  record: [],
  review: ['Review', 'Comments'],
  view: ['View'],
  shapeFormat: ['Shape Format', 'Arrange', 'Format Shape', 'Selection Pane', 'Format'],
  pictureFormat: ['Picture Format', 'Arrange', 'Shape Format'],
  tableDesign: ['Table Design'],
  tableLayout: ['Table Layout', 'Format'],
  chartDesign: ['Chart Design'],
  playback: ['Playback', 'Video Format'],
  slideMaster: ['Slide Master', 'Design', 'Format Background'],
}

/** The engine commands each contextual tab shows in Single line's row (Office's Simplified Ribbon). */
export const CONTEXT_PRIMARY: Record<string, readonly string[]> = {
  shapeFormat: ['shape.fill', 'shape.line', 'shape.effects', 'shape.change', 'arrange.bringForward', 'arrange.sendBackward', 'arrange.group', 'arrange.align', 'shape.resize'],
  pictureFormat: ['picture.crop', 'picture.adjust', 'picture.reset', 'picture.change', 'arrange.bringForward', 'arrange.sendBackward', 'shape.altText'],
  playback: ['media.play', 'media.pause', 'media.stop', 'media.options'],
  slideMaster: ['master.insertLayout', 'master.renameLayout', 'master.deleteLayout', 'master.insertPlaceholder', 'view.closeMaster'],
}
