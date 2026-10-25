//! The settings, as `spline-settings.js` and `particles-settings.js` hold them: LINE1.mnu's, HDR.mnu's and
//! BACKGROUND.mnu's parameters for the wave, PARTICLES.mnu's and PARTICLES_UI.mnu's for the particles, under the
//! page's own camelCase names with the firmware's values, plus the console's state the scene is drawn under. Read by
//! every layer, written by `scene.rs` and shown by `panel.rs`; declarative only.

/// Name-based access, which the scene's sets and the panel go through.
pub trait Params {
    fn get(&self, key: &str) -> Option<f64>;
    fn get_mut(&mut self, key: &str) -> Option<&mut f64>;
    fn keys() -> &'static [&'static str];
}

macro_rules! settings_struct {
    ($ty:ident { $( $field:ident : $key:literal = $default:expr, )* }) => {
        #[derive(Clone, Debug)]
        pub struct $ty { $( pub $field: f64, )* }
        impl Default for $ty {
            fn default() -> Self { Self { $( $field: $default, )* } }
        }
        impl Params for $ty {
            fn get(&self, key: &str) -> Option<f64> {
                match key { $( $key => Some(self.$field), )* _ => None }
            }
            fn get_mut(&mut self, key: &str) -> Option<&mut f64> {
                match key { $( $key => Some(&mut self.$field), )* _ => None }
            }
            fn keys() -> &'static [&'static str] { &[ $( $key, )* ] }
        }
    };
}

settings_struct!(WaveSettings {
    // LINE1.mnu, the base set, which the scene's sets are differences from.
    damping: "damping" = 0.0001,
    length: "length" = 0.306001,
    tension: "tension" = 0.25,
    timestep: "timestep" = 4.0,
    perturbation: "perturbation" = 0.0998587,
    end_x: "endX" = 3.0,
    end_y: "endY" = 0.4,
    end_z: "endZ" = 0.2,
    pos_x: "posX" = -7.67933,
    pos_y: "posY" = -1.08844,
    pos_z: "posZ" = -6.40287,
    ang_x: "angX" = -0.994157,
    ang_y: "angY" = 0.0867576,
    ang_z: "angZ" = 0.065364,
    ang_rot: "angRot" = 18.1208,
    ffd_scale1_x: "ffdScale1X" = 5.67726,
    ffd_scale1_y: "ffdScale1Y" = 1.00077,
    ffd_scale1_z: "ffdScale1Z" = 1.0,
    ffd_scale2_x: "ffdScale2X" = 2.82755,
    ffd_scale2_y: "ffdScale2Y" = 1.27579,
    ffd_scale2_z: "ffdScale2Z" = 2.88782,
    ffd_offset_x: "ffdOffsetX" = 0.0,
    ffd_offset_y: "ffdOffsetY" = -0.469999,
    ffd_offset_z: "ffdOffsetZ" = 0.0,
    ffd_param1: "ffdParam1" = -1.33509,
    brightness: "brightness" = 0.701917,
    mipmap_bias: "mipmapBias" = 1.86707,
    fresnel: "fresnel" = 0.638971,
    falloff: "falloff" = 1.00318,
    spacing: "spacing" = 407.658,
    thinness: "thinness" = 1.0,
    // HDR.mnu, the base set; DITHER is the code's default, which the files that set it set too.
    exposure: "exposure" = 1.05,
    white_level: "whiteLevel" = 0.899181,
    glare_level: "glareLevel" = 1.10245,
    glare_thresh: "glareThresh" = 0.738857,
    gaussian_rad_r: "gaussianRadR" = 1.23888,
    gaussian_rad_g: "gaussianRadG" = 1.43176,
    gaussian_rad_b: "gaussianRadB" = 1.55787,
    glare_sum_pow: "glareSumPow" = 0.557478,
    dither: "dither" = 0.00392157,
    // BACKGROUND.mnu's four colours: 1 and 2 the backdrop's at the top and the bottom, 3 and 4 the wave's at the left
    // and the right.
    colour1_red: "colour1Red" = 1.0,
    colour1_green: "colour1Green" = 1.0,
    colour1_blue: "colour1Blue" = 1.0,
    colour2_red: "colour2Red" = 1.0,
    colour2_green: "colour2Green" = 1.0,
    colour2_blue: "colour2Blue" = 1.0,
    colour3_red: "colour3Red" = 0.847347,
    colour3_green: "colour3Green" = 0.846155,
    colour3_blue: "colour3Blue" = 0.846448,
    colour4_red: "colour4Red" = 0.92487,
    colour4_green: "colour4Green" = 0.922603,
    colour4_blue: "colour4Blue" = 0.922933,
    // BACKGROUND.mnu's backdrop: the base file leaves these out, so the code's defaults stand for them. DAYSPREAD's, 0,
    // lies below its range, which the scene holds it to, so it reads as 1.
    night_blend: "nightBlend" = 1.0,
    night2day_begin: "night2dayBegin" = 4.0,
    night2day_end: "night2dayEnd" = 6.0,
    day2night_begin: "day2nightBegin" = 18.0,
    day2night_end: "day2nightEnd" = 20.0,
    dayspread: "dayspread" = 0.0,
    night_whit_bias: "nightWhitBias" = 0.5,
    colour_shader: "colourShader" = 0.0,
});

settings_struct!(ParticleSettings {
    // PARTICLES.mnu (firmware 4.93 defaults)
    emit_vel_min: "emitVelMin" = 0.15064,
    emit_vel_mul: "emitVelMul" = 0.19,
    emit_vel_var: "emitVelVar" = 0.282567,
    emit_cone_angle: "emitConeAngle" = 51.8695,
    emit_neg_prob: "emitNegProb" = 0.173899,
    emit_vel_zscale: "emitVelZscale" = 0.0,
    emit_per_frame: "emitPerFrame" = 16.6539,
    emit_prob: "emitProb" = 0.479115,
    aging_speed: "agingSpeed" = 0.00285223,
    aging_variance: "agingVariance" = 0.493003,
    friction: "friction" = 0.030551,
    spin_time_scale: "spinTimeScale" = 2.74,
    delta_time: "deltaTime" = 0.0088883,
    gravity: "gravity" = -6.8e-05,
    wind_dir_x: "windDirX" = 0.340188,
    wind_dir_y: "windDirY" = 0.0,
    wind_dir_z: "windDirZ" = 0.35,
    wind_scale: "windScale" = 0.0,
    wind_scale10: "windScale10" = 0.0,
    brownian_scale: "brownianScale" = 0.225311,
    spot_pos_x: "spotPosX" = 4.16,
    spot_pos_y: "spotPosY" = 2.63,
    spot_pos_z: "spotPosZ" = -7.6,
    spot_attn_x: "spotAttnX" = 1.0,
    spot_attn_y: "spotAttnY" = 0.0,
    spot_attn_z: "spotAttnZ" = 0.0,
    specular_power: "specularPower" = 35.2904,
    specular_coeff: "specularCoeff" = 74.74,
    lambert_coeff: "lambertCoeff" = 8.0,
    exposure: "exposure" = 0.0390458,
    fresnel: "fresnel" = 1.33319,
    color_control: "colorControl" = 1.0,
    iridescent_exp: "iridescentExp" = 1.0,
    global_alpha: "globalAlpha" = 1.0,
    size_middle: "sizeMiddle" = 0.0536233,
    size_near: "sizeNear" = 0.0772033,
    size_far: "sizeFar" = 0.874062,
    near_focus: "nearFocus" = 6.12435,
    near_focus_dist: "nearFocusDist" = 1.3193,
    near_focus_pow: "nearFocusPow" = 2.47206,
    near_darkness: "nearDarkness" = 6.24028,
    near_fuzziness: "nearFuzziness" = 3.79,
    far_focus: "farFocus" = 12.8327,
    far_focus_dist: "farFocusDist" = 3.54129,
    far_focus_pow: "farFocusPow" = 1.83324,
    far_darkness: "farDarkness" = 13.9,
    near_align: "nearAlign" = 0.722,
    size_align: "sizeAlign" = 13.19,
    glare: "glare" = 0.159705,
    glare_scale: "glareScale" = 5.44386,
    glare_p1: "glareP1" = 0.99,
    glare_p2: "glareP2" = 4.44,
    // PARTICLES_UI.mnu (firmware 4.93 defaults)
    ui_brownian: "uiBrownian" = 0.6,
    rshake_brw: "rshakeBrw" = 7.4992,
    dpad_rot_max: "dpadRotMax" = 0.000116654,
    dpad_scale_x: "dpadScaleX" = 1.0,
    dpad_scale_y: "dpadScaleY" = 0.0,
    dshake_rot_max: "dshakeRotMax" = 0.000124431,
    dshake_rot_imp: "dshakeRotImp" = 0.513834,
    dshake_brw_imp: "dshakeBrwImp" = 0.061702,
    dshake_thresh: "dshakeThresh" = 0.722145,
    dshake_x_coeff: "dshakeXCoeff" = 0.402735,
    dshake_g_coeff: "dshakeGCoeff" = 0.444397,
    icon_wind: "iconWind" = 11.3877,
    icon_wind_scl_x: "iconWindSclX" = 0.0,
    icon_wind_scl_y: "iconWindSclY" = 1.0,
    // The console's state, the modelled PPU side and the mouse adapter (not from the firmware).
    background_fade_ms: "backgroundFadeMs" = 10000.0,
    flow_strength: "flowStrength" = 1.0,
    icon_ease_sec: "iconEaseSec" = 0.065,
    mouse_accel_to_g: "mouseAccelToG" = 0.04,
    mouse_yaw_gain: "mouseYawGain" = 0.15,
    pointer_wind: "pointerWind" = 0.5,
});

