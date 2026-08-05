// ── Visualizer: 3D sphere of dots ───────────────────────────────────────────
//
// Expects globals from index.html: `audio` (the <audio> element) and
// `currentView` (the active nav view name). Exposes `renderVisualizer()` and
// `stopVisualizer()`, which index.html's showView() calls on view switch.

let audioCtx = null;
let analyser = null;
let sourceNode = null;
let freqData = null;
let timeData = null; // raw waveform samples, used to measure true loudness (getByteFrequencyData is dB-compressed and barely moves with the volume slider)
let binPeakBuf = null; // per-bin running peak, for normalizing each frequency band independently
let binNormBuf = null; // per-bin value relative to that band's own recent peak, 0..1

function ensureAudioGraph() {
  if (audioCtx) return;
  audioCtx = new (window.AudioContext || window.webkitAudioContext)();
  sourceNode = audioCtx.createMediaElementSource(audio);
  analyser = audioCtx.createAnalyser();
  analyser.fftSize = 256;
  analyser.smoothingTimeConstant = 0.82;
  freqData = new Uint8Array(analyser.frequencyBinCount);
  timeData = new Uint8Array(analyser.fftSize);
  binPeakBuf = new Float32Array(analyser.frequencyBinCount).fill(BIN_PEAK_FLOOR);
  binNormBuf = new Float32Array(analyser.frequencyBinCount);
  sourceNode.connect(analyser);
  analyser.connect(audioCtx.destination);
}

const SPHERE_DOT_COUNT = 1400;
const SPHERE_BASE_RADIUS = 5;
const SPHERE_MIN_RADIUS = 1.2;
const SPHERE_MAX_RADIUS = SPHERE_BASE_RADIUS + 4.5; // hard cap so dots can't fly out past a reasonable distance
const SWING_GAIN = 3.5; // was 7 — how much amplitude pushes a dot outward
const BEAT_SWING_GAIN = 1.2; // was 3 — how much a beat pulse pushes a dot outward
const POLE_ROTATE_BASE_SPEED = 0.15; // radians/sec baseline tumble speed for the frequency poles
const POLE_ROTATE_BASS_GAIN = 1.5; // extra speed from sustained bass energy
const POLE_ROTATE_BEAT_KICK = 3.5; // extra speed spike right on a detected beat
const BIN_PEAK_DECAY_RATE = 0.5; // how fast each band's running peak relaxes back down (per second)
const BIN_PEAK_FLOOR = 0.12; // minimum peak, so quiet/noise-floor bins don't get wildly over-amplified
const RMS_REF = 0.22; // waveform RMS (0..~0.7) that counts as "full volume" for the envelope gate — loud, well-mastered music sits around here at volume 1.0

let sphereRenderer = null;
let sphereScene = null;
let sphereCamera = null;
let sphereControls = null;
let sphereMesh = null;
let sphereResizeObserver = null;
let sphereRAF = null;
let spherePoints = null;
let sizePhase = null;
let sizeFreq = null;
let swingGain = null;
let tmpMatrix = null;
let tmpColor = null;
let radiusBuf = null;
let radiusRangeMin = SPHERE_BASE_RADIUS;
let radiusRangeMax = SPHERE_BASE_RADIUS;
let ampSmoothBuf = null;
const AMP_SMOOTH_RATE = 20; // higher = snappier, lower = smoother (per second, dt-based)
let poleAxis = null;
let poleQuat = null;
let poleAngleAccum = 0;
let tmpDir = null;

let visualizerTimeAccum = 0;
let lastFrameTs = 0;
let bassEMA = 0;
let beatPulse = 0;
let beatCooldown = 0;
let overallEnergyEMA = 0;

function renderVisualizer() {
  const main = document.getElementById('main');
  main.classList.add('visualizer-active');
  main.innerHTML = `
    <div class="visualizer-wrap" id="visualizer-wrap">
      <div class="visualizer-vignette"></div>
    </div>`;

  ensureAudioGraph();
  if (audioCtx.state === 'suspended') audioCtx.resume();

  if (window.THREE && window.OrbitControls) {
    initSphereVisualizer();
  } else {
    document.getElementById('visualizer-wrap').insertAdjacentHTML('afterbegin',
      '<p style="color:var(--text-muted);font-size:13px;">Loading visualizer…</p>');
    window.addEventListener('three-ready', function onReady() {
      window.removeEventListener('three-ready', onReady);
      if (currentView === 'visualizer') initSphereVisualizer();
    });
  }
}

