import { localAssetUrl } from '../catalog';
export interface SDKModel {
  addMotion(key: string, data: ArrayBuffer, fadeIn?: number, fadeOut?: number): boolean;
  addExpression(name: string, data: ArrayBuffer): void;
  physics(data: ArrayBuffer): void;
  pose(data: ArrayBuffer): void;
  texture(index: number, texture: WebGLTexture): void;
  start(key: string): boolean;
  finished(): boolean;
  stop(): void;
  expression(name: string | undefined): void;
  frame(
    dt: number,
    input: {
      gaze: { x: number; y: number };
      blink: boolean;
      idle: boolean;
      time: number;
      speaking: boolean;
      level: number;
      closeMouth: boolean;
      sx: number;
      sy: number;
      tx: number;
      ty: number;
      target?: { framebuffer: WebGLFramebuffer; width: number; height: number };
    },
  ): void;
  dimensions(): { width: number; height: number };
  resize(width: number, height: number): void;
  dispose(): void;
  hit(x: number, y: number): string | null;
}
export interface SDK {
  createModel(
    gl: WebGLRenderingContext,
    width: number,
    height: number,
    json: ArrayBuffer,
    moc: ArrayBuffer,
  ): SDKModel;
}
let runtime: Promise<SDK> | null = null;
export function loadRuntime(): Promise<SDK> {
  if (runtime) return runtime;
  runtime = (async () => {
    const url = localAssetUrl('runtime/live2dcubismcore.min.js');
    if (!('Live2DCubismCore' in window))
      await new Promise<void>((resolve, reject) => {
        const script = document.createElement('script');
        script.src = url;
        const failed = () => {
          clearTimeout(timer);
          script.remove();
          reject(new Error('Cubism Core 未安装，请按 Live2D 文档准备本地 runtime'));
        };
        const timer = setTimeout(failed, 15000);
        script.onload = () => {
          clearTimeout(timer);
          resolve();
        };
        script.onerror = failed;
        document.head.append(script);
      });
    const moduleUrl = localAssetUrl('runtime/bridge.js');
    return (await import(/* @vite-ignore */ moduleUrl)) as SDK;
  })().catch((e) => {
    runtime = null;
    throw e;
  });
  return runtime;
}