settings_struct!(NativeSettings {
    // The native app's own, with no page or firmware counterpart: enhancements, each off or at the console's look by
    // default but for the glare's mode. Auto exposure is `auto_exposure.rs`'s; the rest are `postprocess.rs`'s.
    // Switches and choices are numbers, as the panel's selects hold them.
    ae_enabled: "aeEnabled" = 1.0,
    ae_max_push: "aeMaxPush" = 0.99,
    ae_max_pull: "aeMaxPull" = 0.3,
    ae_pull_above: "aePullAbove" = 0.99,
    ae_metering: "aeMetering" = 1.0,
    ae_source: "aeSource" = 1.0,
    ae_highlight_target: "aeHighlightTarget" = 0.95,
    ae_luminance_target: "aeLuminanceTarget" = 0.9,
    ae_zone: "aeZoneSize" = 128.0,
    ae_percentile: "aePercentile" = 0.97,
    ae_push_time: "aePushTime" = 1.0,
    ae_pull_time: "aePullTime" = 2.0,
    ae_start_delay: "aeStartDelaySec" = 7.0,
    ae_build_up: "aeBuildUpSec" = 14.0,
    tonemap: "tonemap" = 3.0,
    tm_gain: "tmGain" = 1.0,
    tm_match: "tmMatchMidGrey" = 1.0,
    tm_saturation: "tmSaturation" = 1.0,
    tm_agx_look: "tmAgxLook" = 0.0,
    tm_gt_max: "tmGtMaxBrightness" = 1.0,
    tm_gt_contrast: "tmGtContrast" = 1.0,
    tm_gt_linear_start: "tmGtLinearStart" = 0.22,
    tm_gt_linear_length: "tmGtLinearLength" = 0.4,
    tm_gt_black: "tmGtBlackTightness" = 1.33,
    tm_gt_pedestal: "tmGtPedestal" = 0.0,
    tm_hable_white: "tmHableWhite" = 11.2,
    tm_reinhard_white: "tmReinhardWhite" = 4.0,
    glare_mode: "glareMode" = 1.0,
    bloom_strength: "bloomStrength" = 1.0,
    bloom_falloff: "bloomFalloff" = 1.2,
    bloom_octaves: "bloomOctaves" = 8.0,
    bloom_tint: "bloomTint" = 1.0,
    glare_radius_on: "glareRadiusScaleOn" = 0.0,
    glare_radius_scale: "glareRadiusScale" = 2.0,
    noise_enabled: "noiseEnabled" = 1.0,
    noise_mode: "noiseMode" = 1.0,
    grain_amount: "grainAmount" = 0.13,
    grain_size: "grainSize" = 1.5,
    grain_iso: "grainIso" = 800.0,
    noise_read: "noiseRead" = 0.0005,
    noise_shot: "noiseShot" = 0.004,
    noise_chroma: "noiseChroma" = 0.5,
    noise_size: "noiseSize" = 1.0,
    particle_limit: "particleLimit" = 2049.0,
    maru_chan: "maruChan" = 1.0,
    maru_first_min: "maruFirstMin" = 0.5,
    maru_every_min: "maruEveryMin" = 0.5,
    maru_music_only: "maruMusicOnly" = 1.0,
    maru_cross_sec: "maruCrossSec" = 30.0,
    maru_size_px: "maruSizePx" = 95.0,
    maru_steam: "maruSteam" = 7.0,
    maru_smoke: "maruSmoke" = 1.0,
    maru_supersample: "maruSupersample" = 4.0,
    maru_motion_blur: "maruMotionBlur" = 1.0,
    maru_buoyancy: "maruBuoyancy" = 1.0,
    maru_tilt: "maruTilt" = 1.0,
    maru_tilt_bounce: "maruTiltBounce" = 1.0,
    maru_bass_bounce: "maruBassBounce" = 1.0,
    maru_saturation: "maruSaturation" = 0.0,
    maru_flat: "maruFlat" = 1.0,
    maru_scene_light: "maruSceneLight" = 1.0,
    maru_scene_tint: "maruSceneTint" = 0.5,
    maru_at: "maruAt" = -1.0,
    epilepsy_warning: "epilepsyWarning" = 1.0,
    epilepsy_warning_every: "epilepsyWarningEvery" = 1.0,
    epilepsy_warning_hold_sec: "epilepsyWarningHoldSec" = 4.0,
    epilepsy_warning_fade_sec: "epilepsyWarningFadeSec" = 0.42,
    epilepsy_warning_after_sec: "epilepsyWarningAfterSec" = 7.0,
    epilepsy_warning_blur_px: "epilepsyWarningBlurPx" = 7.0,
    epilepsy_warning_shadow: "epilepsyWarningShadow" = 0.6,
    epilepsy_warning_shadow_offset_x: "epilepsyWarningShadowOffsetX" = 0.0,
    epilepsy_warning_shadow_offset_y: "epilepsyWarningShadowOffsetY" = 1.0,
    epilepsy_warning_shadow_radius: "epilepsyWarningShadowRadius" = 3.5,
    epilepsy_warning_shadow_red: "epilepsyWarningShadowRed" = 0.0,
    epilepsy_warning_shadow_green: "epilepsyWarningShadowGreen" = 0.0,
    epilepsy_warning_shadow_blue: "epilepsyWarningShadowBlue" = 0.0,
    epilepsy_warning_font: "epilepsyWarningFont" = 3.0,
    epilepsy_warning_size: "epilepsyWarningSize" = 1.0,
    epilepsy_warning_width: "epilepsyWarningWidth" = 0.66,
    epilepsy_warning_x: "epilepsyWarningX" = 0.5,
    epilepsy_warning_y: "epilepsyWarningY" = 0.5,
    epilepsy_warning_text_blur: "epilepsyWarningTextBlur" = 0.0,
    epilepsy_warning_gloss: "epilepsyWarningGloss" = 0.13,
    // The first start's intro (`intro.rs`), after the benchmark and the warning.
    intro: "intro" = 1.0,
    // Season Pass: the backdrop's tour of the months, in place of the clock's (`scene::SeasonPass`).
    season_pass: "seasonPass" = 1.0,
    season_pass_hold_min: "seasonPassHoldMin" = 2.0,
    season_pass_fade_sec: "seasonPassFadeSec" = 20.0,
    season_pass_months: "seasonPassMonths" = 4095.0,
    benchmark_line_inset: "benchmarkLineInset" = 0.185,
    disco_ball: "discoBall" = 0.0,
    disco_after_min: "discoAfterMin" = 7.0,
    disco_bass_level: "discoBassLevel" = 0.69,
    disco_descend_sec: "discoDescendSec" = 30.0,
    disco_size_px: "discoSizePx" = 84.0,
    disco_x: "discoX" = 0.667,
    disco_flat: "discoFlat" = 1.0,
    disco_spin: "discoSpin" = 0.06,
    disco_beams: "discoBeams" = 1.0,
    disco_spots: "discoSpots" = 0.0,
    disco_glare: "discoGlare" = 1.0,
    disco_floor: "discoFloor" = 0.0,
    disco_beat_lights: "discoBeatLights" = 0.0,
    anti_alias: "antiAlias" = 2.0,
    supersample: "supersample" = 1.0,
    sharpen: "sharpen" = 1.0,
    sharpen_amount: "sharpenAmount" = 1.0,
    sharpen_strength: "sharpenStrength" = 0.5,
    desat_enabled: "desatEnabled" = 0.0,
    desat_start: "desatStart" = 1.0,
    desat_range: "desatRange" = 0.5,
    desat_whiten: "desatWhiten" = 1.0,
    // The scene's own state the app adds: the music detected from the system, and the music keeping the clock's colour.
    auto_music: "autoMusic" = 1.0,
    auto_music_hold: "autoMusicHoldSec" = 3.0,
    music_colour_choice: "musicColourChoice" = 0.0,
    music_colour_mix: "musicColourMix" = 0.5,
    // Listening to the system's output while the music holds the scene (audio.rs): the wave's brightness, risen with
    // how loud it has been for a while and flickering over it, and the camera bounced on the beat.
    audio_reactive: "audioReactive" = 1.0,
    audio_flicker: "audioFlicker" = 0.42,
    audio_flicker_rate: "audioFlickerRate" = 2.5,
    audio_follow: "audioFollow" = 0.5,
    audio_quiet: "audioQuiet" = -40.0,
    audio_loud: "audioLoud" = -20.0,
    audio_window: "audioWindow" = 10.0,
    audio_rise: "audioRise" = 3.0,
    audio_fall: "audioFall" = 6.0,
    audio_bass_boost: "audioBassBoost" = 100.0,
    tempo_bounce: "tempoBounce" = 1.0,
    tempo_bounce_amount: "tempoBounceAmount" = 0.02,
    tempo_activity: "tempoActivity" = 0.6,
    tempo_confidence: "tempoConfidence" = 0.2,
    tempo_onsets: "tempoOnsets" = 1.0,
    tempo_bass_weight: "tempoBassWeight" = 1.5,
    tempo_gate: "tempoGate" = 2.0,
    tempo_gate_drop: "tempoGateDrop" = 10.0,
    // The bass's own: its pitch, read between bassLow and bassHigh, flickering the brightness and shaking the camera
    // at its rate (times bassRate), as deep as it is loud; and a bop on each kick.
    bass_flicker: "bassFlicker" = 1.0,
    bass_flicker_depth: "bassFlickerDepth" = 0.03,
    bass_rate: "bassRate" = 0.5,
    bass_fold: "bassFold" = 1.0,
    bass_low: "bassLow" = 30.0,
    bass_high: "bassHigh" = 90.0,
    bass_quiet: "bassQuiet" = -40.0,
    bass_loud: "bassLoud" = 0.0,
    sub_shake: "subShake" = 1.0,
    sub_shake_amount: "subShakeAmount" = 0.042,
    kick_bounce: "kickBounce" = 1.0,
    kick_bounce_amount: "kickBounceAmount" = 0.02,
    kick_threshold: "kickThreshold" = 1.0,
    // The bass's warm-up: its effects open over bassWarmUp seconds once a sure beat has been heard for bassWarmDelay,
    // then answer at once until bassWarmHold minutes pass without music.
    bass_warm_delay: "bassWarmDelay" = 3.0,
    bass_warm_up: "bassWarmUp" = 8.0,
    bass_warm_hold: "bassWarmHold" = 3.0,
    // Motion blur for what the pokes and the music move: the frame drawn in sub-frames, up to this rate.
    motion_blur: "motionBlur" = 1.0,
    motion_blur_hz: "motionBlurHz" = 240.0,
    // The app's own resources (`cpu.rs`, `neighbour.rs`): where its threads run, and what it does while other programs
    // need the screen, the GPU's memory or the battery. The actions: 0 keep running, 1 limit the frame rate, 2 pause,
    // 3 stop.
    renderer: "renderer" = 1.0,
    gpu_wave: "gpuWave" = 1.0,
    render_scale: "renderScale" = 1.0,
    software_lite: "softwareLite" = 1.0,
    triple_buffering: "tripleBuffering" = 2.0,
    cpu_placement: "cpuPlacement" = 1.0,
    cpu_eco_qos: "cpuEcoQoS" = 1.0,
    cpu_priority: "cpuPriority" = 2.0,
    when_focused: "whenOtherFocused" = 0.0,
    when_maximized: "whenOtherMaximized" = 1.0,
    when_fullscreen: "whenOtherFullscreen" = 3.0,
    when_process: "whenProcessRunning" = 3.0,
    when_battery: "whenOnBattery" = 1.0,
    when_gpu_memory: "whenGpuMemoryLow" = 3.0,
    gpu_memory_threshold: "gpuMemoryThreshold" = 67.0,
    limit_fps: "limitFps" = 15.0,
    stop_transition: "stopTransition" = 1.0,
    stop_transition_urgent: "stopTransitionUrgent" = 2.0,
    resume_transition: "resumeTransition" = 1.0,
    stop_hide_window: "stopHideWindow" = 0.0,
    // The settings' own look (`blur.rs`, `panel.rs`): the blurred background behind them, and their size.
    blur_enabled: "blurEnabled" = 1.0,
    window_backdrop: "windowBackdrop" = 0.0,
    scene_blur_radius: "sceneBlurRadius" = 2.0,
    backdrop_tint: "backdropTint" = 0.45,
    scene_blur_saturation: "sceneBlurSaturation" = 1.25,
    scene_blur_grain: "sceneBlurGrain" = 0.02,
    window_theme: "windowTheme" = 0.0,
    window_corners: "windowCorners" = 0.0,
    acrylic_opacity: "acrylicOpacity" = 0.35,
    ui_scale: "uiScale" = 1.0,
    // How the settings are opened, besides the tray and H: poking the scene, which bounces the camera, and the old
    // right-click logo.
    pokes_to_open: "pokesToOpen" = 9.0,
    poke_bounce: "pokeBounce" = 0.09,
    logo_on_right_click: "logoOnRightClick" = 0.0,
});

impl NativeSettings {
    /// The peak auto exposure aims for, under its measure.
    pub fn ae_target(&self) -> f64 {
        if self.ae_metering != 0.0 {
            self.ae_luminance_target
        } else {
            self.ae_highlight_target
        }
    }

    /// What GAUSSIAN RAD R, G and B are multiplied by: the modifier, while it is on.
    pub fn glare_radius_factor(&self) -> f64 {
        if self.glare_radius_on != 0.0 {
            self.glare_radius_scale.max(0.0)
        } else {
            1.0
        }
    }
}

/// proc_iridescent.tga's colours, as 0xRRGGBB: 16 fitted to the firmware's texture (RMS error 13/255).
pub const IRIDESCENT_PALETTE: [u32; 16] = [
    0xf7f9bd, 0xf3dc60, 0xdc993c, 0xc06a49, 0xd4488a, 0xce34be, 0x964cde, 0x638af4, 0x357ce9, 0x496de7, 0x6d84ec,
    0xa482f5, 0xda85d8, 0xc58b73, 0xb6a856, 0xb3bba0,
];

/// The scene's state, which the particle settings hold on the page: the theme, the boot sequence and the console's
/// state the scene is drawn under. Their options are the page's.
#[derive(Clone, Debug)]
pub struct Scene {
    pub theme: &'static str,
    pub sequence: &'static str,
    pub music_playing: bool,
    pub theme_brightness: i32, // Theme Settings' Brightness: 0 (Normal) to 5 (-5)
    pub theme_color: i32,      // Theme Settings' Colour: 0 (Original) or a month, 1 to 12
    pub background_hidden: bool,
    pub video_output: i32, // 1080, 720 or 480 lines
    pub whats_new_open: bool,
    pub vertical_turn: bool, // up and down turn the field ('turn'), or scroll the items ('scroll')
    pub iridescent_palette: Vec<u32>,
}

impl Default for Scene {
    fn default() -> Self {
        Scene {
            theme: "auto",
            sequence: "none",
            music_playing: false,
            theme_brightness: 0,
            theme_color: 0,
            background_hidden: false,
            video_output: 1080,
            whats_new_open: false,
            vertical_turn: true,
            iridescent_palette: IRIDESCENT_PALETTE.to_vec(),
        }
    }
}

pub const THEME_OPTIONS: &[(&str, &str)] = &[
    ("base", "Base (no override)"),
    ("auto", "Auto (time of day)"),
    ("yoake", "Dawn (yoake)"),
    ("day", "Day"),
    ("higure", "Dusk (higure)"),
    ("night", "Night"),
    ("music", "Music"),
    ("black", "Black (hidden)"),
    ("coldboot", "Cold boot (hidden)"),
    ("gameboot", "Game boot"),
    ("welcome", "Welcome"),
];

pub const SEQUENCE_OPTIONS: &[(&str, &str)] =
    &[("none", "None"), ("coldboot", "XMB start"), ("gameboot", "Game launch"), ("otherboot", "Other launch")];

/// A numeric setting's slider: its range, its step and decimals, its tooltip, and, for one kept off the panel, why.
#[derive(Clone, Copy)]
pub struct Meta {
    pub key: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub decimals: usize,
    pub help: &'static str,
    pub hidden: Option<&'static str>,
    pub options: Option<&'static [(f64, &'static str)]>,
    /// Shown only while another setting of the same object holds one of these values.
    pub when: Option<(&'static str, &'static [f64])>,
    /// Values offered as buttons under the slider.
    pub presets: Option<&'static [f64]>,
}

const fn m(key: &'static str, min: f64, max: f64, step: f64, help: &'static str) -> Meta {
    Meta { key, min, max, step, decimals: 4, help, hidden: None, options: None, when: None, presets: None }
}

impl Meta {
    const fn presets(mut self, values: &'static [f64]) -> Self {
        self.presets = Some(values);
        self
    }
    const fn dec(mut self, d: usize) -> Self {
        self.decimals = d;
        self
    }
    const fn hide(mut self, why: &'static str) -> Self {
        self.hidden = Some(why);
        self
    }
    const fn select(mut self, options: &'static [(f64, &'static str)]) -> Self {
        self.options = Some(options);
        self
    }
    const fn when(mut self, key: &'static str, values: &'static [f64]) -> Self {
        self.when = Some((key, values));
        self
    }
}

pub fn meta_of(table: &'static [Meta], key: &str) -> Option<&'static Meta> {
    table.iter().find(|m| m.key == key)
}

/// Where the panel shows each setting: its tab and its section, in this order.
/// The music scene's listening section, whose settings the panel shows in groups by their names' first word.
pub const VISUALIZER: &str = "Music Theme Audio Visualizer";

/// The sections shown in groups by their settings' names' first word, a little space between each.
pub const GROUPED: &[&str] = &[VISUALIZER];

pub struct Section {
    pub tab: &'static str,
    pub section: &'static str,
    pub keys: &'static [&'static str],
}

pub const WAVE_LAYOUT: &[Section] = &[
    Section { tab: "Backdrop", section: "Programs", keys: &["colourShader", "nightBlend", "night2dayBegin", "night2dayEnd", "day2nightBegin", "day2nightEnd", "dayspread", "nightWhitBias"] },
    Section { tab: "Backdrop", section: "Tint, top and bottom", keys: &["colour1Red", "colour1Green", "colour1Blue", "colour2Red", "colour2Green", "colour2Blue"] },
    Section { tab: "Wave", section: "Lines", keys: &["damping", "length", "tension", "timestep", "perturbation", "endX", "endY", "endZ"] },
    Section { tab: "Wave", section: "Placement", keys: &["posX", "posY", "posZ", "angX", "angY", "angZ", "angRot"] },
    Section { tab: "Wave", section: "Deformation", keys: &["ffdScale1X", "ffdScale1Y", "ffdScale1Z", "ffdScale2X", "ffdScale2Y", "ffdScale2Z", "ffdOffsetX", "ffdOffsetY", "ffdOffsetZ", "ffdParam1"] },
    Section { tab: "Wave", section: "Shading", keys: &["brightness", "fresnel", "mipmapBias", "falloff", "spacing", "thinness"] },
    Section { tab: "Wave", section: "Tint, left and right", keys: &["colour3Red", "colour3Green", "colour3Blue", "colour4Red", "colour4Green", "colour4Blue"] },
    Section { tab: "Post", section: "Tone curve", keys: &["exposure", "whiteLevel", "dither"] },
    Section { tab: "Post", section: "Glare", keys: &["glareLevel", "glareThresh", "gaussianRadR", "gaussianRadG", "gaussianRadB", "glareSumPow"] },
];