function stopVisualizer() {
  document.getElementById('main').classList.remove('visualizer-active');
  cancelAnimationFrame(sphereRAF);
  sphereRAF = null;
  if (sphereResizeObserver) {
    sphereResizeObserver.disconnect();
    sphereResizeObserver = null;
  }
  if (sphereControls) {
    sphereControls.dispose();
    sphereControls = null;
  }
  if (sphereRenderer) {
    sphereRenderer.dispose();
    sphereRenderer.domElement.remove();
    sphereRenderer = null;
  }
  sphereScene = null;
  sphereCamera = null;
  sphereMesh = null;
  spherePoints = null;
  sizePhase = null;
  sizeFreq = null;
  swingGain = null;
  radiusBuf = null;
  poleAxis = null;
  poleQuat = null;
  tmpDir = null;
  ampSmoothBuf = null;
}

function initSphereVisualizer() {
  const THREE = window.THREE;
  const wrap = document.getElementById('visualizer-wrap');
  if (!wrap) return;

  const width = wrap.clientWidth;
  const height = wrap.clientHeight;

  sphereScene = new THREE.Scene();
  sphereScene.fog = new THREE.FogExp2(0x0f0f0f, 0.02);

  sphereCamera = new THREE.PerspectiveCamera(55, width / height, 0.1, 100);
  sphereCamera.position.set(0, 0, 11);

  sphereRenderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
  sphereRenderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
  sphereRenderer.setSize(width, height);
  sphereRenderer.domElement.style.position = 'relative';
  sphereRenderer.domElement.style.zIndex = '1';
  wrap.insertBefore(sphereRenderer.domElement, wrap.firstChild);

  sphereControls = new window.OrbitControls(sphereCamera, sphereRenderer.domElement);
  sphereControls.enableDamping = true;
  sphereControls.dampingFactor = 0.08;
  sphereControls.enablePan = false;
  sphereControls.minDistance = 6;
  sphereControls.maxDistance = 28;

  sphereScene.add(new THREE.AmbientLight(0x404050, 1.2));
  const keyLight = new THREE.PointLight(0xc8a96e, 2, 60);
  keyLight.position.set(8, 10, 8);
  sphereScene.add(keyLight);
  const rimLight = new THREE.PointLight(0x4488ff, 1.5, 60);
  rimLight.position.set(-10, -6, -8);
  sphereScene.add(rimLight);

  const dotGeo = new THREE.IcosahedronGeometry(0.16, 1);
  const dotMat = new THREE.MeshBasicMaterial({ fog: false });
  sphereMesh = new THREE.InstancedMesh(dotGeo, dotMat, SPHERE_DOT_COUNT);
  sphereScene.add(sphereMesh);

  tmpMatrix = new THREE.Matrix4();
  tmpColor = new THREE.Color();
  tmpDir = new THREE.Vector3();

  // The frequency poles (bass at one point, treble at its antipode) tumble
  // smoothly around this fixed, randomly chosen axis for the life of the view.
  poleAxis = new THREE.Vector3(Math.random() - 0.5, Math.random() - 0.5, Math.random() - 0.5).normalize();
  poleQuat = new THREE.Quaternion();
  poleAngleAccum = 0;

  // Fibonacci sphere: an even distribution of base directions for each dot
  spherePoints = [];
  sizePhase = new Float32Array(SPHERE_DOT_COUNT);
  sizeFreq = new Float32Array(SPHERE_DOT_COUNT);
  swingGain = new Float32Array(SPHERE_DOT_COUNT);
  radiusBuf = new Float32Array(SPHERE_DOT_COUNT);
  radiusRangeMin = SPHERE_BASE_RADIUS;
  radiusRangeMax = SPHERE_BASE_RADIUS;
  ampSmoothBuf = new Float32Array(SPHERE_DOT_COUNT);
  const golden = Math.PI * (3 - Math.sqrt(5));
  for (let i = 0; i < SPHERE_DOT_COUNT; i++) {
    const y = 1 - (i / (SPHERE_DOT_COUNT - 1)) * 2;
    const r = Math.sqrt(Math.max(0, 1 - y * y));
    const theta = golden * i;
    spherePoints.push(new THREE.Vector3(Math.cos(theta) * r, y, Math.sin(theta) * r));
    sizePhase[i] = Math.random() * Math.PI * 2;
    sizeFreq[i] = 0.5 + Math.random() * 2;
    swingGain[i] = 0.6 + Math.random() * 2.2;
  }

  resizeSphereRenderer();
  sphereResizeObserver = new ResizeObserver(resizeSphereRenderer);
  sphereResizeObserver.observe(wrap);

  lastFrameTs = 0;
  sphereRAF = requestAnimationFrame(drawSphereVisualizer);
}

