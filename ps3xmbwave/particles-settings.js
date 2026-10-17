'use strict';
// Particle parameters: firmware defaults from PARTICLES.mnu and PARTICLES_UI.mnu, the iridescent texture's palette,
// fitted, plus the console's state the particles are drawn under and the knobs of the modelled PPU side. Consumed by
// `particles.js`, `particles-reverse.js` and `xmb-input.js`; sliders, selects and the palette's editor from
// `settings-panels.js`, in the tabs and sections `PARTICLE_SETTINGS_LAYOUT` lays out, each meta's `help` as its
// label's tooltip and its `hidden`, the reason why, keeping it off the panel.
// Reads `PARTICLE_THEME_OPTIONS` (`scene-themes.js`) at load time for the theme dropdown. The theme, the sequence,
// the music and Theme Settings are the whole scene's state: `scene-themes.js` writes the wave's sets by them too.

window.PARTICLE_SETTINGS = {
  // The scene's parameter set, from `scene-themes.js`; 'base' is the firmware's own PARTICLES.mnu below, and
  // LINE1.mnu.
  theme: 'auto',
  // A boot sequence played on top of the theme (`scene-themes.js`): the XMB's start, or a game's or other
  // content's launch, after which the XMB starts again. It returns to 'none' when it is over.
  sequence: 'none',

  // --- PARTICLES.mnu (firmware 4.93 defaults) -------------------------------------------------
  emitVelMin: 0.15064,
  emitVelMul: 0.19,
  emitVelVar: 0.282567,
  emitConeAngle: 51.8695,
  emitNegProb: 0.173899,
  emitVelZscale: 0,
  emitPerFrame: 16.6539,
  emitProb: 0.479115,
  agingSpeed: 0.00285223,
  agingVariance: 0.493003,
  friction: 0.030551,
  spinTimeScale: 2.74,
  deltaTime: 0.0088883,
  gravity: -6.8e-05,
  windDirX: 0.340188,
  windDirY: 0,
  windDirZ: 0.35,
  windScale: 0,
  windScale10: 0,
  brownianScale: 0.225311,
  spotPosX: 4.16,
  spotPosY: 2.63,
  spotPosZ: -7.6,
  spotAttnX: 1,
  spotAttnY: 0,
  spotAttnZ: 0,
  specularPower: 35.2904,
  specularCoeff: 74.74,
  lambertCoeff: 8,
  exposure: 0.0390458,
  fresnel: 1.33319,
  colorControl: 1,
  iridescentExp: 1,
  globalAlpha: 1,
  sizeMiddle: 0.0536233,
  sizeNear: 0.0772033,
  sizeFar: 0.874062,
  nearFocus: 6.12435,
  nearFocusDist: 1.3193,
  nearFocusPow: 2.47206,
  nearDarkness: 6.24028,
  nearFuzziness: 3.79,
  farFocus: 12.8327,
  farFocusDist: 3.54129,
  farFocusPow: 1.83324,
  farDarkness: 13.9,
  nearAlign: 0.722,
  sizeAlign: 13.19,
  glare: 0.159705,
  glareScale: 5.44386,
  glareP1: 0.99,
  glareP2: 4.44,

  // --- PARTICLES_UI.mnu (firmware 4.93 defaults) ----------------------------------------------
  uiBrownian: 0.6,
  rshakeBrw: 7.4992,
  dpadRotMax: 0.000116654,
  dpadScaleX: 1,
  dpadScaleY: 0,
  dshakeRotMax: 0.000124431,
  dshakeRotImp: 0.513834,
  dshakeBrwImp: 0.061702,
  dshakeThresh: 0.722145,
  dshakeXCoeff: 0.402735,
  dshakeGCoeff: 0.444397,
  iconWind: 11.3877,
  iconWindSclX: 0,
  iconWindSclY: 1,

  // --- proc_iridescent.tga, fitted (a texture, not a .mnu parameter) -----------------------------
  // The colours the glint runs through, as 0xRRGGBB: a periodic Catmull-Rom loop, which `particles.js` winds into the
  // texture's spiral. These 16 are fitted to the firmware's texture (RMS error 13/255).
  iridescentPalette: [
    0xf7f9bd, 0xf3dc60, 0xdc993c, 0xc06a49, 0xd4488a, 0xce34be, 0x964cde, 0x638af4,
    0x357ce9, 0x496de7, 0x6d84ec, 0xa482f5, 0xda85d8, 0xc58b73, 0xb6a856, 0xb3bba0,
  ],

  // --- Console output, modelled PPU side and mouse adapter (not from the firmware) ------------
  // the console's video output: below 1080p the particles grow and fade (PARTICLES_SPE.mnu), and on SD the whole scene
  // dims (_Gamma)
  videoOutput: '1080',
  // What's New's board (wboard_plugin) opening its list: over 2 s the particles speed up and glint more
  // (PARTICLES_SPE.mnu), and closing it takes them back
  whatsNewBoard: 'closed',
  // Music playing in the XMB: the scene goes to the music set over 5.5 s and holds it whatever the hour; when it
  // stops, to the base set over 5.5 s and then back to the theme (`scene-themes.js`)
  musicPlayback: 'stopped',
  // Theme Settings > Background > Brightness, Normal to -5: the scene drawn at 1 - 0.15 per step, faded to over 1 s
  themeBrightness: '0',
  // Theme Settings > Colour, Original or a month: a month stops the scene's clock at noon on its 1st, for the Auto
  // theme and the backdrop's Auto gradient alike
  themeColor: '0',
  // The XMB's background given away, as a video, the browser or the Store take it: the scene fades to black over
  // backgroundFadeMs, and back to the brightness when it is taken back
  xmbBackground: 'shown',
  backgroundFadeMs: 200, // the firmware's callers use 0 to 1000
  flowStrength: 1, // this one is the firmware's, the constant the PPU puts in the block
  iconEaseSec: 0.065, // how fast the modelled icons settle after a step
  // What up and down steps do: 'turn' the whole field, as left and right do sideways, or 'scroll' the selected
  // category's items, as on the console, whose dpad scale y of 0 leaves them no turn
  verticalSteps: 'turn',
  mouseAccelToG: 0.04, // the dragged controller's acceleration, screen heights per second squared, in g
  mouseYawGain: 0.15, // and its sideways speed, screen heights per second, to the gyro's reading
  pointerWind: 0.5, // how hard the pointer pushes the particles it passes over, 1 being as hard as a moving icon
};