pub const PARTICLE_LAYOUT: &[Section] = &[
    Section { tab: "Scene", section: "Display", keys: &["backgroundFadeMs"] },
    Section { tab: "Input", section: "Arrow keys", keys: &["dpadRotMax", "dpadScaleX", "dpadScaleY"] },
    Section { tab: "Input", section: "Mouse", keys: &["pointerWind", "mouseAccelToG", "mouseYawGain"] },
    Section { tab: "Input", section: "Motion and shakes", keys: &["uiBrownian", "rshakeBrw", "dshakeRotMax", "dshakeRotImp", "dshakeBrwImp", "dshakeThresh", "dshakeXCoeff", "dshakeGCoeff"] },
    Section { tab: "Input", section: "Icon wind", keys: &["iconWind", "iconWindSclX", "iconWindSclY", "iconEaseSec", "flowStrength"] },
    Section { tab: "Particles", section: "Emission", keys: &["emitVelMin", "emitVelMul", "emitVelVar", "emitConeAngle", "emitNegProb", "emitVelZscale", "emitPerFrame", "emitProb"] },
    Section { tab: "Particles", section: "Life", keys: &["agingSpeed", "agingVariance"] },
    Section { tab: "Particles", section: "Motion", keys: &["friction", "spinTimeScale", "deltaTime", "gravity", "windDirX", "windDirY", "windDirZ", "windScale", "windScale10", "brownianScale"] },
    Section { tab: "Particles", section: "Lighting", keys: &["spotPosX", "spotPosY", "spotPosZ", "spotAttnX", "spotAttnY", "spotAttnZ", "specularPower", "specularCoeff", "lambertCoeff", "exposure", "fresnel", "globalAlpha"] },
    Section { tab: "Particles", section: "Iridescence", keys: &["colorControl", "iridescentExp"] },
    Section { tab: "Particles", section: "Size and focus", keys: &["sizeMiddle", "sizeNear", "sizeFar", "nearFocus", "nearFocusDist", "nearFocusPow", "nearDarkness", "nearFuzziness", "farFocus", "farFocusDist", "farFocusPow", "farDarkness"] },
    Section { tab: "Particles", section: "Alignment and glare", keys: &["nearAlign", "sizeAlign", "glare", "glareScale", "glareP1", "glareP2"] },
];


pub static WAVE_META: &[Meta] = &[
    m("damping", 0.0, 0.002, 0.00001, "How fast the lines lose their speed: each step takes DAMPING x TIMESTEP of each point's velocity off.\nLINE1.mnu: DAMPING").dec(5),
    m("length", 0.05, 1.0, 0.001, "Rest length of the springs between neighbouring points, along a line and across the lines; those two apart rest at twice it.\nLINE1.mnu: LENGTH"),
    m("tension", 0.0, 1.0, 0.005, "Stiffness of the springs between neighbouring points; those two apart are ten times as stiff.\nLINE1.mnu: TENSION"),
    m("timestep", 0.0, 8.0, 0.05, "Speed of the wave's time: how far each of the 60 steps a second moves the points, the clocks and the lattice.\nLINE1.mnu: TIMESTEP"),
    m("perturbation", 0.0, 0.5, 0.001, "Strength of the noise added to every point's velocity each step.\nLINE1.mnu: PERTURBATION"),
    m("endX", 0.0, 6.0, 0.05, "No effect: the step sets the anchored ends at x = 0 and reads only END Y and END Z.\nLINE1.mnu: END X").hide("No effect: the step reads only END Y and END Z"),
    m("endY", 0.0, 2.0, 0.01, "Height of the anchored ends' swing, line after line along the wave.\nLINE1.mnu: END Y"),
    m("endZ", 0.0, 2.0, 0.01, "Depth of the anchored ends' swing.\nLINE1.mnu: END Z"),
    m("posX", -15.0, 5.0, 0.01, "Where the wave sits in the XMB's world, across the screen.\nLINE1.mnu: POS X"),
    m("posY", -5.0, 5.0, 0.01, "Where the wave sits in the XMB's world, up the screen.\nLINE1.mnu: POS Y"),
    m("posZ", -15.0, 0.0, 0.01, "Where the wave sits in the XMB's world, in depth; the camera is at z = 2.\nLINE1.mnu: POS Z"),
    m("angX", -1.0, 1.0, 0.001, "X of the axis the wave is turned about (normalised).\nLINE1.mnu: ANG X"),
    m("angY", -1.0, 1.0, 0.001, "Y of the axis the wave is turned about (normalised).\nLINE1.mnu: ANG Y"),
    m("angZ", -1.0, 1.0, 0.001, "Z of the axis the wave is turned about (normalised).\nLINE1.mnu: ANG Z"),
    m("angRot", -90.0, 90.0, 0.1, "How far the wave is turned about that axis, in degrees.\nLINE1.mnu: ANG ROT"),
    m("ffdScale1X", 0.5, 10.0, 0.01, "Size of the deformation's lattice along x: the points are normalised by it before they are deformed, and the lattice's own points are scaled by it.\nLINE1.mnu: FFD SCALE1 X"),
    m("ffdScale1Y", 0.1, 4.0, 0.01, "Size of the deformation's lattice along y, and the scale of its moving curve.\nLINE1.mnu: FFD SCALE1 Y"),
    m("ffdScale1Z", 0.1, 4.0, 0.01, "Size of the deformation's lattice along z.\nLINE1.mnu: FFD SCALE1 Z"),
    m("ffdScale2X", 0.0, 6.0, 0.01, "Scale of the deformed wave along x.\nLINE1.mnu: FFD SCALE2 X"),
    m("ffdScale2Y", 0.0, 4.0, 0.01, "Scale of the deformed wave along y: the height of its swell.\nLINE1.mnu: FFD SCALE2 Y"),
    m("ffdScale2Z", 0.0, 6.0, 0.01, "Scale of the deformed wave along z: its depth.\nLINE1.mnu: FFD SCALE2 Z"),
    m("ffdOffsetX", -3.0, 3.0, 0.01, "Where the deformation's lattice starts along x, and the offset of its points.\nLINE1.mnu: FFD OFFSET X"),
    m("ffdOffsetY", -3.0, 3.0, 0.01, "Where the deformation's lattice starts along y, and the offset of its points.\nLINE1.mnu: FFD OFFSET Y"),
    m("ffdOffsetZ", -3.0, 3.0, 0.01, "Where the deformation's lattice starts along z, and the offset of its points.\nLINE1.mnu: FFD OFFSET Z"),
    m("ffdParam1", -5.0, 5.0, 0.01, "Where along the lines their spread settles: the grid sent to the SPU has its height and depth scaled by 1.3 - cos((x - P) pi/2) (1 - smoothstep((x - P) / 5)), widest near the anchors, 1.3 from five units past P on, and 0.3 before P.\nLINE1.mnu: FFD PARAM 1"),
    m("brightness", 0.0, 3.0, 0.01, "Weight of the wave's even light, times the size of each cell on screen.\nLINE1.mnu: BRIGHTNESS (lines1.vpo _Brightness)"),
    m("mipmapBias", 0.0, 5.0, 0.01, "Scale of each cell's size on screen, which the light, the fresnel table and the stripes' blur all follow.\nLINE1.mnu: MIPMAP BIAS (lines1.vpo _MipmapBias)"),
    m("fresnel", 0.0, 4.0, 0.01, "Weight of the wave's rim light, from the fresnel table at the angle it is seen at.\nLINE1.mnu: FRESNEL (lines1.vpo _Fresnel)"),
    m("falloff", 0.0, 2.0, 0.01, "No effect: the renderer hands it to the vertex program's _Falloff, which only lines2.vpo declares, and every set draws the wave with lines1 (SHADER 1).\nLINE1.mnu: FALLOFF").hide("No effect: only lines2.vpo declares _Falloff, and every set draws the wave with lines1"),
    m("spacing", 10.0, 800.0, 1.0, "How many stripes run along the wave, across its lines. They show only with Thinness below 1.\nLINE1.mnu: SPACING (lines1.fpo _Spacing)").dec(0).hide("No visible effect in any set: THINNESS is 1 in every one, which turns the stripes off"),
    m("thinness", 0.0, 1.0, 0.01, "How much of each stripe's period is dark. At 1, as in every set, there are no stripes and the wave is lit evenly.\nLINE1.mnu: THINNESS (lines1.fpo _Thinness)").hide("The same in every set, 1, which turns the stripes off, and Spacing with them"),
    m("exposure", 0.0, 4.0, 0.01, "What the light is multiplied by before the tone curve, the backdrop's and the wave's alike: 1/16 is taken off it first, so the faintest light stays black.\nHDR.mnu: EXPOSURE (the preexpose tables, GlareSourcePre _Exposure)"),
    m("whiteLevel", 0.5, 1.5, 0.001, "The tone curve's white, W, in x (1 + x / W^2) / (1 + x): at 1 the curve is a straight line, below 1 it brightens what is already bright.\nHDR.mnu: WHITE LEVEL (the preexpose tables, GlareSourcePre _WhiteSqrRcp = 1 / W^2)"),
    m("glareLevel", 0.0, 3.0, 0.01, "Strength of the glare: its six levels' weights add up to its square.\nHDR.mnu: GLARE LEVEL (AccGlare _Weight)"),
    m("glareThresh", 0.0, 1.5, 0.001, "How bright a pixel's brightest channel must be, after the tone curve, for it to glow.\nHDR.mnu: GLARE THRESH (the preexpose tables' alpha)"),
    m("gaussianRadR", 0.0, 3.0, 0.01, "Spread of the glare's blur in red, in texels of each of its levels: the Gaussian's sigma, cut at 3 sigma - 1 texels.\nHDR.mnu: GAUSSIAN RAD R (Gaussian _Weights)"),
    m("gaussianRadG", 0.0, 3.0, 0.01, "Spread of the glare's blur in green.\nHDR.mnu: GAUSSIAN RAD G (Gaussian _Weights)"),
    m("gaussianRadB", 0.0, 3.0, 0.01, "Spread of the glare's blur in blue.\nHDR.mnu: GAUSSIAN RAD B (Gaussian _Weights)"),
    m("glareSumPow", 0.0, 1.0, 0.001, "What each coarser level of the glare weighs against the one before it: the higher, the wider its halo.\nHDR.mnu: GLARE SUM POW (AccGlare _Weight)"),
    m("dither", 0.0, 0.04, 0.0001, "Strength of the noise that dithers the screen, drawn afresh every frame: each texel is 255 x min(1, 8 x Dither x a random number from 0 to 1), so at 1/255, in every set, it runs from 0 to 7, which the screen gets as 0 to 7 eighths of a step.\nHDR.mnu: DITHER (preexpose_Noise)").dec(5),
    m("colour1Red", 0.0, 1.5, 0.001, "Red of the backdrop's colour at the top of the screen.\nBACKGROUND.mnu: 1 RED (LinesController.vpo _Colour2)"),
    m("colour1Green", 0.0, 1.5, 0.001, "Green of the backdrop's colour at the top of the screen.\nBACKGROUND.mnu: 1 GREEN (LinesController.vpo _Colour2)"),
    m("colour1Blue", 0.0, 1.5, 0.001, "Blue of the backdrop's colour at the top of the screen.\nBACKGROUND.mnu: 1 BLUE (LinesController.vpo _Colour2)"),
    m("colour2Red", 0.0, 1.5, 0.001, "Red of the backdrop's colour at the bottom of the screen.\nBACKGROUND.mnu: 2 RED (LinesController.vpo _Colour1)"),
    m("colour2Green", 0.0, 1.5, 0.001, "Green of the backdrop's colour at the bottom of the screen.\nBACKGROUND.mnu: 2 GREEN (LinesController.vpo _Colour1)"),
    m("colour2Blue", 0.0, 1.5, 0.001, "Blue of the backdrop's colour at the bottom of the screen.\nBACKGROUND.mnu: 2 BLUE (LinesController.vpo _Colour1)"),
    m("colour3Red", 0.0, 1.5, 0.001, "Red of the wave's colour at the left edge of the screen.\nBACKGROUND.mnu: 3 RED (LinesController.vpo _Colour3)"),
    m("colour3Green", 0.0, 1.5, 0.001, "Green of the wave's colour at the left edge of the screen.\nBACKGROUND.mnu: 3 GREEN (LinesController.vpo _Colour3)"),
    m("colour3Blue", 0.0, 1.5, 0.001, "Blue of the wave's colour at the left edge of the screen.\nBACKGROUND.mnu: 3 BLUE (LinesController.vpo _Colour3)"),
    m("colour4Red", 0.0, 1.5, 0.001, "Red of the wave's colour at the right edge of the screen.\nBACKGROUND.mnu: 4 RED (LinesController.vpo _Colour4)"),
    m("colour4Green", 0.0, 1.5, 0.001, "Green of the wave's colour at the right edge of the screen.\nBACKGROUND.mnu: 4 GREEN (LinesController.vpo _Colour4)"),
    m("colour4Blue", 0.0, 1.5, 0.001, "Blue of the wave's colour at the right edge of the screen.\nBACKGROUND.mnu: 4 BLUE (LinesController.vpo _Colour4)"),
    m("nightBlend", 0.0, 1.0, 0.001, "How far the backdrop goes over to the night's textures at night: _NightDayBlend is 1 less this times what is left of the day's two ramps.\nBACKGROUND.mnu: NIGHT BLEND"),
    m("night2dayBegin", 0.0, 24.0, 0.01, "The hour the backdrop's morning ramp starts, a smoothstep from the night's textures to the day's.\nBACKGROUND.mnu: NIGHT2DAY BEGIN"),
    m("night2dayEnd", 0.0, 24.0, 0.01, "The hour the backdrop's morning ramp ends.\nBACKGROUND.mnu: NIGHT2DAY END"),
    m("day2nightBegin", 0.0, 24.0, 0.01, "The hour the backdrop's evening ramp starts, a smoothstep back to the night's textures.\nBACKGROUND.mnu: DAY2NIGHT BEGIN"),
    m("day2nightEnd", 0.0, 24.0, 0.01, "The hour the backdrop's evening ramp ends.\nBACKGROUND.mnu: DAY2NIGHT END"),
    m("dayspread", 0.0, 3.0, 0.01, "How fast the backdrop's day clock runs at midnight, against its slowest at noon: _DayTime is a cubic of the time of day with this slope at both ends. The scene holds it to 1 to 3, so its default, 0, reads as 1.\nBACKGROUND.mnu: DAYSPREAD"),
    m("nightWhitBias", 0.0, 1.0, 0.001, "The least light the night's textures get, wherever the night's glows are.\nBACKGROUND.mnu: NIGHT WHIT BIAS (back_colours0.fpo _NightBrightness)"),
    m("colourShader", 0.0, 1.0, 1.0, "Which program draws the backdrop: 0 the months' (back_colours0), 1 the one without them (back_colours1), which the music and the welcome screens use. A change eases in over 2 seconds.\nBACKGROUND.mnu: COLOUR SHADER").select(&[(0.0, "0 - Months'"), (1.0, "1 - No textures")]),
];

