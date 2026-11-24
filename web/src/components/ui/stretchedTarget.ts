// One real link or button per row or card, stretched over it by its ::after,
// rather than a container with role="link" and a click handler: a real link
// keeps middle-click, Cmd-click, the status-bar URL and one tab stop per row,
// and screen readers announce it by its name. Controls that come after it in
// the DOM and are `relative` paint above the ::after, so they stay clickable.
export const stretchedTarget = [
  'font-medium text-primary after:absolute after:inset-0',
  'focus-visible:outline-none focus-visible:after:outline-2 focus-visible:after:-outline-offset-2 focus-visible:after:outline-primary',
]