window.PARTICLE_SETTINGS_META = {
  theme: {
    type: 'select', options: window.PARTICLE_THEME_OPTIONS,
    help: "Which parameter set the scene runs on, the particles and the wave alike: one of the firmware's theme " +
      'sets, Base for PARTICLES.mnu and LINE1.mnu themselves, or Auto, which blends dawn, day, dusk and night with ' +
      'the clock.\n' +
      'Firmware override sets, played by scene-themes.js',
  },
  sequence: {
    type: 'select',
    options: [
      { value: 'none', label: 'None' }, { value: 'coldboot', label: 'XMB start' },
      { value: 'gameboot', label: 'Game launch' }, { value: 'otherboot', label: 'Other launch' },
    ],
    help: "Plays one of the XMB's boot sequences over the theme: its start, a game's launch or another content's. It " +
      "goes back to None when it is over. The XMB's start begins the wave's lines afresh, and the page opens with it.\n" +
      "The scene's own .rco animations, played by scene-themes.js",
  },

  emitVelMin: {
    min: 0, max: 2, step: 0.001,
    help: 'The slowest a particle is born: its speed at birth never drops below this, however slowly the wave moves ' +
      'where it is born.\n' +
      'PARTICLES.mnu: emit vel min',
  },
  emitVelMul: {
    min: 0, max: 5, step: 0.01,
    help: "How much of the wave's speed a particle is born with: its speed at birth is √(this × the wave's speed " +
      'there), varied by Emit Vel Var and never below Emit Vel Min.\n' +
      'PARTICLES.mnu: emit vel mul',
  },
  emitVelVar: {
    min: 0, max: 2, step: 0.001,
    help: 'Random spread of the speed at birth: what goes under the square root is scaled by 1 plus up to this much ' +
      'either way.\n' +
      'PARTICLES.mnu: emit vel var',
  },
  emitConeAngle: {
    min: 0, max: 180, step: 0.1,
    help: 'Spread of the direction at birth, in degrees: particles leave within this angle of the way the wave moves ' +
      'where they are born.\n' +
      'PARTICLES.mnu: emit cone angle',
  },
  emitNegProb: {
    min: 0, max: 1, step: 0.001,
    help: "Chance that a particle is born heading against the wave's motion rather than along it.\n" +
      'PARTICLES.mnu: emit neg prob',
  },
  emitVelZscale: {
    min: 0, max: 1, step: 0.01,
    help: "How much of the velocity at birth along the camera's axis is kept: at 0, the default, particles are born " +
      "moving within the screen's plane.\n" +
      'PARTICLES.mnu: emit vel zscale',
  },
  emitPerFrame: {
    min: 0, max: 100, step: 0.1,
    help: 'How many particles a burst gives birth to. The console truncates it to a whole number: 16 by default.\n' +
      'PARTICLES.mnu: emit per frame',
  },
  emitProb: {
    min: 0, max: 1, step: 0.001,
    help: 'Chance each frame of a burst of Emit Per Frame births, about 7.7 a frame on average by default. ' +
      'Occasional sweeps along a line of the wave add more.\n' +
      'PARTICLES.mnu: emit prob',
  },
  agingSpeed: {
    min: 0.0002, max: 0.02, step: 0.00001,
    help: 'How fast particles age: life goes from 0 to 1 by this much a frame, so a particle lasts about 1 / this ' +
      'frames, some 6 s by default.\n' +
      'PARTICLES.mnu: aging speed',
  },
  agingVariance: {
    min: 0, max: 1, step: 0.001,
    help: "How far each particle's aging rate strays from Aging Speed, as a fraction either way: 235 to 692 frames " +
      'of life at the defaults.\n' +
      'PARTICLES.mnu: aging variance',
  },
  friction: {
    min: 0, max: 1, step: 0.0005,
    help: "Drag on the particles' velocity: the higher, the sooner they slow down.\n" +
      'PARTICLES.mnu: friction',
  },
  spinTimeScale: {
    min: 0, max: 20, step: 0.01,
    help: 'How fast the flakes tumble. The axis they turn about shifts as they age.\n' +
      'PARTICLES.mnu: spin time scale',
  },
  deltaTime: {
    min: 0.001, max: 0.1, step: 0.0001,
    help: "The time step of the particles' motion: the larger, the faster they move. How fast they age does not " +
      'depend on it.\n' +
      'PARTICLES.mnu: delta time',
  },
  gravity: {
    min: -0.01, max: 0.01, step: 0.00001,
    help: 'A constant vertical force on every particle; negative pulls down.\n' +
      'PARTICLES.mnu: gravity',
  },
  windDirX: {
    min: -1, max: 1, step: 0.001,
    help: "X of the wind's direction, normalised with Y and Z. The wind only blows while Wind Scale or Wind Scale 10 " +
      'is above 0, as in the Game boot and Welcome sets.\n' +
      'PARTICLES.mnu: wind dir x',
  },
  windDirY: {
    min: -1, max: 1, step: 0.001,
    help: "Y of the wind's direction, normalised with X and Z.\n" +
      'PARTICLES.mnu: wind dir y',
  },
  windDirZ: {
    min: -1, max: 1, step: 0.001,
    help: "Z of the wind's direction, normalised with X and Y.\n" +
      'PARTICLES.mnu: wind dir z',
  },
  windScale: {
    min: 0, max: 0.01, step: 0.00001,
    help: 'Strength of the wind, plus ten times Wind Scale 10: a steady push along Wind Dir on every particle.\n' +
      'PARTICLES.mnu: wind scale',
  },
  windScale10: {
    min: 0, max: 0.01, step: 0.00001,
    help: 'Adds ten times itself to Wind Scale.\n' +
      'PARTICLES.mnu: wind scale 10',
  },
  brownianScale: {
    min: 0, max: 2, step: 0.001,
    help: "Strength of each particle's random drift. Its generators restart every frame, so a particle keeps much " +
      'the same drift until particles ahead of it in the pool die or are born. D-pad steps and shakes raise it.\n' +
      'PARTICLES.mnu: brownian scale',
  },
  spotPosX: {
    min: -20, max: 20, step: 0.01,
    help: 'X of the spot light the flakes are lit by, in world units: the camera sits at (0, 0, 2) looking down ' +
      '-z.\n' +
      'PARTICLES.mnu: spot pos x',
  },
  spotPosY: {
    min: -20, max: 20, step: 0.01,
    help: 'Y of the spot light the flakes are lit by.\n' +
      'PARTICLES.mnu: spot pos y',
  },
  spotPosZ: {
    min: -30, max: 10, step: 0.01,
    help: 'Z of the spot light the flakes are lit by.\n' +
      'PARTICLES.mnu: spot pos z',
  },
  spotAttnX: {
    min: 0, max: 4, step: 0.01,
    help: "Constant term of the light's falloff: its light is divided by X + Y × distance + Z × distance².\n" +
      'PARTICLES.mnu: spot attn x',
  },
  spotAttnY: {
    min: 0, max: 2, step: 0.001,
    help: "Linear term of the light's falloff (see Spot Attn X).\n" +
      'PARTICLES.mnu: spot attn y',
  },
  spotAttnZ: {
    min: 0, max: 1, step: 0.001,
    help: "Quadratic term of the light's falloff (see Spot Attn X).\n" +
      'PARTICLES.mnu: spot attn z',
  },
  specularPower: {
    min: 1, max: 200, step: 0.1,
    help: 'Shininess of the flakes: the higher, the narrower the angle at which a flake glints. It also sizes the ' +
      'glare.\n' +
      'PARTICLES.mnu: specular power',
  },
  specularCoeff: {
    min: 0, max: 300, step: 0.1,
    help: "Strength of the flakes' mirror-like glint, the part that carries the iridescent colour.\n" +
      'PARTICLES.mnu: specular coeff',
  },
  lambertCoeff: {
    min: 0, max: 50, step: 0.1,
    help: "Strength of the flakes' diffuse lighting, which is always white.\n" +
      'PARTICLES.mnu: lambert coeff',
  },
  exposure: {
    min: 0, max: 0.5, step: 0.0005,
    help: "Exposure of the flakes' light: brightness is 1 - e^(-exposure × light), so higher is brighter until it " +
      'saturates.\n' +
      'PARTICLES.mnu: exposure',
  },
  fresnel: {
    min: 0.1, max: 8, step: 0.01,
    help: 'How long a flake stays opaque as it turns edge-on to the camera: the higher, the later it fades. Edge-on ' +
      'it always vanishes.\n' +
      'PARTICLES.mnu: fresnel',
  },
  colorControl: {
    min: 0, max: 1, step: 0.01,
    help: 'Colour of the glint: 0 white, 1 the iridescent colours, and a mix in between.\n' +
      'PARTICLES.mnu: color_control',
  },
  iridescentExp: {
    min: 0.1, max: 8, step: 0.01,
    help: 'Exponent on the iridescent colour: above 1 deepens it, below 1 washes it out.\n' +
      'PARTICLES.mnu: iridescent exp',
  },
  globalAlpha: {
    min: 0, max: 2, step: 0.01,
    help: 'Overall opacity of the particles; 0 hides them.\n' +
      'PARTICLES.mnu: global alpha',
  },
  sizeMiddle: {
    min: 0, max: 0.5, step: 0.0005,
    help: 'Size of a flake in focus, in world units.\n' +
      'PARTICLES.mnu: size middle',
  },
  sizeNear: {
    min: 0, max: 0.5, step: 0.0005,
    help: 'Size of a flake fully near-blurred, close to the camera, reached from Size Middle as the blur comes in.\n' +
      'PARTICLES.mnu: size near',
  },
  sizeFar: {
    min: 0, max: 3, step: 0.001,
    help: 'Size of a flake fully far-blurred, in the distance: at the defaults, the big, soft discs.\n' +
      'PARTICLES.mnu: size far',
  },
  nearFocus: {
    min: 0, max: 20, step: 0.01,
    help: 'Distance from the camera within which flakes are fully near-blurred; the blur clears by Near Focus + Near ' +
      'Focus Dist.\n' +
      'PARTICLES.mnu: near focus',
  },
  nearFocusDist: {
    min: 0.01, max: 10, step: 0.01,
    help: 'How far beyond Near Focus the near blur takes to clear.\n' +
      'PARTICLES.mnu: near focus_dist',
  },
  nearFocusPow: {
    min: 0.1, max: 8, step: 0.01,
    help: 'Exponent shaping the near blur: the higher, the sooner it clears.\n' +
      'PARTICLES.mnu: near focus_pow',
  },
  nearDarkness: {
    min: 0, max: 40, step: 0.01,
    help: 'How much the near blur dims flakes that look big on screen.\n' +
      'PARTICLES.mnu: near darkness',
  },
  nearFuzziness: {
    min: 0.1, max: 10, step: 0.01,
    help: "How a near-blurred flake's blob is filled: the higher, the brighter and more even out to its rim; the " +
      'lower, the dimmer, fading from its centre.\n' +
      'PARTICLES.mnu: near fuzziness',
  },
  farFocus: {
    min: 0, max: 30, step: 0.01,
    help: 'Distance from the camera where the far blur begins; it is full by Far Focus + Far Focus Dist. Flakes a ' +
      'little closer fade out briefly, hiding their turn to face the camera.\n' +
      'PARTICLES.mnu: far focus',
  },
  farFocusDist: {
    min: 0.01, max: 20, step: 0.01,
    help: 'How far beyond Far Focus the far blur takes to become full.\n' +
      'PARTICLES.mnu: far focus_dist',
  },
  farFocusPow: {
    min: 0.1, max: 8, step: 0.01,
    help: 'Exponent shaping the far blur: the higher, the later it comes in.\n' +
      'PARTICLES.mnu: far focus_pow',
  },
  farDarkness: {
    min: 0, max: 40, step: 0.01,
    help: 'How much the far blur dims flakes that look big on screen.\n' +
      'PARTICLES.mnu: far darkness',
  },
  nearAlign: {
    min: 0, max: 2, step: 0.001,
    help: 'How much near-blurred flakes turn to face the camera instead of tumbling.\n' +
      'PARTICLES.mnu: near align',
  },
  sizeAlign: {
    min: 0, max: 40, step: 0.01,
    help: 'How much flakes that look big on screen turn to face the camera.\n' +
      'PARTICLES.mnu: size align',
  },
  glare: {
    min: 0, max: 2, step: 0.001,
    help: 'Strength of the glare, the glint drawn over a flake while it mirrors the light into the camera.\n' +
      'PARTICLES.mnu: glare',
  },
  glareScale: {
    min: 0, max: 20, step: 0.01,
    help: 'Size of a glare relative to its flake.\n' +
      'PARTICLES.mnu: glare scale',
  },
  glareP1: {
    min: 0.1, max: 4, step: 0.01,
    help: "Shape of the glare's falloff from its centre: the exponent on the distance in e^(-P2 × distance^P1).\n" +
      'PARTICLES.mnu: glare p1',
  },
  glareP2: {
    min: 0, max: 20, step: 0.01,
    help: 'How fast the glare falls off from its centre: the higher, the smaller and sharper the glint.\n' +
      'PARTICLES.mnu: glare p2',
  },
  uiBrownian: {
    min: 0, max: 4, step: 0.01,
    help: 'How much D-pad steps and shakes raise the drift: a level from 0 to 1, which each sideways step or shake ' +
      'kicks, times this is added to Brownian Scale.\n' +
      'PARTICLES_UI.mnu: brownian',
  },
  rshakeBrw: {
    min: 0, max: 20, step: 0.01,
    help: "How much moving the controller raises the drift: the change in its accelerometer's reading from one frame " +
      'to the next, times this, is added to Brownian Scale.\n' +
      'PARTICLES_UI.mnu: rshake brw',
  },
  dpadRotMax: {
    min: 0, max: 0.002, step: 0.000001,
    help: 'How fast D-pad steps turn the whole field of particles. A single tap is too small to turn it; a held ' +
      'direction does.\n' +
      'PARTICLES_UI.mnu: dpad rot max',
  },
  dpadScaleX: {
    min: 0, max: 2, step: 0.01,
    help: "Weight of left and right steps in the field's turn and in the drift they kick.\n" +
      'PARTICLES_UI.mnu: dpad scale x',
  },
  dpadScaleY: {
    min: 0, max: 2, step: 0.01,
    help: "Weight of up and down steps in the field's turn and in the drift they kick. At 0, the default, they do " +
      'neither, and stop any turn in progress.\n' +
      'PARTICLES_UI.mnu: dpad scale y',
  },
  dshakeRotMax: {
    min: 0, max: 0.002, step: 0.000001,
    help: 'How fast a shake turns the field of particles.\n' +
      'PARTICLES_UI.mnu: dshake rot max',
  },
  dshakeRotImp: {
    min: 0, max: 2, step: 0.001,
    help: "How strongly a detected shake kicks the field's turn.\n" +
      'PARTICLES_UI.mnu: dshake rot imp',
  },
  dshakeBrwImp: {
    min: 0, max: 1, step: 0.001,
    help: 'How much a detected shake raises the drift.\n' +
      'PARTICLES_UI.mnu: dshake brw imp',
  },
  dshakeThresh: {
    min: 0, max: 4, step: 0.001,
    help: 'How hard the controller must be shaken for a shake to be detected.\n' +
      'PARTICLES_UI.mnu: dshake thresh',
  },
  dshakeXCoeff: {
    min: 0, max: 2, step: 0.001,
    help: "Weight of the turn from shakes the accelerometer's sideways axis detects.\n" +
      'PARTICLES_UI.mnu: dshake x coeff',
  },
  dshakeGCoeff: {
    min: 0, max: 2, step: 0.001,
    help: 'Weight of the turn from shakes the gyro detects.\n' +
      'PARTICLES_UI.mnu: dshake g coeff',
  },
  iconWind: {
    min: 0, max: 50, step: 0.01,
    help: 'Strength of the wind the icons raise as they move: it pushes the particles near them the way they go, and ' +
      'dies away within about 1.4 s.\n' +
      'PARTICLES_UI.mnu: icon wind',
  },
  iconWindSclX: {
    min: 0, max: 2, step: 0.01,
    help: 'Horizontal part of the icon wind: 0 by default, so icons moving sideways raise none.\n' +
      'PARTICLES_UI.mnu: icon wind scl x',
  },
  iconWindSclY: {
    min: 0, max: 2, step: 0.01,
    help: 'Vertical part of the icon wind: icons moving up or down push the particles the same way.\n' +
      'PARTICLES_UI.mnu: icon wind scl y',
  },
  iridescentPalette: {
    type: 'palette', min: 3, max: 32,
    help: "The colours the glint runs through, in a loop: a flake takes its colour from the way it tilts, and it " +
      'shifts a little further along as the flake tilts further, as the disc shows, its top for a flake facing up. ' +
      'Each swatch opens a colour picker; + adds a colour between the last and the first, - takes the last away. It ' +
      'reaches the screen as it is while Color Control and Iridescent Exp are 1, as on the console.\n' +
      'proc_iridescent.tga, fitted: a periodic Catmull-Rom loop through 16 colours',
  },
  videoOutput: {
    type: 'select',
    options: [{ value: '1080', label: '1080p' }, { value: '720', label: '720p' }, { value: '480', label: 'SD' }],
    help: "The console's video output: below 1080p the flakes in focus are drawn bigger and all the particles " +
      'fainter - at the defaults, a fifth bigger and 28% fainter at 720p, two fifths bigger and 56% fainter on SD - ' +
      'and on SD the whole scene is drawn at 0.8, under the colour the interface is drawn with.\n' +
      "Console state: PARTICLES_SPE.mnu's second factor, and paf's colour on SD (_Gamma)",
  },
  whatsNewBoard: {
    hidden: "Closed: which of the XMB's actions open the board is not pursued (docs/particles/scene-events.md), and " +
      'focusing it hides the scene anyway; closed, as in every capture',
    type: 'select', options: [{ value: 'closed', label: 'Closed' }, { value: 'open', label: 'Open' }],
    help: "What's New's board: opening its list speeds the particles up, widens their glint and strengthens their " +
      'glare, over 2 s; closing it takes them back.\n' +
      "Console state: PARTICLES_SPE.mnu's first factor",
  },
  musicPlayback: {
    type: 'select', options: [{ value: 'stopped', label: 'Stopped' }, { value: 'playing', label: 'Playing' }],
    help: 'Music playing in the XMB, its visualizer showing the scene: the particles and the wave go to the music ' +
      'set over 2 s and hold it whatever the hour, the wave rising and coming forward. When it stops they go to ' +
      'the base set (PARTICLES.mnu, LINE1.mnu) over 5.5 s, then back to the theme over 1 s.\n' +
      "Console state: the scene's music event (event 4) and the visualizer's view (soundvisualizer_plugin)",
  },
  themeBrightness: {
    type: 'select',
    options: [
      { value: '0', label: 'Normal' }, { value: '1', label: '-1' }, { value: '2', label: '-2' },
      { value: '3', label: '-3' }, { value: '4', label: '-4' }, { value: '5', label: '-5' },
    ],
    help: "Theme Settings' Brightness: each step below Normal takes another 15% off the scene's brightness - the " +
      "particles', the backdrop's and the wave's - fading over 1 s.\n" +
      'Console state: Theme Settings',
  },
  themeColor: {
    type: 'select',
    options: [
      { value: '0', label: 'Original' }, { value: '1', label: 'January' }, { value: '2', label: 'February' },
      { value: '3', label: 'March' }, { value: '4', label: 'April' }, { value: '5', label: 'May' },
      { value: '6', label: 'June' }, { value: '7', label: 'July' }, { value: '8', label: 'August' },
      { value: '9', label: 'September' }, { value: '10', label: 'October' }, { value: '11', label: 'November' },
      { value: '12', label: 'December' },
    ],
    help: "Theme Settings' Colour: a month stops the scene's clock at noon on the 1st of that month, whatever the " +
      "date and hour, so the Auto theme holds the day set and the backdrop's Auto gradient that month's daytime " +
      'colour. Original lets the clock run. The particles and the wave take a change over 1 s.\n' +
      'Console state: Theme Settings (registry key 0x5f)',
  },
  xmbBackground: {
    type: 'select', options: [{ value: 'shown', label: 'Shown' }, { value: 'hidden', label: 'Hidden' }],
    help: 'Hidden stands for another module taking the screen, as a video, the browser or the Store do: the ' +
      'whole scene fades to black over Background Fade Ms. Shown brings it back.\n' +
      "Console state: the XMB's background",
  },
  backgroundFadeMs: {
    min: 0, max: 1000, step: 10,
    help: "How long the scene takes to fade out, or back in, when the XMB's background is hidden or shown.\n" +
      "Page setting: the console's callers use 0 to 1000 ms",
  },
  flowStrength: {
    hidden: 'A firmware constant, not a parameter: the PPU always writes 1',
    min: 0, max: 2, step: 0.0005,
    help: 'Strength of the flow grid, which carries the icon wind to the particles.\n' +
      'Firmware constant: the PPU always writes 1',
  },
  iconEaseSec: {
    hidden: "Modelled and closed: how the XMB moves its icons is not pursued, and the page draws none " +
      '(docs/particles/flow-grid.md)',
    min: 0.005, max: 1, step: 0.005,
    help: "Time constant of the modelled icons' easing after a step: the shorter, the faster they move, and faster " +
      'icons raise a stronger wind.\n' +
      'Modelled: how the XMB moves its icons is not traced',
  },
  verticalSteps: {
    type: 'select', options: [{ value: 'turn', label: 'Turn the field' }, { value: 'scroll', label: 'Scroll items' }],
    help: 'What the up and down arrows do. Turn: they turn the whole field up or down and kick its drift, as left ' +
      'and right do sideways, with Dpad Scale X\'s weight; held with a left or right arrow, they turn it ' +
      'diagonally, no faster than one arrow alone. Scroll: as on the console, whose Dpad Scale Y of 0 ' +
      "leaves them no turn: they only scroll the selected category's unseen items, which raises a wind under them.\n" +
      'Page setting: the arrow keys stand in for the D-pad',
  },
  mouseAccelToG: {
    min: 0, max: 0.2, step: 0.001,
    help: 'How strongly dragging the mouse shakes the virtual controller: an acceleration of one screen height per ' +
      'second² reads as this many g on its accelerometer.\n' +
      'Page setting: the mouse stands in for the Sixaxis',
  },
  mouseYawGain: {
    min: 0, max: 1, step: 0.005,
    help: 'How strongly dragging sideways turns the virtual controller: a speed of one screen height a second reads ' +
      'this much on its gyro, whose reading runs from -1 to 1.\n' +
      'Page setting: the mouse stands in for the Sixaxis',
  },
  pointerWind: {
    min: 0, max: 1, step: 0.05,
    help: 'How hard the mouse pointer pushes the particles it passes over: it writes its motion into the flow grid ' +
      'as a moving icon does, times this, and in both directions where the icons push only up and down. 0 turns ' +
      'it off.\nPage setting: the console has no pointer',
  },
};

