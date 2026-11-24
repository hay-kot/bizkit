// The signing page's script: report the view, and offer a draw pad that
// records strokes into the form. Without it the typed signature still works.
;(() => {
  const form = document.getElementById('sign')
  const token = form.querySelector('input[name=t]').value
  fetch('viewed', {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({ t: token }),
  }).catch(() => {})

  const pad = document.getElementById('pad')
  const strokesInput = document.getElementById('strokes')
  const typed = document.getElementById('typed')
  const drawn = document.getElementById('drawn')
  document.getElementById('draw-choice').hidden = false
  const ctx = pad.getContext('2d')
  ctx.lineWidth = 4
  ctx.lineCap = 'round'
  ctx.lineJoin = 'round'
  let strokes = []
  let current = null

  const point = (event) => {
    const box = pad.getBoundingClientRect()
    return [
      Math.round(((event.clientX - box.left) / box.width) * 1000),
      Math.round(((event.clientY - box.top) / box.height) * 300),
    ]
  }
  pad.addEventListener('pointerdown', (event) => {
    pad.setPointerCapture(event.pointerId)
    current = [point(event)]
    strokes.push(current)
  })
  pad.addEventListener('pointermove', (event) => {
    if (!current) return
    const [x, y] = point(event)
    const [px, py] = current[current.length - 1]
    if (Math.abs(x - px) + Math.abs(y - py) < 3) return
    current.push([x, y])
    ctx.beginPath()
    ctx.moveTo(px, py)
    ctx.lineTo(x, y)
    ctx.stroke()
  })
  const end = () => {
    current = null
    strokesInput.value = JSON.stringify(strokes)
  }
  pad.addEventListener('pointerup', end)
  pad.addEventListener('pointercancel', end)
  document.getElementById('clear').addEventListener('click', () => {
    strokes = []
    strokesInput.value = ''
    ctx.clearRect(0, 0, pad.width, pad.height)
  })
  form.addEventListener('change', (event) => {
    if (event.target.name !== 'kind') return
    const draw = event.target.value === 'drawn'
    drawn.hidden = !draw
    typed.hidden = draw
  })
})()