pub static PARTICLE_META: &[Meta] = &[
    m("emitVelMin", 0.0, 2.0, 0.001, "The slowest a particle is born: its speed at birth never drops below this, however slowly the wave moves where it is born.\nPARTICLES.mnu: emit vel min"),
    m("emitVelMul", 0.0, 5.0, 0.01, "How much of the wave's speed a particle is born with: its speed at birth is sqrt(this x the wave's speed there), varied by Emit Vel Var and never below Emit Vel Min.\nPARTICLES.mnu: emit vel mul"),
    m("emitVelVar", 0.0, 2.0, 0.001, "Random spread of the speed at birth: what goes under the square root is scaled by 1 plus up to this much either way.\nPARTICLES.mnu: emit vel var"),
    m("emitConeAngle", 0.0, 180.0, 0.1, "Spread of the direction at birth, in degrees: particles leave within this angle of the way the wave moves where they are born.\nPARTICLES.mnu: emit cone angle"),
    m("emitNegProb", 0.0, 1.0, 0.001, "Chance that a particle is born heading against the wave's motion rather than along it.\nPARTICLES.mnu: emit neg prob"),
    m("emitVelZscale", 0.0, 1.0, 0.01, "How much of the velocity at birth along the camera's axis is kept: at 0, the default, particles are born moving within the screen's plane.\nPARTICLES.mnu: emit vel zscale"),
    m("emitPerFrame", 0.0, 100.0, 0.1, "How many particles a burst gives birth to. The console truncates it to a whole number: 16 by default.\nPARTICLES.mnu: emit per frame"),
    m("emitProb", 0.0, 1.0, 0.001, "Chance each frame of a burst of Emit Per Frame births, about 7.7 a frame on average by default. Occasional sweeps along a line of the wave add more.\nPARTICLES.mnu: emit prob"),
    m("agingSpeed", 0.0002, 0.02, 0.00001, "How fast particles age: life goes from 0 to 1 by this much a frame, so a particle lasts about 1 / this frames, some 6 s by default.\nPARTICLES.mnu: aging speed").dec(5),
    m("agingVariance", 0.0, 1.0, 0.001, "How far each particle's aging rate strays from Aging Speed, as a fraction either way: 235 to 692 frames of life at the defaults.\nPARTICLES.mnu: aging variance"),
    m("friction", 0.0, 1.0, 0.0005, "Drag on the particles' velocity: the higher, the sooner they slow down.\nPARTICLES.mnu: friction"),
    m("spinTimeScale", 0.0, 20.0, 0.01, "How fast the flakes tumble. The axis they turn about shifts as they age.\nPARTICLES.mnu: spin time scale"),
    m("deltaTime", 0.001, 0.1, 0.0001, "The time step of the particles' motion: the larger, the faster they move. How fast they age does not depend on it.\nPARTICLES.mnu: delta time"),
    m("gravity", -0.01, 0.01, 0.00001, "A constant vertical force on every particle; negative pulls down.\nPARTICLES.mnu: gravity").dec(6),
    m("windDirX", -1.0, 1.0, 0.001, "X of the wind's direction, normalised with Y and Z. The wind only blows while Wind Scale or Wind Scale 10 is above 0, as in the Game boot and Welcome sets.\nPARTICLES.mnu: wind dir x"),
    m("windDirY", -1.0, 1.0, 0.001, "Y of the wind's direction, normalised with X and Z.\nPARTICLES.mnu: wind dir y"),
    m("windDirZ", -1.0, 1.0, 0.001, "Z of the wind's direction, normalised with X and Y.\nPARTICLES.mnu: wind dir z"),
    m("windScale", 0.0, 0.01, 0.00001, "Strength of the wind, plus ten times Wind Scale 10: a steady push along Wind Dir on every particle.\nPARTICLES.mnu: wind scale").dec(6),
    m("windScale10", 0.0, 0.01, 0.00001, "Adds ten times itself to Wind Scale.\nPARTICLES.mnu: wind scale 10").dec(6),
    m("brownianScale", 0.0, 2.0, 0.001, "Strength of each particle's random drift. Its generators restart every frame, so a particle keeps much the same drift until particles ahead of it in the pool die or are born. D-pad steps and shakes raise it.\nPARTICLES.mnu: brownian scale"),
    m("spotPosX", -20.0, 20.0, 0.01, "X of the spot light the flakes are lit by, in world units: the camera sits at (0, 0, 2) looking down -z.\nPARTICLES.mnu: spot pos x"),
    m("spotPosY", -20.0, 20.0, 0.01, "Y of the spot light the flakes are lit by.\nPARTICLES.mnu: spot pos y"),
    m("spotPosZ", -30.0, 10.0, 0.01, "Z of the spot light the flakes are lit by.\nPARTICLES.mnu: spot pos z"),
    m("spotAttnX", 0.0, 4.0, 0.01, "Constant term of the light's falloff: its light is divided by X + Y x distance + Z x distance^2.\nPARTICLES.mnu: spot attn x"),
    m("spotAttnY", 0.0, 2.0, 0.001, "Linear term of the light's falloff (see Spot Attn X).\nPARTICLES.mnu: spot attn y"),
    m("spotAttnZ", 0.0, 1.0, 0.001, "Quadratic term of the light's falloff (see Spot Attn X).\nPARTICLES.mnu: spot attn z"),
    m("specularPower", 1.0, 200.0, 0.1, "Shininess of the flakes: the higher, the narrower the angle at which a flake glints. It also sizes the glare.\nPARTICLES.mnu: specular power"),
    m("specularCoeff", 0.0, 300.0, 0.1, "Strength of the flakes' mirror-like glint, the part that carries the iridescent colour.\nPARTICLES.mnu: specular coeff"),
    m("lambertCoeff", 0.0, 50.0, 0.1, "Strength of the flakes' diffuse lighting, which is always white.\nPARTICLES.mnu: lambert coeff"),
    m("exposure", 0.0, 0.5, 0.0005, "Exposure of the flakes' light: brightness is 1 - e^(-exposure x light), so higher is brighter until it saturates.\nPARTICLES.mnu: exposure"),
    m("fresnel", 0.1, 8.0, 0.01, "How long a flake stays opaque as it turns edge-on to the camera: the higher, the later it fades. Edge-on it always vanishes.\nPARTICLES.mnu: fresnel"),
    m("colorControl", 0.0, 1.0, 0.01, "Colour of the glint: 0 white, 1 the iridescent colours, and a mix in between.\nPARTICLES.mnu: color_control"),
    m("iridescentExp", 0.1, 8.0, 0.01, "Exponent on the iridescent colour: above 1 deepens it, below 1 washes it out.\nPARTICLES.mnu: iridescent exp"),
    m("globalAlpha", 0.0, 2.0, 0.01, "Overall opacity of the particles; 0 hides them.\nPARTICLES.mnu: global alpha"),
    m("sizeMiddle", 0.0, 0.5, 0.0005, "Size of a flake in focus, in world units.\nPARTICLES.mnu: size middle"),
    m("sizeNear", 0.0, 0.5, 0.0005, "Size of a flake fully near-blurred, close to the camera, reached from Size Middle as the blur comes in.\nPARTICLES.mnu: size near"),
    m("sizeFar", 0.0, 3.0, 0.001, "Size of a flake fully far-blurred, in the distance: at the defaults, the big, soft discs.\nPARTICLES.mnu: size far"),
    m("nearFocus", 0.0, 20.0, 0.01, "Distance from the camera within which flakes are fully near-blurred; the blur clears by Near Focus + Near Focus Dist.\nPARTICLES.mnu: near focus"),
    m("nearFocusDist", 0.01, 10.0, 0.01, "How far beyond Near Focus the near blur takes to clear.\nPARTICLES.mnu: near focus_dist"),
    m("nearFocusPow", 0.1, 8.0, 0.01, "Exponent shaping the near blur: the higher, the sooner it clears.\nPARTICLES.mnu: near focus_pow"),
    m("nearDarkness", 0.0, 40.0, 0.01, "How much the near blur dims flakes that look big on screen.\nPARTICLES.mnu: near darkness"),
    m("nearFuzziness", 0.1, 10.0, 0.01, "How a near-blurred flake's blob is filled: the higher, the brighter and more even out to its rim; the lower, the dimmer, fading from its centre.\nPARTICLES.mnu: near fuzziness"),
    m("farFocus", 0.0, 30.0, 0.01, "Distance from the camera where the far blur begins; it is full by Far Focus + Far Focus Dist. Flakes a little closer fade out briefly, hiding their turn to face the camera.\nPARTICLES.mnu: far focus"),
    m("farFocusDist", 0.01, 20.0, 0.01, "How far beyond Far Focus the far blur takes to become full.\nPARTICLES.mnu: far focus_dist"),
    m("farFocusPow", 0.1, 8.0, 0.01, "Exponent shaping the far blur: the higher, the later it comes in.\nPARTICLES.mnu: far focus_pow"),
    m("farDarkness", 0.0, 40.0, 0.01, "How much the far blur dims flakes that look big on screen.\nPARTICLES.mnu: far darkness"),
    m("nearAlign", 0.0, 2.0, 0.001, "How much near-blurred flakes turn to face the camera instead of tumbling.\nPARTICLES.mnu: near align"),
    m("sizeAlign", 0.0, 40.0, 0.01, "How much flakes that look big on screen turn to face the camera.\nPARTICLES.mnu: size align"),
    m("glare", 0.0, 2.0, 0.001, "Strength of the glare, the glint drawn over a flake while it mirrors the light into the camera.\nPARTICLES.mnu: glare"),
    m("glareScale", 0.0, 20.0, 0.01, "Size of a glare relative to its flake.\nPARTICLES.mnu: glare scale"),
    m("glareP1", 0.1, 4.0, 0.01, "Shape of the glare's falloff from its centre: the exponent on the distance in e^(-P2 x distance^P1).\nPARTICLES.mnu: glare p1"),
    m("glareP2", 0.0, 20.0, 0.01, "How fast the glare falls off from its centre: the higher, the smaller and sharper the glint.\nPARTICLES.mnu: glare p2"),
    m("uiBrownian", 0.0, 4.0, 0.01, "How much D-pad steps and shakes raise the drift: a level from 0 to 1, which each sideways step or shake kicks, times this is added to Brownian Scale.\nPARTICLES_UI.mnu: brownian"),
    m("rshakeBrw", 0.0, 20.0, 0.01, "How much moving the controller raises the drift: the change in its accelerometer's reading from one frame to the next, times this, is added to Brownian Scale.\nPARTICLES_UI.mnu: rshake brw"),
    m("dpadRotMax", 0.0, 0.002, 0.000001, "How fast D-pad steps turn the whole field of particles. A single tap is too small to turn it; a held direction does.\nPARTICLES_UI.mnu: dpad rot max").dec(6),
    m("dpadScaleX", 0.0, 2.0, 0.01, "Weight of left and right steps in the field's turn and in the drift they kick.\nPARTICLES_UI.mnu: dpad scale x"),
    m("dpadScaleY", 0.0, 2.0, 0.01, "Weight of up and down steps in the field's turn and in the drift they kick. At 0, the default, they do neither, and stop any turn in progress.\nPARTICLES_UI.mnu: dpad scale y"),
    m("dshakeRotMax", 0.0, 0.002, 0.000001, "How fast a shake turns the field of particles.\nPARTICLES_UI.mnu: dshake rot max").dec(6),
    m("dshakeRotImp", 0.0, 2.0, 0.001, "How strongly a detected shake kicks the field's turn.\nPARTICLES_UI.mnu: dshake rot imp"),
    m("dshakeBrwImp", 0.0, 1.0, 0.001, "How much a detected shake raises the drift.\nPARTICLES_UI.mnu: dshake brw imp"),
    m("dshakeThresh", 0.0, 4.0, 0.001, "How hard the controller must be shaken for a shake to be detected.\nPARTICLES_UI.mnu: dshake thresh"),
    m("dshakeXCoeff", 0.0, 2.0, 0.001, "Weight of the turn from shakes the accelerometer's sideways axis detects.\nPARTICLES_UI.mnu: dshake x coeff"),
    m("dshakeGCoeff", 0.0, 2.0, 0.001, "Weight of the turn from shakes the gyro detects.\nPARTICLES_UI.mnu: dshake g coeff"),
    m("iconWind", 0.0, 50.0, 0.01, "Strength of the wind the icons raise as they move: it pushes the particles near them the way they go, and dies away within about 1.4 s.\nPARTICLES_UI.mnu: icon wind"),
    m("iconWindSclX", 0.0, 2.0, 0.01, "Horizontal part of the icon wind: 0 by default, so icons moving sideways raise none.\nPARTICLES_UI.mnu: icon wind scl x"),
    m("iconWindSclY", 0.0, 2.0, 0.01, "Vertical part of the icon wind: icons moving up or down push the particles the same way.\nPARTICLES_UI.mnu: icon wind scl y"),
    m("backgroundFadeMs", 0.0, 20000.0, 10.0, "How long the scene takes to fade out, or back in, when the XMB's background is hidden or shown.\nPage setting: the console's callers use 0 to 1000 ms; the native app's default is 10 s, its own choice").dec(0),
    m("flowStrength", 0.0, 2.0, 0.0005, "Strength of the flow grid, which carries the icon wind to the particles.\nFirmware constant: the PPU always writes 1").hide("A firmware constant, not a parameter: the PPU always writes 1"),
    m("iconEaseSec", 0.005, 1.0, 0.005, "Time constant of the modelled icons' easing after a step: the shorter, the faster they move, and faster icons raise a stronger wind.\nModelled: how the XMB moves its icons is not traced").hide("Modelled and closed: how the XMB moves its icons is not pursued, and the page draws none"),
    m("mouseAccelToG", 0.0, 0.2, 0.001, "How strongly dragging the mouse shakes the virtual controller: an acceleration of one screen height per second^2 reads as this many g on its accelerometer.\nPage setting: the mouse stands in for the Sixaxis"),
    m("mouseYawGain", 0.0, 1.0, 0.005, "How strongly dragging sideways turns the virtual controller: a speed of one screen height a second reads this much on its gyro, whose reading runs from -1 to 1.\nPage setting: the mouse stands in for the Sixaxis"),
    m("pointerWind", 0.0, 1.0, 0.05, "How hard the mouse pointer pushes the particles it passes over: it writes its motion into the flow grid as a moving icon does, times this, and in both directions where the icons push only up and down. 0 turns it off.\nPage setting: the console has no pointer"),
];