// Where the panel shows each setting: its tab and its section, in this order. A setting left out lands in an Other
// section of the Particles tab; one whose meta says `hidden` stays off the panel.
window.PARTICLE_SETTINGS_LAYOUT = [
  { tab: 'Scene', section: 'Scene', keys: ['theme', 'sequence', 'musicPlayback'] },
  { tab: 'Scene', section: 'Theme Settings', keys: ['themeBrightness', 'themeColor'] },
  { tab: 'Scene', section: 'Display', keys: ['videoOutput', 'xmbBackground', 'backgroundFadeMs', 'whatsNewBoard'] },
  { tab: 'Input', section: 'Arrow keys', keys: ['verticalSteps', 'dpadRotMax', 'dpadScaleX', 'dpadScaleY'] },
  { tab: 'Input', section: 'Mouse', keys: ['pointerWind', 'mouseAccelToG', 'mouseYawGain'] },
  { tab: 'Input', section: 'Motion and shakes', keys: ['uiBrownian', 'rshakeBrw', 'dshakeRotMax', 'dshakeRotImp',
    'dshakeBrwImp', 'dshakeThresh', 'dshakeXCoeff', 'dshakeGCoeff'] },
  { tab: 'Input', section: 'Icon wind', keys: ['iconWind', 'iconWindSclX', 'iconWindSclY', 'iconEaseSec',
    'flowStrength'] },
  { tab: 'Particles', section: 'Emission', keys: ['emitVelMin', 'emitVelMul', 'emitVelVar', 'emitConeAngle',
    'emitNegProb', 'emitVelZscale', 'emitPerFrame', 'emitProb'] },
  { tab: 'Particles', section: 'Life', keys: ['agingSpeed', 'agingVariance'] },
  { tab: 'Particles', section: 'Motion', keys: ['friction', 'spinTimeScale', 'deltaTime', 'gravity', 'windDirX',
    'windDirY', 'windDirZ', 'windScale', 'windScale10', 'brownianScale'] },
  { tab: 'Particles', section: 'Lighting', keys: ['spotPosX', 'spotPosY', 'spotPosZ', 'spotAttnX', 'spotAttnY',
    'spotAttnZ', 'specularPower', 'specularCoeff', 'lambertCoeff', 'exposure', 'fresnel', 'globalAlpha'] },
  { tab: 'Particles', section: 'Iridescence', keys: ['colorControl', 'iridescentExp', 'iridescentPalette'] },
  { tab: 'Particles', section: 'Size and focus', keys: ['sizeMiddle', 'sizeNear', 'sizeFar', 'nearFocus',
    'nearFocusDist', 'nearFocusPow', 'nearDarkness', 'nearFuzziness', 'farFocus', 'farFocusDist', 'farFocusPow',
    'farDarkness'] },
  { tab: 'Particles', section: 'Alignment and glare', keys: ['nearAlign', 'sizeAlign', 'glare', 'glareScale',
    'glareP1', 'glareP2'] },
];
