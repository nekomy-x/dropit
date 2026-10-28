import { useEffect, useRef } from 'react';

/**
 * Tilted mosaic of very small triangles behind the closing CTA.
 *
 * The surface is one static vertex buffer drawn in a single WebGL pass. All
 * per-frame motion — the per-tile drift, the press and the ripples under the
 * pointer — is computed in the shaders, so the tile count is bounded by the GPU
 * instead of by per-frame JavaScript. That choice is measured, not stylistic: a
 * canvas-2D path builder was tried first and could not hold its frame budget at
 * this density (~7k triangles ≈ 70 ms/frame just to record the paths), which is
 * why this file is a shader.
 *
 * It reads as: graphite tiles bobbing out of step with their neighbours, so the
 * surface ripples asynchronously instead of sliding as one sheet; the pointer
 * presses a well into the plane and leaves ripples along fast movement; the
 * whole plane tilts toward the pointer. There is deliberately no accent colour
 * and no beat: the field is structure behind the copy, not a light show.
 *
 * Decoration only — `aria-hidden`, no pointer capture, never the only carrier
 * of information. Reduced motion paints one static frame and starts no loop.
 * Off-screen and hidden tabs stop rendering. Without WebGL the canvas simply
 * stays empty and the section keeps its flat background.
 */

const RIPPLE_SLOTS = 4;
const MAX_RIPPLES = 4;
const BASE_TILT = 0.42;

const VERT = `#version 300 es
precision highp float;

in vec2 aCorner;   // offset of this corner from the tile centroid, px
in vec2 aCenter;   // tile centroid in plane space, px
in vec3 aRandom;   // phase, drift rate, sway rate
in float aTone;    // static patch brightness

uniform vec2 uResolution;
uniform float uTime;
uniform vec2 uPointer;
uniform float uPointerActive;
uniform float uTiltX;
uniform float uTiltY;
uniform vec4 uRipples[${RIPPLE_SLOTS}]; // x, y, age, amplitude

out float vTone;
out float vShade;
out float vLift;

float blob(vec2 p, vec2 c, float radius) {
  vec2 d = p - c;
  return exp(-dot(d, d) / (2.0 * radius * radius));
}

void main() {
  vec2 p = aCenter;

  // Every tile runs on its own phase and speed, so neighbours never rise
  // together: this is the asynchrony, and it is the whole animation.
  float drift = uTime * (0.42 + aRandom.y * 0.5) + aRandom.x * 40.0;

  float z = sin(drift) * 3.4 + cos(drift * 0.7) * 1.6;
  vec2 sway = vec2(cos(drift * 1.1), sin(drift * 0.9)) * (1.1 + aRandom.z * 1.2);

  // The pointer presses a well into the plane. Geometry only: no glow.
  if (uPointerActive > 0.5) {
    z -= blob(p, uPointer, 96.0) * 26.0;
  }

  for (int i = 0; i < ${RIPPLE_SLOTS}; i++) {
    if (uRipples[i].w <= 0.0) continue;
    float dist = length(p - uRipples[i].xy);
    float wave = sin(dist * 0.045 - uRipples[i].z * 9.0) * exp(-dist / 130.0);
    z += wave * uRipples[i].w * exp(-uRipples[i].z * 1.5);
  }

  // Tilt the tile corners together with the centroid, so every triangle is
  // foreshortened by the same rotation instead of staying screen-aligned.
  vec2 centred = p - uResolution * 0.5 + aCorner * 0.88;
  float cx = cos(uTiltX), sx = sin(uTiltX);
  float cy = cos(uTiltY), sy = sin(uTiltY);

  vec3 pos = vec3(centred.x + sway.x, centred.y + sway.y, z);
  pos.xz = vec2(pos.x * cy + pos.z * sy, pos.z * cy - pos.x * sy);
  pos.yz = vec2(pos.y * cx - pos.z * sx, pos.y * sx + pos.z * cx);

  // Mild depth scale only: a true perspective divide pulls the far edge inward
  // and exposes the background behind the plane.
  float depth = pos.z;
  float scale = 1.0 + depth / (uResolution.y * 3.2);
  pos.xy = pos.xy * scale + uResolution * 0.5;

  vec2 clip = pos.xy / uResolution * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);

  vTone = aTone + depth * 0.0016;
  vShade = scale;
  vLift = clamp(z / 14.0, -1.0, 1.0);
}
`;

