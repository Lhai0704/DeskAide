import { safeAssetPath } from '../manifest';
import type { Live2DAvatarPackManifest, AvatarPreferences, SemanticState } from '../types';
import { loadRuntime, type SDKModel } from './runtime';
import { MotionController } from './motion';
import { Gaze } from './gaze';
export interface RendererInput {
  state: SemanticState;
  speakingLevel: number;
  interaction: number;
  cursorFocus: { x: number; y: number } | null;
  preferences: AvatarPreferences;
}
interface ModelJSON {
  FileReferences: {
    Moc: string;
    Textures: string[];
    Physics?: string;
    Pose?: string;
    Expressions?: { Name: string; File: string }[];
    Motions?: Record<
      string,
      { File: string; Sound?: string; FadeInTime?: number; FadeOutTime?: number }[]
    >;
  };
}
export function validateModelReferences(value: unknown): asserts value is ModelJSON {
  if (!value || typeof value !== 'object' || !('FileReferences' in value))
    throw new Error('model3.json 无效');
  const refs = (value as ModelJSON).FileReferences;
  if (!refs || !Array.isArray(refs.Textures) || !refs.Textures.length)
    throw new Error('模型缺少贴图');
  safeAssetPath(refs.Moc);
  refs.Textures.forEach(safeAssetPath);
  const walk = (v: unknown, key = '') => {
    if (
      typeof v === 'string' &&
      ['File', 'Sound', 'Physics', 'Pose', 'DisplayInfo', 'UserData'].includes(key)
    )
      safeAssetPath(v);
    else if (v && typeof v === 'object') for (const [k, x] of Object.entries(v)) walk(x, k);
  };
  walk(refs);
}
export class Live2DRenderer {
  private model: SDKModel | null = null;
  private textures: WebGLTexture[] = [];
  private abort = new AbortController();
  private motion: MotionController | null = null;
  private gaze = new Gaze();
  private frame = 0;
  private last = 0;
  private alive = true;
  private paused = false;
  private input: RendererInput;
  private gl: WebGLRenderingContext;
  private observer: ResizeObserver;
  private previousInteraction = 0;
  private mouthWasActive = false;
  private tapExpression: string | undefined;
  constructor(
    private canvas: HTMLCanvasElement,
    private pack: Live2DAvatarPackManifest,
    private root: string,
    input: RendererInput,
    private fail: (e: Error) => void,
  ) {
    this.input = input;
    const gl = canvas.getContext('webgl2', {
      alpha: true,
      premultipliedAlpha: true,
      preserveDrawingBuffer: false,
    });
    if (!gl) throw new Error('WebGL 不可用');
    this.gl = gl;
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(canvas);
    this.resize();
  }
  async load() {
    const signal = this.abort.signal;
    const read = async (url: string) => {
      const r = await fetch(url, { signal });
      if (!r.ok) throw new Error(`模型资源加载失败：${r.status}`);
      return r.arrayBuffer();
    };
    const json = await read(`${this.root}/${this.pack.model}`);
    const data: unknown = JSON.parse(new TextDecoder().decode(json));
    validateModelReferences(data);
    const base = `${this.root}/${this.pack.model.slice(0, this.pack.model.lastIndexOf('/') + 1)}`;
    const sdk = await loadRuntime();
    const moc = await read(base + data.FileReferences.Moc);
    signal.throwIfAborted();
    this.model = sdk.createModel(this.gl, this.canvas.width, this.canvas.height, json, moc);
    const model = this.model;
    for (const [index, path] of data.FileReferences.Textures.entries()) {
      const bytes = await read(base + path);
      const bitmap = await createImageBitmap(new Blob([bytes]));
      if (signal.aborted) {
        bitmap.close();
        signal.throwIfAborted();
      }
      const texture = this.gl.createTexture();
      if (!texture) {
        bitmap.close();
        throw new Error('无法创建贴图');
      }
      this.textures.push(texture);
      this.gl.bindTexture(this.gl.TEXTURE_2D, texture);
      this.gl.pixelStorei(this.gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, 0);
      this.gl.texImage2D(
        this.gl.TEXTURE_2D,
        0,
        this.gl.RGBA,
        this.gl.RGBA,
        this.gl.UNSIGNED_BYTE,
        bitmap,
      );
      bitmap.close();
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_MIN_FILTER, this.gl.LINEAR);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_MAG_FILTER, this.gl.LINEAR);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_WRAP_S, this.gl.CLAMP_TO_EDGE);
      this.gl.texParameteri(this.gl.TEXTURE_2D, this.gl.TEXTURE_WRAP_T, this.gl.CLAMP_TO_EDGE);
      model.texture(index, texture);
    }
    const optional = async (path: string, load: (b: ArrayBuffer) => void) => {
      try {
        const b = await read(base + path);
        signal.throwIfAborted();
        load(b);
      } catch (e) {
        if (signal.aborted) throw e;
      }
    };
    for (const [group, motions] of Object.entries(data.FileReferences.Motions ?? {}))
      for (const [index, m] of motions.entries())
        await optional(m.File, (b) => {
          model.addMotion(`${group}:${index}`, b, m.FadeInTime, m.FadeOutTime);
        });
    for (const e of data.FileReferences.Expressions ?? [])
      await optional(e.File, (b) => model.addExpression(e.Name, b));
    if (data.FileReferences.Physics)
      await optional(data.FileReferences.Physics, (b) => model.physics(b));
    if (data.FileReferences.Pose) await optional(data.FileReferences.Pose, (b) => model.pose(b));
    const idle = data.FileReferences.Motions?.Idle?.length
      ? { group: 'Idle', index: 0 }
      : undefined;
    this.motion = new MotionController(model, { idle, ...this.pack.motions });
    signal.throwIfAborted();
    this.resize();
    this.schedule();
  }
  update(input: RendererInput) {
    const changed = JSON.stringify(input) !== JSON.stringify(this.input);
    this.input = input;
    if (!this.paused && changed) this.schedule();
  }
  pause(value: boolean) {
    this.paused = value;
    if (value) {
      cancelAnimationFrame(this.frame);
      this.frame = 0;
      this.last = 0;
    } else this.schedule();
  }
  resize() {
    const r = this.canvas.getBoundingClientRect();
    const dpr = Math.min(2, window.devicePixelRatio || 1);
    this.canvas.width = Math.max(1, Math.round(r.width * dpr));
    this.canvas.height = Math.max(1, Math.round(r.height * dpr));
    this.model?.resize(this.canvas.width, this.canvas.height);
    if (this.model) this.schedule();
  }
  private schedule() {
    if (this.alive && !this.paused && !this.frame)
      this.frame = requestAnimationFrame((t) => this.tick(t));
  }
  private tick(now: number) {
    this.frame = 0;
    if (!this.alive || this.paused || !this.model) return;
    if (now - this.last < 1000 / 30) {
      this.schedule();
      return;
    }
    const dt = this.last ? Math.min(0.1, (now - this.last) / 1000) : 1 / 30;
    this.last = now;
    try {
      const i = this.input,
        p = i.preferences;
      const w = this.canvas.clientWidth,
        h = this.canvas.clientHeight;
      const dimensions = this.model.dimensions();
      const ratio = dimensions.width / dimensions.height;
      const scale = (this.pack.layout?.scale ?? 1) * p.scale;
      const fittedHeight = Math.min(h, w / ratio) * scale;
      const fittedWidth = fittedHeight * ratio;
      const anchor = this.pack.layout?.anchor ?? { x: 0.5, y: 0.5 };
      const position = this.pack.layout?.position ?? { x: 0.5, y: 0.5 };
      const center = {
        x: w * position.x + (0.5 - anchor.x) * fittedWidth,
        y: h * (position.y + p.verticalPosition) + (0.5 - anchor.y) * fittedHeight,
      };
      const gaze = this.gaze.update(
        p.mouseTracking ? i.cursorFocus : null,
        { ...center, width: fittedWidth, height: fittedHeight },
        now,
        dt,
        p.idleAnimation,
      );
      if (i.interaction && i.interaction !== this.previousInteraction) {
        this.previousInteraction = i.interaction;
        const point = i.cursorFocus;
        const area = point
          ? this.model.hit(
              ((point.x - center.x) * 2) / fittedHeight,
              ((center.y - point.y) * 2) / fittedHeight,
            )
          : null;
        const tap = area ? this.pack.taps?.[area] : undefined;
        this.tapExpression = tap?.expression ?? this.pack.expressions?.tap;
        if (p.motions)
          this.motion?.oneShot(tap?.motion ?? this.pack.motions?.activated, i.interaction);
      }
      this.motion?.update(i.state, p.motions, p.idleAnimation, dt);
      this.model.expression(
        i.interaction
          ? this.tapExpression
          : i.state === 'thinking'
            ? this.pack.expressions?.thinking
            : this.pack.expressions?.neutral,
      );
      this.gl.viewport(0, 0, this.canvas.width, this.canvas.height);
      this.gl.clearColor(0, 0, 0, 0);
      this.gl.clear(this.gl.COLOR_BUFFER_BIT);
      this.model.frame(dt, {
        gaze,
        blink: p.autoBlink && this.pack.behavior?.blink !== 'model',
        idle: p.idleAnimation,
        time: now / 1000,
        speaking: i.state === 'speaking',
        level: i.speakingLevel,
        closeMouth: this.mouthWasActive,
        sx: fittedHeight / w,
        sy: fittedHeight / h,
        tx: ((center.x / w) * 2 - 1) / (fittedHeight / w),
        ty: (1 - (center.y / h) * 2) / (fittedHeight / h),
      });
      this.mouthWasActive = i.state === 'speaking';
    } catch (e) {
      this.pause(true);
      this.fail(e instanceof Error ? e : new Error(String(e)));
      return;
    }
    const p = this.input.preferences;
    if (
      p.idleAnimation ||
      p.autoBlink ||
      p.motions ||
      p.mouseTracking ||
      this.input.state === 'speaking'
    )
      this.schedule();
  }
  dispose(releaseContext = true) {
    if (!this.alive) return;
    this.alive = false;
    this.abort.abort();
    cancelAnimationFrame(this.frame);
    this.frame = 0;
    this.observer.disconnect();
    this.motion?.cancel();
    this.model?.dispose();
    this.model = null;
    for (const t of this.textures) this.gl.deleteTexture(t);
    this.textures = [];
    if (releaseContext) this.gl.getExtension('WEBGL_lose_context')?.loseContext();
  }
}