function resizeSphereRenderer() {
  const wrap = document.getElementById('visualizer-wrap');
  if (!wrap || !sphereRenderer) return;
  const w = wrap.clientWidth, h = wrap.clientHeight;
  if (w === 0 || h === 0) return;
  sphereCamera.aspect = w / h;
  sphereCamera.updateProjectionMatrix();
  sphereRenderer.setSize(w, h);
}

function drawSphereVisualizer(ts) {
  sphereRAF = requestAnimationFrame(drawSphereVisualizer);
  if (!sphereRenderer) return;

  const dt = lastFrameTs ? Math.min((ts - lastFrameTs) / 1000, 0.05) : 0.016;
  lastFrameTs = ts;
  visualizerTimeAccum += dt;

  const playing = analyser && !audio.paused && !audio.ended && audio.currentTime > 0;
  if (analyser) analyser.getByteFrequencyData(freqData);

  let bassEnergy;
  if (playing) {
    let bassSum = 0;
    for (let i = 0; i < 6; i++) bassSum += freqData[i];
    bassEnergy = bassSum / 6 / 255;
  } else {
    bassEnergy = 0.15 + 0.08 * Math.sin(visualizerTimeAccum * 1.2);
  }

  bassEMA += (bassEnergy - bassEMA) * 0.08;
  beatCooldown = Math.max(0, beatCooldown - dt);
  if (playing && bassEnergy > bassEMA * 1.3 && bassEnergy > 0.35 && beatCooldown <= 0) {
    beatPulse = 1;
    beatCooldown = 0.15;
  }
  beatPulse *= Math.pow(0.02, dt);

  const binCount = freqData ? freqData.length : 0;

  // normalize each frequency band against its own recent peak so quiet/treble
  // bins get to swing just as far as the naturally louder bass bins, instead
  // of bass dominating the whole sphere while everything else stays flat
  let overallEnvelope = 1;
  if (playing) {
    const usableBins = Math.floor(binCount * 0.85);
    const peakDecay = Math.exp(-BIN_PEAK_DECAY_RATE * dt);
    for (let b = 0; b < usableBins; b++) {
      const raw = freqData[b] / 255;
      let peak = binPeakBuf[b] * peakDecay;
      if (raw > peak) peak = raw;
      if (peak < BIN_PEAK_FLOOR) peak = BIN_PEAK_FLOOR;
      binPeakBuf[b] = peak;
      binNormBuf[b] = raw / peak;
    }

    // Per-band normalization above is scale-invariant on purpose (that's what
    // balances bass vs. treble), but it means a bin can hit its own "peak" even
    // at near-silent absolute levels — nothing in it responds to the actual
    // volume slider. getByteFrequencyData is also the wrong signal to measure
    // real loudness from: it's dB-compressed against a fixed ~70dB window, so
    // halving the linear volume barely moves it. Instead, measure the true
    // waveform loudness (RMS of the raw samples) and gate everything by that —
    // audio.volume is a linear gain applied before this point, so RMS scales
    // with it directly and the sphere calms down exactly when the user turns
    // the volume down, not just when the track happens to go quiet.
    analyser.getByteTimeDomainData(timeData);
    let sumSq = 0;
    for (let s = 0; s < timeData.length; s++) {
      const v = (timeData[s] - 128) / 128;
      sumSq += v * v;
    }
    const rms = Math.sqrt(sumSq / timeData.length);
    overallEnergyEMA += (rms - overallEnergyEMA) * 0.15;
    // sqrt-compress so the gate opens up quickly off silence but still tapers
    // smoothly into full swing, instead of a harsh linear on/off ramp
    overallEnvelope = Math.min(1, Math.sqrt(overallEnergyEMA / RMS_REF));
  }

  // tumble speed rides on sustained bass energy and spikes on each detected beat
  const poleRotateSpeed = POLE_ROTATE_BASE_SPEED * (1 + bassEMA * POLE_ROTATE_BASS_GAIN + beatPulse * POLE_ROTATE_BEAT_KICK);
  poleAngleAccum += poleRotateSpeed * dt;
  poleQuat.setFromAxisAngle(poleAxis, poleAngleAccum);

  let frameMin = Infinity;
  let frameMax = -Infinity;

  // dt-based exponential smoothing factor, so per-dot amplitude eases toward
  // its target each frame instead of snapping straight to the raw FFT reading
  const ampSmoothing = 1 - Math.exp(-AMP_SMOOTH_RATE * dt);

  for (let i = 0; i < SPHERE_DOT_COUNT; i++) {
    // rotate this dot's base direction around the slowly tumbling pole axis so
    // which physical point is "bass" vs "treble" drifts smoothly over time
    tmpDir.copy(spherePoints[i]).applyQuaternion(poleQuat);

    let amp;
    if (playing) {
      const bin = Math.floor((i / SPHERE_DOT_COUNT) * binCount * 0.85);
      amp = binNormBuf[bin] * overallEnvelope;
    } else {
      amp = 0.12 + 0.1 * Math.sin(visualizerTimeAccum * 1.6 + i * 0.15);
    }

    ampSmoothBuf[i] += (amp - ampSmoothBuf[i]) * ampSmoothing;
    amp = ampSmoothBuf[i];

    const swing = (amp - 0.32) * SWING_GAIN * swingGain[i] + beatPulse * BEAT_SWING_GAIN * swingGain[i];
    let radius = SPHERE_BASE_RADIUS + swing;
    radius = radius < SPHERE_MIN_RADIUS ? SPHERE_MIN_RADIUS : radius > SPHERE_MAX_RADIUS ? SPHERE_MAX_RADIUS : radius;
    const scale = 0.6 + 0.5 * Math.sin(visualizerTimeAccum * sizeFreq[i] + sizePhase[i]);

    tmpMatrix.makeScale(scale, scale, scale);
    tmpMatrix.setPosition(tmpDir.x * radius, tmpDir.y * radius, tmpDir.z * radius);
    sphereMesh.setMatrixAt(i, tmpMatrix);

    radiusBuf[i] = radius;
    if (radius < frameMin) frameMin = radius;
    if (radius > frameMax) frameMax = radius;
  }

  // auto-range the color mapping to this frame's actual spread of radii (smoothed
  // over time) so the dots always sweep the full color spectrum as they bounce,
  // regardless of how loud or quiet the current audio is.
  const rangeSmooth = Math.min(1, dt * 4);
  radiusRangeMin += (frameMin - radiusRangeMin) * rangeSmooth;
  radiusRangeMax += (frameMax - radiusRangeMax) * rangeSmooth;
  const span = Math.max(0.75, radiusRangeMax - radiusRangeMin);

  for (let i = 0; i < SPHERE_DOT_COUNT; i++) {
    // color by distance from the sphere's center: near = one end of the spectrum,
    // far = the other, sweeping the full rainbow as the dot bounces in and out.
    let distT = (radiusBuf[i] - radiusRangeMin) / span;
    distT = distT < 0 ? 0 : distT > 1 ? 1 : distT;
    const hue = distT * 0.85;
    tmpColor.setHSL(hue, 1, Math.min(0.75, 0.4 + distT * 0.35 + beatPulse * 0.15));
    sphereMesh.setColorAt(i, tmpColor);
  }

  sphereMesh.instanceMatrix.needsUpdate = true;
  if (sphereMesh.instanceColor) sphereMesh.instanceColor.needsUpdate = true;

  sphereControls.update();
  sphereRenderer.render(sphereScene, sphereCamera);
}