/// The native app's own settings on the panel, grouped as Enhancements at the top of their tab, ahead of the console's.
/// A setting whose meta has a `when` shows only while the setting it names holds one of its values.
/// Settings renamed since configs were saved with them: the old name, and the new.
static RENAMED: &[(&str, &str)] = &[
    ("aeMaxBoost", "aeMaxPush"),
    ("aeMetric", "aeMetering"),
    ("aeTargetBrightestChannel", "aeHighlightTarget"),
    ("aeTargetLuminance", "aeLuminanceTarget"),
    ("aeCell", "aeZoneSize"),
    ("aeQuantile", "aePercentile"),
    ("aeRiseSec", "aePushTime"),
    ("aeFallSec", "aePullTime"),
    ("blurWindowBackdrop", "windowBackdrop"),
    ("blurRadius", "sceneBlurRadius"),
    ("blurTint", "backdropTint"),
    ("blurSaturation", "sceneBlurSaturation"),
    ("blurNoise", "sceneBlurGrain"),
    ("musicClockColour", "musicColourChoice"),
    ("musicClockColourMix", "musicColourMix"),
];

/// A setting's name now: `key`, or what it was renamed to.
pub fn current_name(key: &str) -> &str {
    RENAMED.iter().find(|(old, _)| *old == key).map_or(key, |(_, new)| new)
}

/// The Resources tab's section of what the app does while other programs need the PC.
pub const READING_THE_ROOM: &str = "Reading the room";

pub const NATIVE_LAYOUT: &[Section] = &[
    Section { tab: "Scene", section: "Automatic Music Theme", keys: &["autoMusic", "autoMusicHoldSec", "musicColourChoice", "musicColourMix"] },
    // One section, its settings in groups by their names' first word (audio, tempo, bass, sub, kick, motion): see
    // `GROUPED`.
    Section { tab: "Scene", section: VISUALIZER, keys: &["audioReactive", "audioFlicker", "audioFlickerRate", "audioFollow", "audioQuiet", "audioLoud", "audioWindow", "audioRise", "audioFall", "audioBassBoost", "tempoBounce", "tempoBounceAmount", "tempoActivity", "tempoConfidence", "tempoOnsets", "tempoBassWeight", "tempoGate", "tempoGateDrop", "bassFlicker", "bassFlickerDepth", "bassRate", "bassFold", "bassLow", "bassHigh", "bassQuiet", "bassLoud", "subShake", "subShakeAmount", "kickBounce", "kickBounceAmount", "kickThreshold", "bassWarmDelay", "bassWarmUp", "bassWarmHold", "motionBlur", "motionBlurHz"] },
    Section { tab: "Scene", section: "Epilepsy warning", keys: &["epilepsyWarning", "epilepsyWarningEvery", "epilepsyWarningHoldSec", "epilepsyWarningFadeSec", "epilepsyWarningAfterSec", "epilepsyWarningBlurPx", "epilepsyWarningFont", "epilepsyWarningSize", "epilepsyWarningWidth", "epilepsyWarningX", "epilepsyWarningY", "epilepsyWarningTextBlur", "epilepsyWarningGloss", "epilepsyWarningShadow", "epilepsyWarningShadowOffsetX", "epilepsyWarningShadowOffsetY", "epilepsyWarningShadowRadius", "epilepsyWarningShadowRed", "epilepsyWarningShadowGreen", "epilepsyWarningShadowBlue"] },
    Section { tab: "Scene", section: "First start", keys: &["intro", "benchmarkLineInset"] },
    Section { tab: "Backdrop", section: "Season Pass", keys: &["seasonPass", "seasonPassHoldMin", "seasonPassFadeSec", "seasonPassMonths"] },
    Section { tab: "Post", section: "Auto exposure (AE)", keys: &["aeEnabled", "aeMaxPush", "aeMaxPull", "aePullAbove", "aeSource", "aeMetering", "aeHighlightTarget", "aeLuminanceTarget", "aeZoneSize", "aePercentile", "aePushTime", "aePullTime", "aeStartDelaySec", "aeBuildUpSec"] },
    Section { tab: "Post", section: "Tonemapper", keys: &["tonemap", "tmMatchMidGrey", "tmGain", "tmSaturation", "tmAgxLook", "tmGtMaxBrightness", "tmGtContrast", "tmGtLinearStart", "tmGtLinearLength", "tmGtBlackTightness", "tmGtPedestal", "tmHableWhite", "tmReinhardWhite"] },
    Section { tab: "Post", section: "Glare mode", keys: &["glareMode", "bloomStrength", "bloomFalloff", "bloomOctaves", "bloomTint"] },
    Section { tab: "Post", section: "Glare radius (Console mode)", keys: &["glareRadiusScaleOn", "glareRadiusScale"] },
    Section { tab: "Post", section: "Sensor noise and AlexaGrain", keys: &["noiseEnabled", "noiseMode", "grainAmount", "grainSize", "grainIso", "noiseRead", "noiseShot", "noiseChroma", "noiseSize"] },
    Section { tab: "Particles", section: "Particle limit", keys: &["particleLimit"] },
    Section { tab: "Maru-time", section: "Maru-chan", keys: &["maruChan", "maruFirstMin", "maruEveryMin", "maruMusicOnly", "maruCrossSec", "maruSizePx", "maruSteam", "maruSmoke", "maruBuoyancy", "maruTilt", "maruTiltBounce", "maruBassBounce", "maruSaturation", "maruFlat", "maruSupersample", "maruMotionBlur", "maruSceneLight", "maruSceneTint"] },
    Section { tab: "Maru-time", section: "Disco ball", keys: &["discoBall", "discoAfterMin", "discoBassLevel", "discoDescendSec", "discoSizePx", "discoX", "discoFlat", "discoSpin", "discoBeams", "discoSpots", "discoGlare", "discoFloor", "discoBeatLights"] },
    Section { tab: "Post", section: "Anti-aliasing", keys: &["antiAlias", "supersample"] },
    Section { tab: "Post", section: "Sharpening", keys: &["sharpen", "sharpenAmount", "sharpenStrength"] },
    Section { tab: "Post", section: "Highlight desaturation (deprecated)", keys: &["desatEnabled", "desatStart", "desatRange", "desatWhiten"] },
    Section { tab: "Panel", section: "Background blur", keys: &["blurEnabled", "windowBackdrop", "backdropTint", "sceneBlurRadius", "sceneBlurSaturation", "sceneBlurGrain", "windowTheme", "acrylicOpacity", "windowCorners"] },
    Section { tab: "Panel", section: "Opening the settings", keys: &["pokesToOpen", "pokeBounce", "logoOnRightClick"] },
    Section { tab: "Panel", section: "Size", keys: &["uiScale"] },
    Section { tab: "Resources", section: "Graphics", keys: &["renderer", "gpuWave", "renderScale", "softwareLite", "tripleBuffering"] },
    Section { tab: "Resources", section: READING_THE_ROOM, keys: &["whenOtherFocused", "whenOtherMaximized", "whenOtherFullscreen", "whenOnBattery", "whenGpuMemoryLow", "gpuMemoryThreshold", "limitFps", "stopTransition", "stopTransitionUrgent", "stopHideWindow", "resumeTransition"] },
    Section { tab: "Resources", section: "CPU", keys: &["cpuPlacement", "cpuEcoQoS", "cpuPriority"] },
];

/// The tabs that hold only the native app's own settings, with no Sony-original knobs to set them apart from.
pub const APP_TABS: &[&str] = &["Panel", "Resources", "Maru-time"];

const ACTIONS: &[(f64, &str)] = &[(0.0, "Keep running"), (1.0, "Limit frame rate"), (2.0, "Pause"), (3.0, "Stop")];
const SOME_ACTION: &[f64] = &[1.0, 2.0, 3.0];

const ON: &[f64] = &[1.0];
const CONSOLE: &[f64] = &[0.0];
const TONEMAPPED: &[f64] = &[1.0, 2.0, 3.0, 4.0, 5.0];

const OFF_ON: &[(f64, &str)] = &[(0.0, "Off"), (1.0, "On")];

