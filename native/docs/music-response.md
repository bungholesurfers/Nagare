# Responding to music as a listener would: research notes

Research only, nothing of it built yet (2026-10-23). What `src/audio.rs` could do so that the scene's response to
the music - the wave's brightness, the camera's bounce, the bass's effects - reads as a person hears the music, and
could be pushed further than the subdued settings the current analysis allows. Each idea with its cost and how sure
the finding is.

## Recommended, best payoff first

1. **Lead the beat by the system's latency.** People notice audio and picture out of step within tens of
   milliseconds (ITU-R BT.1359), and visuals that land late feel worse than early ones. The tempo tracker already
   predicts the next beat's time, so its effects can be fired early by the capture's latency (the WASAPI loopback
   buffer) plus the display's (swap chain, refresh), about 20-60 ms, to be measured by eye.
   *Cheap; benefit medium-high confidence, the offset needs tuning.*
2. **Perceptual loudness: ITU-R BS.1770 (LUFS).** K-weighting (a high-shelf and a high-pass, two biquads), then mean
   square over 400 ms ("momentary") and 3 s ("short-term"). Brightness following the momentary loudness against the
   short-term one matches heard dynamics far better than raw RMS or a bass-boosted level, and makes quiet and loud
   tracks behave alike. *Cheap; high confidence.*
3. **Mel bands with per-band automatic gain, asymmetric smoothing.** About 24 mel-spaced bands, each normalised by
   its own running range (gain control per band), each smoothed with a fast rise and a slow fall. The technique the
   hobby audio-reactive projects converge on because it "looks musical": Scott Lawson's audio-reactive-led-strip
   (MIT), LedFx, WLED's audioreactive. *A few hundred lines; high confidence.*
4. **SuperFlux onsets** (Böck & Widmer, DAFx 2013). Spectral flux on a log-magnitude mel spectrogram, with a maximum
   filter across frequency over the previous frame, so vibrato and sustained tones do not trigger. Still among the
   best simple onset detectors; a small change from the band onsets used now. *Medium-high confidence.*
5. **Downbeats and the bar.** Listeners feel the "one" of the bar: phrase starts, drops. A metrical-position stage on
   top of the beat - a particle filter or an HMM over the beat's place in the bar, as BeatNet does - lets effects
   accent downbeats and stay calm between them, which reads as musical rather than metronomic. *Moderate work, fiddly
   tuning; medium confidence.*
6. **Sections: build-ups, drops, breakdowns.** Compare the last few seconds with the last ~30 s - loudness, spectral
   centroid, onset density - to let the scene "open up" on a drop. Foote's checkerboard novelty on a self-similarity
   matrix is the heavier, classic form. *The cheap form is cheap; medium confidence.*

Suggested order: 1, then 2, 3 and 4; then 5 and 6.

## Not recommended

- **aubio** (onsets, tempo; Rust bindings `aubio`, `aubio-rs`, `bliss-audio-aubio-rs`): GPL-3.0, which Nagare's MIT
  licence cannot take in; its beat tracking is not clearly better than ours.
- **BTrack** (Stark, Davies, Plumbley; causal, real-time, C++): GPL-3.0, the same problem.
- **madmom**: BSD code, but its trained models are under a Creative Commons non-commercial share-alike licence.
- **Essentia**: AGPL-3.0.
- **BeatNet** (Heydari, Cwitkowitz, Duan, ISMIR 2021; BeatNet+, TISMIR 2024): the state of the art for online beat,
  downbeat and meter tracking - a convolutional-recurrent network with particle filtering - but PyTorch; in Rust it
  would mean exporting it to ONNX and running it with ONNX Runtime (`ort`, ~10-20 MB). The code's licence was not
  confirmed. *Feasible but heavy; low-to-medium confidence that it is worth it.*

## Sources

- [BeatNet: CRNN and Particle Filtering for Online Joint Beat Downbeat and Meter Tracking](https://deepai.org/publication/beatnet-crnn-and-particle-filtering-for-online-joint-beat-downbeat-and-meter-tracking)
- [BeatNet+ (TISMIR, 2024)](https://doaj.org/article/393c7ce2039d4e8dad5843c4c498a6d7)
- [madmom: a new Python Audio and Music Signal Processing Library](https://arxiv.org/pdf/1605.07008), [madmom on PyPI](https://pypi.org/project/madmom)
- [BTrack](https://code.soundsoftware.ac.uk/projects/btrack)
- [aubio](https://aubio.org/), [aubio crate](https://docs.rs/aubio), [aubio-rs](https://lib.rs/crates/aubio-rs)
- From knowledge, not looked up this time: ITU-R BS.1770 (loudness), ITU-R BT.1359 (audio-video timing), Böck & Widmer
  "Maximum filter vibrato suppression for onset detection" (DAFx 2013), Foote "Automatic audio segmentation using a
  measure of audio novelty" (ICME 2000), Scott Lawson's audio-reactive-led-strip.
