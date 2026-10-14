# The page's input

Part of the [particle notes](../../PARTICLES_REVERSE_ENGINEER.md): what stands in for the controller
in `ps3xmbwave/` - the arrow keys and the mouse, in `xmb-input.js` - and what the particles do with
it. How the console's own input reaches the scene, and what it does there, is in [The
controller](controller.md#the-controller); the response is ported from there, and what stands in
for the controller is modelled. The rest of the implementation is in
[`implementation.md`](implementation.md).

## The arrow keys

- Steps are the arrow keys, one a frame, the way the XMB sends them. A key the browser repeats is
  held - the newest of those down, since the browser repeats only the last one pressed - and the
  system repeats it every 8 frames of its own, the rate the captures' turn implies. Held for ten
  seconds, the turn settles between 0.1745 and 0.2010 times `dpad rot max`, where the four
  captures read 0.176 to 0.191.
- Every step also moves the modelled icons, whose wind goes through the flow grid - see [Modelled
  choices](implementation.md#modelled-choices): sideways steps raise it along the row - in y, the
  icons growing and shrinking, `icon wind scl x` being 0 - as well as turning the field, as on the
  console.
- Up and down are the page's choice, `verticalSteps`. Turn, its default, has them turn the field
  about the camera's right axis with `dpad scale x`'s weight and kick the noise, as sideways steps
  do, scrolling no items. Scroll is the console's way: with `dpad scale y` at 0 they only scroll
  the items, whose wind takes each cell the way its icon moves, and down scrolls them up (inferred
  from how the XMB scrolls). Either way, down sweeps the particles up.
- Under Turn, a horizontal and a vertical arrow held together turn the field about the diagonal
  between them, the page's own: the XMB sends one direction a step, and here each step's axis
  becomes the diagonal, as long as one direction's, so the field turns no faster than for one.
  Each step still kicks the noise as its own direction does.

**Which way the field turns is measured.** Four captures, two taken holding right and two holding
left, carry the rotation at +2.09e-5 and +2.16e-5 against -2.05e-5 and -2.23e-5: right is
positive. With the particles six and a half units beyond the centre of the turn, that sweeps them
left - the way the icons go. It also dates the savestate taken while navigating: its +1.84e-5 was
a step to the right.

**Headless**, holding a direction for two seconds - a step, then after 30 frames one every 8:

- the field turns only from the fourth step on, and until 33 frames after the key is let go. A
  second after that, the drawn particles' mean has moved by -0.34 in x for right and +0.27 for
  left. A single tap does not turn the field at all. Under Scroll, down and up move the mean by
  +0.06 and -0.04 in y: the wind acts only where the icons move;
- as the key is let go, the turn has added 0.70 to the live particles' mean velocity, the way it
  sweeps them, for right and for up alike, and right and up held together add 0.71 at 45°, the
  turn a frame peaking at 2.02e-5 against 2.04e-5. Their mean place is a poor gauge sideways: the
  band spans the kill box's width, so the particles swept past its wall die and the emitter fills
  the band again;
- the noise stands at 3.16 times its rest as the key is let go, and 2.74 times 0.4 s later; the
  savestate taken while navigating read 3.19. A tap raises it to 1.25 times.

## The mouse

- Dragging moves a virtual Sixaxis. The drag's acceleration reads on the accelerometer's x and y,
  through `mouseAccelToG` and 1 g = 112 of the PPU's units, and its sideways speed reads on the
  gyro, through `mouseYawGain`. z stays at zero. Headless, a synthetic shake of the
  accelerometer's x and the gyro, ±0.4 of the PPU's units at 3 Hz, holds the noise at 6.3 times its
  rest, against the shaking savestate's 6.63, and turns the field for 20 frames as it starts,
  against the gyro's reading at that moment. At ±0.05 nothing is detected, and the motion alone
  raises the noise by a third.
- The pointer is the page's own: the console has none. Its place goes through the icons' wind
  writer under an id of its own, in both directions - its x not weighed by `icon wind scl x` - and
  its bytes times `pointerWind`, 0.5 by default; past a quarter of the screen's height a second
  they saturate, as a stepping icon's do. Off the scene or dragging it writes nothing, nor on its
  return until it moves. It sends no steps: the arrow keys turn the field, the pointer only
  pushes the particles it passes over.