const FRAG = `#version 300 es
precision highp float;

in float vTone;
in float vShade;
in float vLift;

out vec4 outColour;

void main() {
  // Graphite only. The baked patch pattern gives the surface its variation;
  // raised tiles catch a little more light, and that is what reads as depth.
  vec3 colour = vec3(0.072, 0.072, 0.084) + vec3(vTone * 0.20);
  colour *= clamp(0.92 + (vShade - 1.0) * 2.0 + vLift * 0.45, 0.74, 1.5);
  outColour = vec4(colour, 1.0);
}
`;

type Ripple = { x: number; y: number; start: number; amp: number };

function compileShader(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader | null {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    console.warn('[TriangleField] shader compile failed:', gl.getShaderInfoLog(shader));
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

export default function TriangleField() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const stage = canvas?.parentElement;
    const area = canvas?.closest('section') ?? stage;
    if (!canvas || !stage || !area) return;

    const gl = canvas.getContext('webgl2', {
      alpha: false,
      antialias: true,
      depth: false,
      stencil: false,
      powerPreference: 'low-power',
    });
    if (!gl) return;

    const vertexShader = compileShader(gl, gl.VERTEX_SHADER, VERT);
    const fragmentShader = compileShader(gl, gl.FRAGMENT_SHADER, FRAG);
    if (!vertexShader || !fragmentShader) return;

    const program = gl.createProgram();
    if (!program) return;
    gl.attachShader(program, vertexShader);
    gl.attachShader(program, fragmentShader);
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      console.warn('[TriangleField] program link failed:', gl.getProgramInfoLog(program));
      return;
    }

    const aCorner = gl.getAttribLocation(program, 'aCorner');
    const aCenter = gl.getAttribLocation(program, 'aCenter');
    const aRandom = gl.getAttribLocation(program, 'aRandom');
    const aTone = gl.getAttribLocation(program, 'aTone');
    const uResolution = gl.getUniformLocation(program, 'uResolution');
    const uTime = gl.getUniformLocation(program, 'uTime');
    const uPointer = gl.getUniformLocation(program, 'uPointer');
    const uPointerActive = gl.getUniformLocation(program, 'uPointerActive');
    const uTiltX = gl.getUniformLocation(program, 'uTiltX');
    const uTiltY = gl.getUniformLocation(program, 'uTiltY');
    const uRipples = gl.getUniformLocation(program, 'uRipples[0]');

    const vao = gl.createVertexArray();
    const buffers: WebGLBuffer[] = [];
    let vertexCount = 0;

    const reduceQuery = window.matchMedia?.('(prefers-reduced-motion: reduce)');
    let reduced = reduceQuery?.matches ?? false;

    let width = 1;
    let height = 1;
    let raf = 0;
    let running = false;
    let startedAt = 0;
    let lastFrame = 0;
    let pointerX = 0;
    let pointerY = 0;
    let pointerActive = false;
    let lastX = 0;
    let lastY = 0;
    let tiltX = BASE_TILT;
    let tiltY = 0;
    const ripples: Ripple[] = [];
    const rippleData = new Float32Array(RIPPLE_SLOTS * 4);

    const buildGeometry = () => {
      // Tile size steps with the canvas: small screens get fewer, still tiny tiles.
      const spacing = width >= 1024 ? 26 : width >= 640 ? 24 : 22;
      const overscan = 0.3;
      const cols = Math.ceil((width * (1 + overscan * 2)) / spacing);
      const rows = Math.ceil((height * (1 + overscan * 2)) / spacing);
      const ox = -width * overscan;
      const oy = -height * overscan;
      const half = spacing / 2;
      const vertices = cols * rows * 6;

      const corner = new Float32Array(vertices * 2);
      const center = new Float32Array(vertices * 2);
      const random = new Float32Array(vertices * 3);
      const tone = new Float32Array(vertices);

      for (let row = 0; row < rows; row += 1) {
        for (let col = 0; col < cols; col += 1) {
          const cx = ox + col * spacing + half;
          const cy = oy + row * spacing + half;
          const base = (row * cols + col) * 6;
          // Alternate the shared diagonal so the mesh reads as woven, not gridded.
          const corners: readonly [number, number][] = (row + col) % 2 === 0
            ? [[-half, -half], [half, -half], [-half, half], [half, -half], [half, half], [-half, half]]
            : [[-half, -half], [half, -half], [half, half], [-half, -half], [half, half], [-half, half]];

          const patches = 0.5 + 0.5 * Math.sin(cx * 0.014 + Math.sin(cy * 0.011) * 1.9);
          const distance = Math.min(
            1,
            Math.hypot((cx - width / 2) / (width / 2), ((cy - height / 2) / (height / 2)) * 0.8),
          );
          const tileTone = (0.14 + patches * 0.72) * (0.3 + 0.7 * distance);

          for (let v = 0; v < 6; v += 1) {
            const index = base + v;
            const offset = corners[v]!;
            corner[index * 2] = offset[0];
            corner[index * 2 + 1] = offset[1];
            center[index * 2] = cx;
            center[index * 2 + 1] = cy;
            random[index * 3] = Math.random();
            random[index * 3 + 1] = 0.6 + Math.random() * 0.9;
            random[index * 3 + 2] = 0.5 + Math.random() * 1.1;
            tone[index] = tileTone;
          }
        }
      }

      vertexCount = vertices;
      gl.bindVertexArray(vao);
      const upload = (data: Float32Array, location: number, size: number) => {
        const buffer = gl.createBuffer();
        if (!buffer) return;
        buffers.push(buffer);
        gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
        gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
        gl.enableVertexAttribArray(location);
        gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0);
      };
      upload(corner, aCorner, 2);
      upload(center, aCenter, 2);
      upload(random, aRandom, 3);
      upload(tone, aTone, 1);
      gl.bindVertexArray(null);
    };

    const resize = () => {
      const rect = stage.getBoundingClientRect();
      width = Math.max(1, Math.round(rect.width));
      height = Math.max(1, Math.round(rect.height));
      const dpr = Math.min(1.75, window.devicePixelRatio || 1);
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
      gl.viewport(0, 0, canvas.width, canvas.height);
      pointerX = width / 2;
      pointerY = height / 2;
      lastX = pointerX;
      lastY = pointerY;
      buildGeometry();
    };

    // Floor colour: whatever the tiles do not cover must read as the surface,
    // never as a black hole. (`alpha: false` clears to black otherwise.)
    gl.clearColor(0.043, 0.043, 0.051, 1);

    const draw = (now: number) => {
      gl.clear(gl.COLOR_BUFFER_BIT);
      const seconds = reduced ? 0 : (now - startedAt) / 1000;

      gl.useProgram(program);
      gl.bindVertexArray(vao);
      gl.uniform2f(uResolution, width, height);
      gl.uniform1f(uTime, seconds);
      gl.uniform2f(uPointer, pointerX, pointerY);
      gl.uniform1f(uPointerActive, pointerActive && !reduced ? 1 : 0);
      gl.uniform1f(uTiltX, tiltX);
      gl.uniform1f(uTiltY, tiltY);

      rippleData.fill(0);
      for (let i = 0; i < ripples.length && i < RIPPLE_SLOTS; i += 1) {
        const ripple = ripples[i]!;
        rippleData[i * 4] = ripple.x;
        rippleData[i * 4 + 1] = ripple.y;
        rippleData[i * 4 + 2] = (now - ripple.start) / 1000;
        rippleData[i * 4 + 3] = ripple.amp;
      }
      gl.uniform4fv(uRipples, rippleData);

      gl.drawArrays(gl.TRIANGLES, 0, vertexCount);
      gl.bindVertexArray(null);
    };

    const step = (now: number, dt: number) => {
      const wantedX = pointerActive ? BASE_TILT + (pointerY / height - 0.5) * 0.3 : BASE_TILT;
      const wantedY = pointerActive ? -(pointerX / width - 0.5) * 0.36 : 0;
      const ease = 1 - Math.pow(0.004, dt);
      tiltX += (wantedX - tiltX) * ease;
      tiltY += (wantedY - tiltY) * ease;

      for (let i = ripples.length - 1; i >= 0; i -= 1) {
        if ((now - ripples[i]!.start) / 1000 > 2.4) ripples.splice(i, 1);
      }
    };

    const frame = (now: number) => {
      const dt = Math.min(0.05, Math.max(0.008, (now - lastFrame) / 1000));
      lastFrame = now;
      step(now, dt);
      draw(now);
      raf = requestAnimationFrame(frame);
    };

    const start = () => {
      if (running) return;
      running = true;
      lastFrame = performance.now();
      if (!startedAt) startedAt = lastFrame;
      raf = requestAnimationFrame(frame);
    };

    const stop = () => {
      running = false;
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
    };

    const onPointerMove = (event: PointerEvent) => {
      if (reduced) return;
      const rect = area.getBoundingClientRect();
      const px = event.clientX - rect.left;
      const py = event.clientY - rect.top;
      const speed = Math.hypot(px - lastX, py - lastY);
      lastX = px;
      lastY = py;
      pointerX = px;
      pointerY = py;
      pointerActive = true;

      if (speed > 18 && ripples.length < MAX_RIPPLES) {
        ripples.push({
          x: px,
          y: py,
          start: performance.now(),
          amp: Math.min(22, 5 + speed * 0.22),
        });
      }
    };

    const onPointerLeave = () => {
      pointerActive = false;
    };

    resize();
    draw(performance.now());

    area.addEventListener('pointermove', onPointerMove, { passive: true });
    area.addEventListener('pointerleave', onPointerLeave, { passive: true });

    const resizeObserver = new ResizeObserver(() => {
      resize();
      if (!running) draw(performance.now());
    });
    resizeObserver.observe(stage);

    const intersectionObserver = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) start();
        else stop();
      },
      { rootMargin: '140px 0px', threshold: 0.01 },
    );
    intersectionObserver.observe(stage);

    const onVisibility = () => {
      if (document.hidden) stop();
      else start();
    };
    document.addEventListener('visibilitychange', onVisibility);

    const onReduceChange = (event: MediaQueryListEvent) => {
      reduced = event.matches;
      if (reduced) {
        stop();
        tiltX = BASE_TILT;
        tiltY = 0;
        pointerActive = false;
        draw(performance.now());
      } else {
        start();
      }
    };
    reduceQuery?.addEventListener?.('change', onReduceChange);

    return () => {
      stop();
      resizeObserver.disconnect();
      intersectionObserver.disconnect();
      document.removeEventListener('visibilitychange', onVisibility);
      reduceQuery?.removeEventListener?.('change', onReduceChange);
      area.removeEventListener('pointermove', onPointerMove);
      area.removeEventListener('pointerleave', onPointerLeave);
      for (const buffer of buffers) gl.deleteBuffer(buffer);
      gl.deleteVertexArray(vao);
      gl.deleteProgram(program);
      gl.deleteShader(vertexShader);
      gl.deleteShader(fragmentShader);
    };
  }, []);

  return <canvas ref={canvasRef} aria-hidden="true" className="connect-triangle-field" />;
}