pub static NATIVE_META: &[Meta] = &[
    m("aeEnabled", 0.0, 1.0, 1.0, "Auto exposure: pushes the exposure while the screen's highlights stay below their target, by at most aeMaxPush, and pulls it back - by at most aeMaxPull - while they clip. The exposure setting is left as the scene writes it; the composite draws with the sum. Also the panel's AE button.").select(OFF_ON),
    m("aeMaxPush", 0.0, 2.0, 0.01, "The most auto exposure adds to the exposure, pushing a dark scene up.").dec(2).when("aeEnabled", ON),
    m("aeMaxPull", 0.0, 1.0, 0.01, "The most auto exposure takes from the exposure, pulling a scene that clips back down.").dec(2).when("aeEnabled", ON),
    m("aePullAbove", 0.8, 1.0, 0.005, "How bright the highlights must be before auto exposure pulls: the metered peak, from 0 to 1, well above the target, so that a scene that merely reaches its target is left alone; 1 pulls only at the clip.").dec(3).when("aeEnabled", ON),
    m("aeSource", 0.0, 1.0, 1.0, "What auto exposure meters. The final frame: what goes to the screen, the glare (or the bloom) and the tone curve with it, before the particles. Before the glare: the composite, as AE did before - the glare it does not see can carry the highlights past the target.").select(&[(0.0, "Before the glare"), (1.0, "The final frame")]).when("aeEnabled", ON),
    m("aeMetering", 0.0, 1.0, 1.0, "What a pixel's brightness is metered as. Brightest channel: its largest of red, green and blue, which keeps any channel from clipping. Luminance: Rec. 709's weighting, closer to how bright it looks, which lets a saturated colour clip. Each has its own target.").select(&[(0.0, "Brightest channel"), (1.0, "Luminance")]).when("aeEnabled", ON),
    m("aeHighlightTarget", 0.3, 1.0, 0.01, "Where auto exposure places the highlights while metering the brightest channel, from 0 to 1, in what aeSource meters.").dec(2).when("aeMetering", CONSOLE),
    m("aeLuminanceTarget", 0.3, 1.0, 0.01, "Where auto exposure places the highlights while metering luminance, from 0 to 1, in what aeSource meters.").dec(2).when("aeMetering", ON),
    m("aeZoneSize", 32.0, 128.0, 32.0, "The metering zones: square cells of this many pixels, each taking its brightest pixel. Smaller zones let a small bright area count for more of the screen.").select(&[(32.0, "32 px"), (64.0, "64 px"), (128.0, "128 px")]).when("aeEnabled", ON),
    m("aePercentile", 0.5, 1.0, 0.005, "Which zone counts as the highlights, from the darkest (0) to the brightest (1): 0.97 lets the brightest 3% of them pass the target, 1 meters the single brightest zone, so that nothing at all passes it.").dec(3).when("aeEnabled", ON),
    m("aePushTime", 0.05, 10.0, 0.05, "How long auto exposure takes to push the exposure up to a darker scene's target: seconds to cover the difference.").dec(2).when("aeEnabled", ON),
    m("aePullTime", 0.05, 10.0, 0.05, "How long auto exposure takes to come down: a push given back, or a pull while the highlights clip - seconds to cover the difference.").dec(2).when("aeEnabled", ON),
    m("aeStartDelaySec", 0.0, 60.0, 0.5, "How long after Nagare starts auto exposure stays off, so that the start - the XMB's own, from black - shows the scene at its true brightness. 0: on at once. Only at the start: AE switched on later comes on at once.").dec(1).when("aeEnabled", ON),
    m("aeBuildUpSec", 0.0, 60.0, 0.5, "Then, over how long auto exposure's speed builds up, from barely moving to aePushTime and aePullTime: with 7 and 14, it comes on 7 s after the start and is at full speed at 21 s. 0: at full speed at once.").dec(1).when("aeEnabled", ON),
    m("glareMode", 0.0, 1.0, 1.0, "How the glare is drawn. Console: the console's passes, out of a 128 x 64 buffer, whose small highlights come out as soft blocky blobs. Exponential bloom: the same source - the screen above GLARE THRESH, times GLARE LEVEL - blurred at every octave from half the screen's size down, and the octaves summed with weights that change geometrically, as a lens's scatter does.").select(&[(0.0, "Console"), (1.0, "Exponential bloom")]),
    m("bloomStrength", 0.0, 4.0, 0.01, "Exponential bloom: a multiplier on its light, over the GLARE LEVEL the scene sets.").dec(2).when("glareMode", ON),
    m("bloomFalloff", 0.3, 1.5, 0.01, "Exponential bloom: each octave's weight against the one finer than it. Below 1 the light stays near the highlight; at 1 every octave weighs the same; above 1 the widest octaves dominate. 1.2 puts the light about as far out as the console's levels.").dec(2).when("glareMode", ON),
    m("bloomOctaves", 8.0, 64.0, 1.0, "Exponential bloom: how many octaves are summed, from half the screen's size down, each half the one before - so how far the light reaches. Past a single pixel the octaves stay at one pixel, each adding the screen's average once more, a haze over the whole screen. With many octaves, bloomTint's per-channel falloffs drift apart (the widest channel's light goes to the haze), which bloomTint 0 or a falloff of 1 or less keeps in hand.").dec(0).when("glareMode", ON),
    m("bloomTint", 0.0, 2.0, 0.01, "Exponential bloom: how much the console's GAUSSIAN RAD R, G and B colour it. Each channel's falloff is scaled by its radius against green's, so a wider blue radius gives the bloom a bluer, wider reach. 0 makes it neutral.").dec(2).when("glareMode", ON),
    m("glareRadiusScaleOn", 0.0, 1.0, 1.0, "Multiplies GAUSSIAN RAD R, G and B, which the scene's sets write, by glareRadiusScale, leaving the settings themselves alone. The console's glare: wider, smoother Gaussians, with more taps than the console's 15 where they need them. Shown while glareMode is Console: the exponential bloom reads only the radii's ratios to each other, which the multiplier leaves as they are; its width is bloomFalloff and bloomOctaves.").select(OFF_ON).when("glareMode", CONSOLE),
    m("glareRadiusScale", 0.0, 10.0, 0.05, "What GAUSSIAN RAD R, G and B are multiplied by, while glareRadiusScaleOn is on.").dec(2).when("glareRadiusScaleOn", ON),
    m("tonemap", 0.0, 5.0, 1.0, "The curve the exposed light goes through to the screen. Console: the console's own, x (1 + x / W^2) / (1 + x) per channel, W being WHITE LEVEL, which lets bright light pass 1 and clip. The others take the console's light as a display's (2.2) and go back to it after, so mid-tones stay where the console has them, and roll the highlights off: ACES (Hill's fit of the RRT and ODT), AgX (Sobotka's, minimal fit, with its looks), Gran Turismo (Uchimura's curve, with its six knobs), Hable (Uncharted 2's filmic curve) and Reinhard (extended, on luminance, so hues hold). The glare's mask still reads the console's curve, so the glare stays the console's.").select(&[(0.0, "Console"), (1.0, "ACES"), (2.0, "AgX"), (3.0, "Gran Turismo"), (4.0, "Hable"), (5.0, "Reinhard")]),
    m("tmGain", 0.1, 4.0, 0.01, "What the light is multiplied by before the curve, on top of the exposure (and of the mid-grey match).").dec(2).when("tonemap", TONEMAPPED),
    m("tmMatchMidGrey", 0.0, 1.0, 1.0, "Brings mid-grey through the curve back to mid-grey, with a gain worked out for the curve and its knobs, so that the mid-tones stay where the console has them and only the shadows and the highlights change. Off shows the curve as it is, most of them darker.").select(OFF_ON).when("tonemap", TONEMAPPED),
    m("tmSaturation", 0.0, 2.0, 0.01, "The curve's colour, after it: 0 grey, 1 as the curve leaves it, 2 twice as saturated, about each pixel's luminance.").dec(2).when("tonemap", TONEMAPPED),
    m("tmAgxLook", 0.0, 2.0, 1.0, "AgX's look: none, Golden (warm, softer) or Punchy (more contrast and colour).").select(&[(0.0, "None"), (1.0, "Golden"), (2.0, "Punchy")]).when("tonemap", &[2.0]),
    m("tmGtMaxBrightness", 0.5, 4.0, 0.01, "Gran Turismo: P, the brightest the curve reaches (1 the screen's white). Above 1 it clips before it gets there.").dec(2).when("tonemap", &[3.0]),
    m("tmGtContrast", 0.1, 3.0, 0.01, "Gran Turismo: a, the slope of its straight middle.").dec(2).when("tonemap", &[3.0]),
    m("tmGtLinearStart", 0.0, 1.0, 0.01, "Gran Turismo: m, where the straight middle starts, the toe below it.").dec(2).when("tonemap", &[3.0]),
    m("tmGtLinearLength", 0.0, 1.0, 0.01, "Gran Turismo: l, how long the straight middle runs before the shoulder rolls it off.").dec(2).when("tonemap", &[3.0]),
    m("tmGtBlackTightness", 1.0, 3.0, 0.01, "Gran Turismo: c, the toe's curvature: higher crushes the blacks.").dec(2).when("tonemap", &[3.0]),
    m("tmGtPedestal", 0.0, 0.2, 0.001, "Gran Turismo: b, lifts the blacks.").dec(3).when("tonemap", &[3.0]),
    m("tmHableWhite", 1.0, 30.0, 0.1, "Hable: the linear white point, the light that reaches the screen's white.").dec(1).when("tonemap", &[4.0]),
    m("tmReinhardWhite", 1.0, 20.0, 0.1, "Reinhard: the luminance that reaches the screen's white.").dec(1).when("tonemap", &[5.0]),
    m("particleLimit", 2049.0, 65536.0, 1.0, "How many particles there can be at once: the size of the pool the emitter takes them from. 2049, the XMB's own pool, as read from a savestate - the console runs it nearly full, so a larger pool fills with more particles at once, the emitter as it is. At rest the emitter keeps only about 2,600 alive whatever the pool; to fill a larger one, raise emitPerFrame or emitProb, or slow agingSpeed, with it (emitPerFrame 40 fills about 6,700). The simulation's cost grows with it, on one core.").dec(0),
    m("maruChan", 0.0, 1.0, 1.0, "Maru-chan: Nagare-maru, the perf overlay's little steamboat, as a 3D boat in the scene, sailing the wave across the screen now and then. It blinks, squints at a kick and sings when the music is loud, trembles with the bass as subShake shakes the camera, and puffs smoke, the more the louder the bass.").select(OFF_ON),
    m("maruFirstMin", 0.0, 60.0, 0.1, "Minutes after it is switched on - or after the app starts with it on - before its first voyage.").dec(1).when("maruChan", ON),
    m("maruEveryMin", 0.5, 120.0, 0.5, "Minutes from the end of one voyage to the start of the next.").dec(1).when("maruChan", ON),
    m("maruMusicOnly", 0.0, 1.0, 1.0, "Sails only while the music theme is on: a voyage that comes due waits for the music.").select(OFF_ON).when("maruChan", ON),
    m("maruCrossSec", 5.0, 120.0, 1.0, "Seconds it takes to cross the screen, from off its left edge to off its right.").dec(0),
    m("maruSizePx", 16.0, 420.0, 1.0, "Its length on the screen, in pixels, wherever it floats.").dec(0),
    m("maruSteam", 0.0, 69.0, 0.05, "How much smoke it puffs: a little always, more the louder the bass.").dec(2),
    m("antiAlias", 0.0, 2.0, 1.0, "Smooths the finished frame's jagged edges - the scene, the particles and maru-chan, after the sharpening, so that it is not sharpened back - with FXAA, fast approximate anti-aliasing (after Timothy Lottes' algorithm, re-authored): one pass that finds where the brightness steps, follows the edge to its ends and blends across it. Fast searches a short way along each edge, Quality further, for long shallow edges. The perf overlay shows it as FXAA.").select(&[(0.0, "Off"), (1.0, "FXAA, fast"), (2.0, "FXAA, quality")]),
    m("supersample", 1.0, 2.0, 0.5, "Draws the scene at 1.5 or 2 times the window's size and filters it down onto the screen: the thorough way to smooth every edge, at 2.25 or 4 times the pixels' cost on the GPU. FXAA can run over it too.\nVulkan and the software renderer only: OpenGL draws straight into the window").select(&[(1.0, "Off"), (1.5, "1.5x"), (2.0, "2x")]),
    m("maruSmoke", 0.0, 1.0, 1.0, "Its smoke. Puffs: soft round puffs that rise, swell and drift back. Swirling: many finer wisps, carried besides by a flow that curls and eddies (curl noise), as smoke does.").select(&[(0.0, "Puffs"), (1.0, "Swirling")]),
    m("maruTilt", 0.0, 3.0, 0.05, "Tilts maru-chan with the wave's surface under it - its bow up as the wave rises ahead, down as it falls - on a spring, so that it overshoots and wobbles back: 1 follows the slope, more exaggerates it, 0 keeps it level but for its gentle rocking.").dec(2),
    m("maruTiltBounce", 0.0, 1.0, 0.01, "How bouncy the tilt's spring is: 0 settles at once, 1 rocks back and forth a while after each change of the slope, a small one making it wobble about.").dec(2),
    m("maruBuoyancy", 0.0, 1.0, 0.01, "How buoyant it is on the wave: 0 glides along its top, eased; more floats it on a spring that follows every rise and fall of the wave, overshoots and bounces, leans as it rises and falls, and hops at a kick.").dec(2),
    m("maruBassBounce", 0.0, 4.0, 0.05, "How far the bass moves it up and down, at subShake's rate, as far as the bass is loud: 0 not at all, 1 as it was, more for a bigger bounce.").dec(2),
    m("maruSupersample", 1.0, 4.0, 1.0, "Draws maru-chan - the boat, not its smoke - into a tile of its own at 2, 3 or 4 times the screen's pixels, and averages each block of them into one, over the scene: its edges, the stripe and the face's strokes as smooth as supersampling makes them, for a few hundred pixels' cost. FXAA, after, sees little left to do.").select(&[(1.0, "Off"), (2.0, "2x"), (3.0, "3x"), (4.0, "4x")]),
    m("maruMotionBlur", 0.0, 1.0, 1.0, "Blurs its motion, as a camera's open shutter does: it is drawn where it was at up to 16 moments across the frame, along the path it took - the bass's quick bounce, its bob, its float - and they are averaged. As many as its move that frame needs; none when it hardly moves.").select(OFF_ON),
    m("maruSceneLight", 0.0, 1.0, 1.0, "Lights maru-chan by the scene around it, so that it sits in it: its glass, its paint and its smoke as bright as the frame is around it and on the whole - dark while the scene is dark, as at the start, brighter as the exposure rises - and tinted the frame's colour. What its glass shows of the scene behind is the scene's already. Off: lit by its own light, the same in any scene.").select(OFF_ON),
    m("maruSceneTint", 0.0, 1.0, 0.01, "How far the scene's colour tints maru-chan while lit by the scene: 0 only its brightness, 1 its colour too.").dec(2).when("maruSceneLight", ON),
    m("maruAt", -1.0, 1.0, 0.001, "Holds maru-chan out the whole time, floating at this share of the screen's width from the left, whatever its schedule; below 0 it sails as scheduled.\nFor renders: nagare-customize's shots set it").dec(3).hide("For renders: nagare-customize's shots set it with --set"),
    m("epilepsyWarning", 0.0, 2.0, 1.0, "The photosensitive epilepsy warning, as the console shows it as it starts. Before the start: on black, before anything else is seen; the XMB's start then plays from its beginning. After the XMB's start: over the scene, blurred behind it, once the particles have come in.").select(&[(0.0, "Off"), (1.0, "Before the start"), (2.0, "After the XMB's start")]),
    m("epilepsyWarningEvery", 0.0, 1.0, 1.0, "When it shows: only at Nagare's first start ever, or at every start. Show it now shows it at once.").select(&[(0.0, "First start only"), (1.0, "Every start")]).when("epilepsyWarning", &[1.0, 2.0]),
    m("epilepsyWarningHoldSec", 0.5, 15.0, 0.1, "Seconds it stays, between fading in and fading out.").dec(1),
    m("epilepsyWarningFadeSec", 0.05, 3.0, 0.01, "Seconds it takes to fade in, and to fade out - the blur behind it too.").dec(2),
    m("epilepsyWarningAfterSec", 0.0, 30.0, 0.1, "After the XMB's start: seconds into it before the warning comes - about half a second after the particles come in, as the console's does.").dec(1).when("epilepsyWarning", &[2.0]),
    m("epilepsyWarningBlurPx", 0.0, 60.0, 0.5, "After the XMB's start: how far the scene behind it is blurred, in pixels.").dec(1).when("epilepsyWarning", &[2.0]),
    m("epilepsyWarningShadow", 0.0, 1.0, 0.01, "How opaque the shadow under its letters is: 0 none.").dec(2),
    m("epilepsyWarningShadowOffsetX", -20.0, 20.0, 0.1, "How far the shadow lies to the right of the letters (left, below 0), in pixels at 1080 lines - scaled with the screen's height.").dec(1),
    m("epilepsyWarningShadowOffsetY", -20.0, 20.0, 0.1, "How far the shadow lies below the letters (above, below 0), in pixels at 1080 lines - scaled with the screen's height.").dec(1),
    m("epilepsyWarningShadowRadius", 0.0, 20.0, 0.1, "How far the shadow spreads and softens, in pixels at 1080 lines: 0 a sharp copy of the letters.").dec(1),
    m("epilepsyWarningShadowRed", 0.0, 1.0, 0.01, "The shadow's colour: its red.").dec(2),
    m("epilepsyWarningShadowGreen", 0.0, 1.0, 0.01, "The shadow's colour: its green.").dec(2),
    m("epilepsyWarningShadowBlue", 0.0, 1.0, 0.01, "The shadow's colour: its blue.").dec(2),
    m("epilepsyWarningFont", 0.0, 3.0, 1.0, "The font it is set in: egui's own (Built-in), or one of Windows', as near the console's as they come; Built-in where they are not to be had.").select(&[(0.0, "Arial"), (1.0, "Segoe UI"), (2.0, "Bahnschrift"), (3.0, "Built-in")]),
    m("epilepsyWarningSize", 0.25, 3.0, 0.01, "Its letters' size, against the console's: 1 as tall, for the screen's height, as the console sets them.").dec(2),
    m("epilepsyWarningWidth", 0.2, 1.0, 0.005, "How wide its block may run before a line wraps, as a share of the screen's width.").dec(3),
    m("epilepsyWarningX", 0.0, 1.0, 0.005, "Where its block's centre lies across the screen, as a share of its width: 0.5 the middle.").dec(3),
    m("epilepsyWarningY", 0.0, 1.0, 0.005, "Where its block's centre lies down the screen, as a share of its height: 0.5 the middle; the console's sits a little below it.").dec(3),
    m("epilepsyWarningTextBlur", 0.0, 4.0, 0.05, "Softens the letters themselves, in pixels at 1080 lines: a fraction of a pixel takes the edge off them, as the console's video output did.").dec(2),
    m("epilepsyWarningGloss", 0.0, 1.0, 0.01, "Shades each line's letters from the top down - lit at the top, darker and a touch cooler towards their feet - after the console's glassy lettering.").dec(2),
    m("benchmarkLineInset", 0.0, 0.45, 0.005, "How far the benchmark screen's two lines lie from the top and the bottom, as a share of the 16:9 canvas's height - which, fitted into the screen, leaves a 4:3 screen more room above and below.").dec(3),
    m("seasonPass", 0.0, 2.0, 1.0, "A tour of the months' backdrops, in place of the clock's month: each month's colours for seasonPassHoldMin, at the clock's own hour, then a slow crossfade over seasonPassFadeSec to the next - in the calendar's order, or shuffled - of the months picked below. Wherever the clock's colours show - the scene at rest, or the music with musicColourChoice's Normal color - while the XMB's start keeps its own, and Theme Settings' Colour, a month held, wins.").select(&[(0.0, "Off"), (1.0, "In order"), (2.0, "Shuffled")]),
    m("seasonPassHoldMin", 0.1, 60.0, 0.1, "How long each month's colours stay, in minutes, before the next fades in.").dec(1).when("seasonPass", &[1.0, 2.0]),
    m("seasonPassFadeSec", 1.0, 120.0, 1.0, "How long the backdrop takes to fade from one month to the next, in seconds.").dec(0).when("seasonPass", &[1.0, 2.0]),
    m("seasonPassMonths", 1.0, 4095.0, 1.0, "The months the tour visits, as bits, January the lowest.").hide("set by the month toggles under the section"),
    m("intro", 0.0, 1.0, 1.0, "At Nagare's first start, after its benchmark and the epilepsy warning and before the XMB's start: a short card, after an advertisement of the console's day.").select(&[(0.0, "Off"), (1.0, "First start only")]),
    m("discoBall", 0.0, 1.0, 1.0, "A disco ball: once the music theme has played heavy bass for a few minutes, a mirror ball is lowered on its chain near the screen's top right corner and hangs there, spinning, throwing spots of light over the scene and glinting, until the music ends. It needs the music to be heard: one of the Music Theme Audio Visualizer's effects on.").select(OFF_ON),
    m("discoAfterMin", 0.0, 30.0, 0.1, "Minutes of heavy bass before it comes down. Bass that is not heavy drains it again, at half the pace.").dec(1).when("discoBall", ON),
    m("discoBassLevel", 0.05, 0.95, 0.01, "How loud the bass must be, from 0 to 1, as the visualizer hears it, to count as heavy.").dec(2).when("discoBall", ON),
    m("discoDescendSec", 1.0, 60.0, 0.5, "Seconds it takes to come down, easing in at the end.").dec(1).when("discoBall", ON),
    m("discoSizePx", 40.0, 256.0, 1.0, "Its width on the screen, in pixels.").dec(0).when("discoBall", ON),
    m("discoX", 0.0, 1.0, 0.001, "Where across the screen it hangs, as a share of the screen's width from the left: 0.667 is a third of the width in from the right edge.").dec(3).when("discoBall", ON),
    m("discoFlat", 0.0, 1.0, 1.0, "Drawn flat, as an icon: seen square on, its rows of tiles level. Off: seen from below, as it hangs above the eye, its rows curving round it.").select(OFF_ON).when("discoBall", ON),
    m("discoBeams", 0.0, 3.0, 0.05, "How bright the beams through the haze are, as in a room with smoke in its air: the spotlight's onto the ball, and those its tiles send back out, drifting fog in them. 0 none.").dec(2).when("discoBall", ON),
    m("discoFloor", 0.0, 3.0, 0.05, "How bright the lit dance floor is that the ball's lower tiles reflect - seen in the ball alone, nowhere else. 0 none.").dec(2).when("discoBall", ON),
    m("discoBeatLights", 0.0, 2.0, 0.05, "How much the lights go with the beat, as far as the visualizer is sure of it: on each beat they flash, the coloured light takes the palette's next colour, the white swings to the other side - the glints jumping with it - and the dance floor changes. 0 steady.").dec(2).when("discoBall", ON),
    m("discoSpin", 0.0, 0.5, 0.005, "How fast it turns, in turns a second.").dec(3).when("discoBall", ON),
    m("discoSpots", 0.0, 3.0, 0.05, "How bright the spots of light it throws onto the scene behind it are: 0 none.").dec(2).when("discoBall", ON),
    m("discoGlare", 0.0, 3.0, 0.05, "How bright and big its glints - stars on the tiles that catch a light - and its glow are: 0 none.").dec(2).when("discoBall", ON),
    m("maruSaturation", 0.0, 1.5, 0.01, "How much colour it has: 0 none, white glass as the XMB's icons are; 1 the 2D boat's colours.").dec(2),
    m("maruFlat", 0.0, 1.0, 1.0, "Drawn flat, as an icon: square to the screen, every part at the boat's own distance, so that the scene's perspective places and sizes it but does not distort it. Off: turned to the eye, in perspective, as a model in the scene.").select(OFF_ON),
    m("sharpen", 0.0, 1.0, 1.0, "Sharpens the finished frame - the scene and the particles, not the panel or the overlay - with Contrast Adaptive Sharpening (AMD FidelityFX's CAS, re-authored): it lifts soft detail and holds back where the contrast is already high, so edges do not ring. One pass of nine reads a pixel; the perf overlay's Sharpen rows say what it costs.").select(OFF_ON),
    m("sharpenAmount", 0.0, 1.0, 0.01, "CAS's sharpness: 0 the gentlest, 1 the most it gives.").dec(2).when("sharpen", ON),
    m("sharpenStrength", 0.0, 3.0, 0.01, "How much of what the sharpening adds is kept: 1 as CAS gives it, less to soften it, more to go past it (and nearer to halos).").dec(2).when("sharpen", ON),
    m("noiseEnabled", 0.0, 1.0, 1.0, "Noise in the scene's light, added before the exposure, the tone curve and the glare, fresh every frame, in linear light, so that back on the screen it shows most in the dark: AlexaGrain's film grain, or a sensor's noise.").select(OFF_ON),
    m("noiseMode", 0.0, 1.0, 1.0, "Which noise. AlexaGrain: an ARRI ALEXA's grain, as Riley Gray's AlexaGrain gizmo for Nuke draws it - grains of a few pixels, finer in grey than in colour, coarsest in blue, more in the light than in the dark as the camera's response gives it. Sensor: a sensor's read and shot noise, a value of its own in every pixel.").select(&[(0.0, "Sensor"), (1.0, "AlexaGrain")]).when("noiseEnabled", ON),
    m("grainAmount", 0.0, 4.0, 0.01, "The grain's strength: 1 the gizmo's Overall Amount, the camera's own.").dec(2).when("noiseMode", ON),
    m("grainSize", 0.25, 4.0, 0.01, "The grain's size: 1 an ALEXA SXT's 3.4k sensor across the screen's width, as the gizmo's sample resized to the plate; 2 a sensor half as wide, its grains twice as large.").dec(2).when("noiseMode", ON),
    m("grainIso", 160.0, 3200.0, 1.0, "The camera's ISO: above its native 800 the grain grows, as the response is read at a darker exposure and brought back.").select(&[(160.0, "160"), (200.0, "200"), (250.0, "250"), (320.0, "320"), (400.0, "400"), (500.0, "500"), (640.0, "640"), (800.0, "800 (native)"), (1000.0, "1000"), (1280.0, "1280"), (1600.0, "1600"), (2000.0, "2000"), (2560.0, "2560"), (3200.0, "3200")]).when("noiseMode", ON),
    m("noiseRead", 0.0, 0.01, 0.0001, "Read noise: the noise's floor, the same everywhere in linear light, which is what shows in the shadows - its standard deviation in linear light (1 the screen's white).").dec(4).when("noiseMode", CONSOLE),
    m("noiseShot", 0.0, 0.05, 0.0005, "Shot noise: noise that grows with the square root of the linear light, as photons' counts do - its standard deviation at the screen's white. On the screen it stays small in the highlights.").dec(4).when("noiseMode", CONSOLE),
    m("noiseChroma", 0.0, 1.0, 0.01, "How coloured the noise is: 0 the same in red, green and blue (grey grain), 1 each channel apart, as a sensor's colour filters give it.").dec(2).when("noiseMode", CONSOLE),
    m("noiseSize", 1.0, 4.0, 1.0, "The grain's size in screen pixels.").dec(0).when("noiseMode", CONSOLE),
    m("desatEnabled", 0.0, 1.0, 1.0, "Deprecated: it sux, not recommended. Highlight desaturation: where a pixel's brightest channel goes past desatStart, its colour gives way to white over desatRange, instead of each channel clipping on its own - which shifts a bright colour's hue and flattens it. Below desatStart nothing changes; the tone curve itself is left alone. Applied where the screen is written and where the glare is added.").select(OFF_ON),
    m("desatStart", 0.5, 2.0, 0.01, "Where the colour starts to give way: the brightest channel's value, 1 being where the screen clips.").dec(2).when("desatEnabled", ON),
    m("desatRange", 0.05, 4.0, 0.01, "How far past desatStart the colour is all gone.").dec(2).when("desatEnabled", ON),
    m("desatWhiten", 0.0, 1.0, 0.01, "What the colour gives way to: 0 a grey of the pixel's own luminance, which keeps its brightness; 1 a grey of its brightest channel, so that it burns to white, as film does.").dec(2).when("desatEnabled", ON),
    m("autoMusic", 0.0, 2.0, 1.0, "Plays the music scene while the system says media is playing, as the XMB's own does while it plays music: Windows' media sessions, the ones its media controls show. While playing: a session reports Playing. While controls show: any session at all, paused or not. Off leaves only the Music setting, further down under Console-original knobs. It adds to that setting, which still turns the music on by hand.\nWindows only").select(&[(0.0, "Off"), (1.0, "While playing"), (2.0, "While controls show")]),
    m("autoMusicHoldSec", 0.0, 30.0, 0.5, "How long the music scene stays after the media stops, so that a pause between tracks does not take it away.").dec(1).when("autoMusic", &[1.0, 2.0]),
    m("musicColourChoice", 0.0, 1.0, 1.0, "The colours while the music scene plays. Original: the music theme's own, its purple and its dark floor. Normal: the colours the scene has without the music - the clock's, the overridden clock's, Season Pass's or Theme Settings' Colour's - the backdrop's program and its day and night, and BACKGROUND.mnu's four colours, as far as musicColourMix takes them. Either way the motion, exposure and glare stay the music's, and the clock keeps ticking.").select(&[(0.0, "Original music theme (dank purple)"), (1.0, "Normal color (clock, fake clock, or Season Pass)")]),
    m("whenOtherFocused", 0.0, 3.0, 1.0, "While another program's window has the keyboard, the desktop and the taskbar aside. Limit frame rate draws at most limitFps; Pause holds the last frame, drawing nothing; Stop plays stopTransition, then frees the GPU's memory, until the condition ends and resumeTransition brings the scene back. Of several conditions at once, the strongest action wins.\nWindows, while the scene is the wallpaper").select(ACTIONS),
    m("whenOtherMaximized", 0.0, 3.0, 1.0, "While a maximized window covers the scene's monitor, so that the wallpaper is out of sight.\nWindows, while the scene is the wallpaper").select(ACTIONS),
    m("whenOtherFullscreen", 0.0, 3.0, 1.0, "While a window fills the scene's whole monitor - a game, a video, a presentation - or a program has the screen to itself (exclusive full screen, presentation mode).\nWindows, while the scene is the wallpaper").select(ACTIONS),
    m("whenProcessRunning", 0.0, 3.0, 1.0, "While any of the programs listed below runs, by the name of its file (game.exe), whether or not it shows a window.\nWindows only").select(ACTIONS).hide("the three process watches, each with its own action, replaced it; a config's single list moves to the one this action names"),
    m("whenOnBattery", 0.0, 3.0, 1.0, "While the PC runs on its battery.\nWindows only").select(ACTIONS),
    m("whenGpuMemoryLow", 0.0, 3.0, 1.0, "While the GPU's dedicated memory, all programs' together, is fuller than gpuMemoryThreshold: a game, a model being loaded. Stop frees the app's at once, without its transition; it comes back once the GPU's memory is ten points under the threshold, room for what it will take again.\nWindows only").select(ACTIONS),
    m("gpuMemoryThreshold", 50.0, 99.0, 1.0, "How full the GPU's dedicated memory may get, in percent, before whenGpuMemoryLow acts.\nWindows only").dec(0).when("whenGpuMemoryLow", SOME_ACTION),
    m("limitFps", 6.0, 120.0, 1.0, "The frame rate Limit frame rate holds the scene to, below the panel's own cap if it has one.").dec(0).presets(&[15.0, 30.0, 60.0]),
    m("stopTransition", 0.0, 2.0, 1.0, "How the scene goes when it stops. Game launch: the XMB's game launch sequence, then black as the game takes the screen. Background fade: the scene fades to black over backgroundFadeMs (Scene tab). At once: black at once. Its memory is freed once it is black.").select(&[(0.0, "Game launch"), (1.0, "Background fade"), (2.0, "At once")]),
    m("stopTransitionUrgent", 0.0, 2.0, 1.0, "How the scene goes when it stops for an urgent condition - the GPU's memory running low (whenGpuMemoryLow) or a program on its process watch running - where stopTransition is for the others: the same three ways. At once frees the GPU's memory soonest.").select(&[(0.0, "Game launch"), (1.0, "Background fade"), (2.0, "At once")]),
    m("stopHideWindow", 0.0, 1.0, 1.0, "What the scene leaves behind the icons while stopped. Black: its window, black, which keeps the buffers the driver holds for it (a few frames of the screen's size: about 45 MB at 2560 x 1440). Hide: the window goes too, so they are freed, and the desktop's own wallpaper shows until the scene comes back. A stop for the GPU's memory always hides it.\nWindows, while the scene is the wallpaper").select(&[(0.0, "Black"), (1.0, "Hide")]),
    m("resumeTransition", 0.0, 2.0, 1.0, "How the scene comes back after it stopped. XMB start: the XMB's start, as the console's comes back from a game. Background fade: the scene fades in over backgroundFadeMs. At once: as it is.").select(&[(0.0, "XMB start"), (1.0, "Background fade"), (2.0, "At once")]),
    m("renderer", 0.0, 2.0, 1.0, "The graphics API the scene is drawn with. Vulkan: the OpenGL renderer's programs and uniforms, rewritten for Vulkan, through wgpu - on macOS, Metal - with the wave built on the GPU if gpuWave says so. OpenGL: the original renderer. Software: the system's software rasteriser (WARP on Windows), on the CPU, for a PC whose GPU draws neither. Changing it makes the window and its renderer again. Where one cannot start, or stops working, the next draws - Vulkan, OpenGL, then software - and the panel's top says why.\n--renderer opengl, vulkan or software sets it for a run").select(&[(0.0, "OpenGL"), (1.0, "Vulkan"), (2.0, "Software")]),
    m("renderScale", 0.25, 1.0, 0.05, "The scene's size against the window's: below 1 it is drawn smaller and stretched to fill the screen, for a GPU (or a CPU, under the software renderer) that cannot keep up - 0.5 draws a quarter of the pixels. The panel and the warning are drawn at the window's full size. Under Vulkan it multiplies supersample.").dec(2),
    m("tripleBuffering", 0.0, 2.0, 1.0, "Lets the display queue a finished frame ahead of the one it shows, so that a frame held up - by the scheduler, say - for less than a frame's time still comes on time; it adds that queued frame to the latency. Auto: on while the frame rate is 30 or more, off below 20, where a queued frame would make the settings slow to answer. It only acts while frames wait for the display (no cap, or a cap at the refresh rate): a lower cap paces each frame itself, with time to spare.\nVulkan and the software renderer; OpenGL's is the driver's (NVIDIA's control panel)").select(&[(0.0, "Off"), (1.0, "On"), (2.0, "Auto")]),
    m("softwareLite", 0.0, 1.0, 1.0, "While the software renderer draws - chosen, or fallen back to - the heavier features are left out without touching their settings, which the panel marks: supersampling, maru-chan's supersampling, motion blur and scene light, the disco ball's beams and floor, sharpening, FXAA, the settings' blur, renderScale above 0.5, and gpuWave (the spline mesh built on the CPU). Off draws everything as set.").select(OFF_ON),
    m("gpuWave", 0.0, 1.0, 1.0, "Where the wave is built under Vulkan - the deformation of the 361 points and the 128 x 128 surface, the console's SPU program. GPU: as compute passes, in double precision where the GPU has it, the CPU working out only the vertices the particles are emitted from. CPU: as the OpenGL renderer always does, its mesh uploaded every frame.\nVulkan only").select(&[(0.0, "CPU"), (1.0, "GPU")]).when("renderer", &[1.0, 2.0]),
    m("cpuPlacement", 0.0, 2.0, 1.0, "Which cores the app's threads keep to - softly, so the system may still move them when those are taken. Efficiency cores: on a hybrid CPU (P- and E-cores), the E-cores; elsewhere the system decides. One cache cluster: the cores sharing one cache - one cluster of E-cores sharing an L2, or one L3 of a CPU with several (AMD's CCDs) - so the threads' data stays in it. Threads the driver starts later follow too.\nWindows' CPU sets, Linux's affinity, macOS's Background QoS").select(&[(0.0, "System decides"), (1.0, "Efficiency cores"), (2.0, "One cache cluster")]),
    m("cpuEcoQoS", 0.0, 1.0, 1.0, "Windows 11's EcoQoS: tells the system the app's speed matters less than its power, so it prefers the efficiency cores and lower clocks. Off leaves it to the system.\nWindows only").select(OFF_ON),
    m("cpuPriority", 0.0, 2.0, 1.0, "The app's priority: below normal lets every program at normal priority run first when they compete for the CPU. Auto: normal while Nagare or the desktop has the keyboard - the scene, its settings, the wallpaper in view - and below normal while another program has it.\nWindows' priority class, Linux's niceness, macOS's QoS; Auto reads the focus on Windows only, normal elsewhere").select(&[(0.0, "Normal"), (1.0, "Below normal"), (2.0, "Auto")]),
    m("blurEnabled", 0.0, 1.0, 1.0, "Blurs what is behind the settings, under a dark tint: the scene behind the panel, or behind the settings window.").select(OFF_ON),
    m("windowBackdrop", 0.0, 4.0, 1.0, "What the settings window (the wallpaper's) shows through. The scene: the scene, blurred, as it lies behind the window on the desktop, on any system - the panel inside the scene's window always shows it. Acrylic, Mica and Mica Alt: Windows 11's own materials, shown while the window is active - Acrylic blurs whatever is behind, other windows too; Mica tints with the desktop's wallpaper; Mica Alt more strongly. Acrylic (always): the older acrylic of Windows 10, which stays while the window is inactive, under its own tint.\nWindows 11 22H2 and later for Acrylic, Mica and Mica Alt; Windows 10 and later for Acrylic (always)").select(&[(0.0, "The scene"), (1.0, "Acrylic"), (2.0, "Mica"), (3.0, "Mica Alt"), (4.0, "Acrylic (always)")]).when("blurEnabled", ON),
    m("sceneBlurRadius", 1.0, 64.0, 1.0, "How far the scene's blur reaches, in points: behind the panel in the scene's window, and behind the settings window while it shows the scene.").dec(0).when("windowBackdrop", CONSOLE),
    m("backdropTint", 0.0, 1.0, 0.01, "How dark the tint the settings lay over what shows behind them is: 0 none, 1 black. Their text needs some to stand out.").dec(2).when("blurEnabled", ON),
    m("sceneBlurSaturation", 0.0, 2.0, 0.01, "The scene's blur's colour: 1 as the scene's, more makes it richer, as the system's acrylic does.").dec(2).when("windowBackdrop", CONSOLE),
    m("sceneBlurGrain", 0.0, 0.1, 0.001, "A fine grain over the scene's blur, which keeps its gradients from banding.").dec(3).when("windowBackdrop", CONSOLE),
    m("windowTheme", 0.0, 1.0, 1.0, "The system material's own shade, and the window's frame: dark or light.\nWindows 11").select(&[(0.0, "Dark"), (1.0, "Light")]).when("windowBackdrop", &[1.0, 2.0, 3.0, 4.0]),
    m("acrylicOpacity", 0.0, 1.0, 0.01, "How much of the older acrylic's own tint, dark or light as windowTheme says, lies over its blur.\nWindows 10 and later").dec(2).when("windowBackdrop", &[4.0]),
    m("windowCorners", 0.0, 3.0, 1.0, "The settings window's corners, as Windows 11 rounds them.\nWindows 11").select(&[(0.0, "System default"), (1.0, "Round"), (2.0, "Slightly round"), (3.0, "Square")]),
    m("uiScale", 0.5, 2.0, 0.05, "The settings' size, over the display's own scale.").dec(2),
    m("pokesToOpen", 0.0, 20.0, 1.0, "Pokes - clicks on the scene, or on the desktop behind the icons, each soon after the last - that open the settings. Each bounces the camera, and after the first few a hint counts down the rest. 0 opens them only from the tray, H and the right-click logo.").dec(0),
    m("pokeBounce", 0.0, 0.3, 0.005, "How far each poke bounces the camera forwards and back, in the scene's units (the camera stands 2 from the wave).").dec(3),
    m("logoOnRightClick", 0.0, 1.0, 1.0, "Right-clicking the scene, or the desktop behind the icons, shows the logo above and left of the pointer, which opens the settings.").select(OFF_ON),
    m("musicColourMix", 0.0, 0.5, 0.01, "How far the colours go from the music's towards the normal ones: 0 the music's, up to half way to the normal ones. The backdrop's program, a whole number, switches to the normal one at a half.").dec(2).when("musicColourChoice", ON),
    m("audioReactive", 0.0, 1.0, 1.0, "While the music scene plays, listens to what the system plays and lets the wave's brightness rise with how loud it has been for a while - not with each beat - and flicker finely over it: loud music stirs it, a podcast barely does. Listening starts only with the music scene.\nWindows only").select(&[(0.0, "Off"), (1.0, "On")]),
    m("audioFlicker", 0.0, 1.0, 0.005, "How far the brightness flickers at full activity, as a share of it: 0.42 moves it by up to 42% either way.").dec(3).when("audioReactive", ON),
    m("audioFlickerRate", 0.2, 10.0, 0.1, "How fast the flicker moves, in changes a second.").dec(1).when("audioReactive", ON),
    m("audioFollow", 0.0, 1.0, 0.01, "How much the flicker follows the music's own swells over its average, rather than wandering at random: 0 random, 1 only the music.").dec(2).when("audioReactive", ON),
    m("audioQuiet", -80.0, 0.0, 1.0, "The average loudness, in dB below full scale, at which there is no activity at all.").dec(0),
    m("audioLoud", -60.0, 0.0, 1.0, "The average loudness at which the activity is full.").dec(0),
    m("audioWindow", 1.0, 60.0, 0.5, "The seconds the loudness is averaged over, its pauses included.").dec(1),
    m("audioRise", 0.1, 30.0, 0.1, "How many seconds the activity takes to rise to the loudness.").dec(1),
    m("audioFall", 0.1, 30.0, 0.1, "How many seconds it takes to fall back.").dec(1),
    m("audioBassBoost", 0.0, 1000.0, 0.5, "How much more the bass counts in the loudness the activity and the flicker read, in dB: its bands (below 100 Hz, and around 180 Hz) added again at this gain. 0 the loudness as it is.").dec(1),
    m("tempoBounce", 0.0, 1.0, 1.0, "While the music scene plays and the music has been loud a while, finds its beat and bounces the camera on it, as a poke does - met as the beat falls, not after.\nWindows only").select(&[(0.0, "Off"), (1.0, "On")]),
    m("tempoBounceAmount", 0.0, 0.2, 0.002, "How far each beat bounces the camera, in the scene's units.").dec(3).when("tempoBounce", ON),
    m("tempoActivity", 0.0, 1.0, 0.01, "The activity the music must have reached before the beat bounces the camera.").dec(2).when("tempoBounce", ON),
    m("tempoConfidence", 0.0, 1.0, 0.01, "How sure of the beat it must be: the onsets' correlation at the beat's period, 0 to 1.").dec(2).when("tempoBounce", ON),
    m("tempoOnsets", 0.0, 1.0, 1.0, "What the beat is found in. Bass and percussion: the rise of four bands' energy - the kick's (below 100 Hz), the bass's body (around 180 Hz), the snare's crack (around 2 kHz) and the hats' (above 7 kHz) - the bass weighted most. Whole band: the low band's and the whole band's, as before.").select(&[(0.0, "Whole band"), (1.0, "Bass and percussion")]).when("tempoBounce", ON),
    m("tempoBassWeight", 0.0, 4.0, 0.05, "How much the kick's and the bass's bands count in the beat, against the snare's (1) and the hats' (0.5).").dec(2).when("tempoOnsets", ON),
    m("tempoGate", 0.0, 3.0, 1.0, "What holds the bounce back when the song goes quiet, so that a beat still followed does not bounce on: the loudness at the moment, the bass's and the percussion's at the moment (each against its average less tempoGateDrop), or whether the beats are still heard where they fall. Each bounce is as large as the gate is open.").select(&[(0.0, "Off"), (1.0, "Loudness now"), (2.0, "Bass and drums now"), (3.0, "Beats still heard")]).when("tempoBounce", ON),
    m("tempoGateDrop", 0.0, 30.0, 0.5, "How far under its average over the window the level may fall, in dB, before the gate starts to shut; it is shut 6 dB further down, and below audioQuiet.").dec(1).when("tempoGate", &[1.0, 2.0]),
    m("bassFlicker", 0.0, 1.0, 1.0, "With audioReactive, flickers the wave's brightness at the bass's own pitch, times bassRate, as deep as the bass is loud: a 42 Hz sub flickers differently from a 70 Hz bass line or a 100 Hz kick.\nWindows only").select(&[(0.0, "Off"), (1.0, "On")]).when("audioReactive", ON),
    m("bassFlickerDepth", 0.0, 0.5, 0.005, "How far the bass flickers the brightness at its loudest, as a share of it - held under half of bassRate (0.5 at 1x, 0.25 at 0.5x, 0.125 at 0.25x), so that the slower flickers, which a display shows as they are, flash less.").dec(3).when("bassFlicker", ON),
    m("bassRate", 0.25, 1.0, 0.25, "The flicker's and the shake's rate against the bass's pitch: at 0.5x a 70 Hz bass flickers at 35 Hz.").select(&[(0.25, "0.25x"), (0.5, "0.5x"), (1.0, "1x")]),
    m("bassFold", 0.0, 1.0, 1.0, "Halves the rate, an octave at a time, until the frame rate can show it - at 60 fps, no faster than 27 Hz. Off, a faster rate shows as whatever the frames catch of it, a slower flicker that rises and falls with the pitch.").select(&[(0.0, "Off"), (1.0, "On")]),
    m("bassLow", 20.0, 200.0, 1.0, "The lowest bass pitch looked for, in Hz. The bass's flicker and its shake follow only a pitch between bassLow and bassHigh; outside it they rest.").dec(0),
    m("bassHigh", 40.0, 250.0, 1.0, "The highest bass pitch looked for, in Hz: a bass above it neither flickers the brightness nor shakes the camera.").dec(0),
    m("bassQuiet", -80.0, 0.0, 1.0, "The bass's level (below 180 Hz, over the last 85 ms), in dB, at which its flicker and its shake have no effect; they deepen from here to bassLoud.").dec(0),
    m("bassLoud", -60.0, 0.0, 1.0, "The bass's level at which its flicker and its shake are full, in dB.").dec(0),
    m("subShake", 0.0, 1.0, 1.0, "Shakes the camera, forwards and back, at the bass's rate, as deep as it is loud: the wave shaken by the subs.\nWindows only").select(&[(0.0, "Off"), (1.0, "On")]),
    m("subShakeAmount", 0.0, 0.05, 0.0005, "How far the bass shakes the camera at its loudest, in the scene's units.").dec(4).when("subShake", ON),
    m("kickBounce", 0.0, 1.0, 1.0, "Bops the camera on each kick as it is heard - the low bands' (below 100 Hz, and around 180 Hz) sudden rise - besides the beat's bounce, which comes on the beat found.\nWindows only").select(&[(0.0, "Off"), (1.0, "On")]),
    m("kickBounceAmount", 0.0, 0.2, 0.002, "How far a kick bops the camera, half to one and a half times this as it hits softer or harder.").dec(3).when("kickBounce", ON),
    m("kickThreshold", 0.1, 5.0, 0.05, "How sharply the low bands must rise to count as a kick (in their log; 1 is about 4 dB in 10 ms).").dec(2).when("kickBounce", ON),
    m("bassWarmDelay", 0.0, 30.0, 0.5, "How long a sure beat (tempoConfidence) must be heard before the bass's effects - its flicker, its shake and the kicks' bops - begin to open, in seconds.").dec(1),
    m("bassWarmUp", 0.0, 60.0, 0.5, "How long they take to open fully, in seconds; once open they answer the bass at once.").dec(1),
    m("bassWarmHold", 0.0, 30.0, 0.5, "How long without music, in minutes, before they close and the next music warms them up again.").dec(1),
    m("motionBlur", 0.0, 1.0, 1.0, "Draws the wave and the particles several times a frame while the pokes or the music move the camera or flicker the brightness within it, each at its moment, and averages them, as a camera's shutter open the whole frame would: a bop or a shake blurs into motion instead of jumping between frames, and a flicker faster than the frame rate averages out rather than showing as a slower one. Frames where nothing moves are drawn once.").select(&[(0.0, "Off"), (1.0, "On")]),
    m("motionBlurHz", 60.0, 480.0, 10.0, "The rate the sub-frames are drawn at: 240 draws four a frame at 60 fps, two at 120 and one at 240. Each costs another draw of the wave and the particles.").dec(0).when("motionBlur", ON),
];
