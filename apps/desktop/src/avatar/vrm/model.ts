import {
  AmbientLight,
  AnimationMixer,
  Box3,
  DirectionalLight,
  LoopOnce,
  LoopRepeat,
  LoadingManager,
  Mesh,
  Object3D,
  PerspectiveCamera,
  Quaternion,
  Scene,
  SRGBColorSpace,
  Vector3,
  WebGLRenderer,
  type AnimationAction,
  type AnimationClip,
} from 'three';
import { GLTFLoader, type GLTF } from 'three/addons/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';
import { VRMHumanBoneName, VRMLoaderPlugin, VRMUtils, type VRM } from '@pixiv/three-vrm';
import {
  createVRMAnimationClip,
  VRMAnimationLoaderPlugin,
  type VRMAnimation,
} from '@pixiv/three-vrm-animation';
import type { RendererInput, SemanticState, VrmAvatarPackManifest } from '../types';
import { frameDue } from '../live2d/frame';
import { Gaze } from '../live2d/gaze';
import { clearHitMask, publishHitMask, sameHitMask } from '../live2d/passthrough';
import { reAnchorRootPositionTrack } from './anchor';
import { VrmBlink } from './blink';
import { selectVrmClip, semanticExpression } from './clips';
import { type VrmBounds, vrmCameraFrame, vrmPixelRatio } from './frame';
import { fillCapsuleHitMask, type HitSegment } from './hit';
import { MouthDriver } from './lips';

const LINKS = [
  [VRMHumanBoneName.Head, VRMHumanBoneName.Neck],
  [VRMHumanBoneName.Neck, VRMHumanBoneName.UpperChest],
  [VRMHumanBoneName.Neck, VRMHumanBoneName.Chest],
  [VRMHumanBoneName.UpperChest, VRMHumanBoneName.Chest],
  [VRMHumanBoneName.Chest, VRMHumanBoneName.Spine],
  [VRMHumanBoneName.Spine, VRMHumanBoneName.Hips],
  [VRMHumanBoneName.Chest, VRMHumanBoneName.LeftUpperArm],
  [VRMHumanBoneName.LeftShoulder, VRMHumanBoneName.LeftUpperArm],
  [VRMHumanBoneName.LeftUpperArm, VRMHumanBoneName.LeftLowerArm],
  [VRMHumanBoneName.LeftLowerArm, VRMHumanBoneName.LeftHand],
  [VRMHumanBoneName.Chest, VRMHumanBoneName.RightUpperArm],
  [VRMHumanBoneName.RightShoulder, VRMHumanBoneName.RightUpperArm],
  [VRMHumanBoneName.RightUpperArm, VRMHumanBoneName.RightLowerArm],
  [VRMHumanBoneName.RightLowerArm, VRMHumanBoneName.RightHand],
  [VRMHumanBoneName.Hips, VRMHumanBoneName.LeftUpperLeg],
  [VRMHumanBoneName.LeftUpperLeg, VRMHumanBoneName.LeftLowerLeg],
  [VRMHumanBoneName.LeftLowerLeg, VRMHumanBoneName.LeftFoot],
  [VRMHumanBoneName.Hips, VRMHumanBoneName.RightUpperLeg],
  [VRMHumanBoneName.RightUpperLeg, VRMHumanBoneName.RightLowerLeg],
  [VRMHumanBoneName.RightLowerLeg, VRMHumanBoneName.RightFoot],
] as const;

const TORSO = new Set<string>([
  VRMHumanBoneName.Neck,
  VRMHumanBoneName.UpperChest,
  VRMHumanBoneName.Chest,
  VRMHumanBoneName.Spine,
  VRMHumanBoneName.Hips,
]);

function boneRadius(name: string, body: number) {
  if (name === VRMHumanBoneName.Head) return Math.max(18, body * 0.22);
  if (TORSO.has(name)) return Math.max(14, body * 0.16);
  return Math.max(10, body * 0.09);
}

function modelBounds(root: Object3D): VrmBounds | null {
  const box = new Box3();
  const child = new Box3();
  root.updateMatrixWorld(true);
  root.traverse((object) => {
    const mesh = object as Mesh;
    if (!mesh.isMesh || !mesh.visible || !mesh.geometry) return;
    if (mesh.name.startsWith('VRMC_springBone_collider')) return;
    if (!mesh.geometry.boundingBox) mesh.geometry.computeBoundingBox();
    const geometryBox = mesh.geometry.boundingBox;
    if (!geometryBox) return;
    child.copy(geometryBox).applyMatrix4(mesh.matrixWorld);
    box.union(child);
  });
  if (box.isEmpty()) return null;
  return {
    minX: box.min.x,
    maxX: box.max.x,
    minY: box.min.y,
    maxY: box.max.y,
    minZ: box.min.z,
    maxZ: box.max.z,
  };
}

export class VrmRenderer {
  private readonly renderer: WebGLRenderer;
  private readonly scene = new Scene();
  private readonly camera: PerspectiveCamera;
  private readonly gaze = new Gaze();
  private readonly blink = new VrmBlink();
  private readonly mouth = new MouthDriver();
  private readonly lookTarget = new Object3D();
  private readonly world = new Vector3();
  private readonly forward = new Vector3();
  private readonly offset = new Vector3();
  private readonly ndc = new Vector3();
  private readonly breathRest = new Quaternion();
  private mixer: AnimationMixer | null = null;
  private vrm: VRM | null = null;
  private bounds: VrmBounds | null = null;
  private breathBone: Object3D | null = null;
  private clips = new Map<SemanticState, AnimationClip>();
  private current: AnimationAction | null = null;
  private currentName: SemanticState | null = null;
  private oneShot = false;
  private consumedInteraction = 0;
  private consumedError = false;
  private shownExpression: string | null = null;
  private lookEnabled = false;
  private abort = new AbortController();
  private observer: ResizeObserver;
  private input: RendererInput;
  private frame = 0;
  private last = 0;
  private alive = true;
  private paused = false;
  private lastHitAt = 0;
  private hitDirty = true;
  private lastHitMask: Uint8Array | null = null;

  constructor(
    private canvas: HTMLCanvasElement,
    private pack: VrmAvatarPackManifest,
    private root: string,
    input: RendererInput,
    private fail: (error: Error) => void,
  ) {
    this.input = input;
    const renderer = new WebGLRenderer({
      canvas,
      alpha: true,
      antialias: false,
      premultipliedAlpha: true,
      powerPreference: 'high-performance',
    });
    renderer.outputColorSpace = SRGBColorSpace;
    renderer.setClearColor(0x000000, 0);
    this.renderer = renderer;
    this.camera = new PerspectiveCamera(30, 1, 0.05, 50);
    this.camera.add(this.lookTarget);
    this.scene.add(this.camera);
    const key = new DirectionalLight(0xffffff, 1.15);
    key.position.set(0.45, 1.6, 1.2);
    const fill = new DirectionalLight(0xffffff, 0.35);
    fill.position.set(-0.8, 0.7, 0.5);
    this.scene.add(key, fill, new AmbientLight(0xffffff, 0.55));
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(canvas);
    this.resize();
  }

  async load() {
    const signal = this.abort.signal;
    const modelUrl = `${this.root}/${this.pack.model}`;
    const bytes = await readBuffer(modelUrl, signal);
    const gltf = await this.parse(bytes, directoryUrl(modelUrl));
    signal.throwIfAborted();
    const vrm = gltf.userData.vrm as VRM | undefined;
    if (!vrm) throw new Error('文件不是 VRM 模型');
    if (!this.alive) {
      VRMUtils.deepDispose(vrm.scene);
      return;
    }
    try {
      this.mount(vrm, gltf.scene);
      await this.loadMotions(vrm, signal);
      signal.throwIfAborted();
    } catch (error) {
      if (this.alive && this.vrm !== vrm) {
        this.scene.remove(vrm.scene);
        VRMUtils.deepDispose(vrm.scene);
      }
      throw error;
    }
    this.resize();
    this.schedule();
  }

  update(input: RendererInput) {
    if (
      input.preferences.scale !== this.input.preferences.scale ||
      input.preferences.verticalPosition !== this.input.preferences.verticalPosition
    )
      this.hitDirty = true;
    this.input = input;
    if (!this.paused) this.schedule();
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
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(1, rect.width);
    const height = Math.max(1, rect.height);
    this.renderer.setPixelRatio(vrmPixelRatio(window.devicePixelRatio));
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.hitDirty = true;
    if (this.vrm) {
      this.frameCamera();
      this.schedule();
    }
  }

  dispose(releaseContext = true) {
    if (!this.alive) return;
    this.alive = false;
    this.lastHitMask = null;
    clearHitMask();
    this.abort.abort();
    cancelAnimationFrame(this.frame);
    this.frame = 0;
    this.observer.disconnect();
    this.mixer?.stopAllAction();
    if (this.vrm) {
      this.scene.remove(this.vrm.scene);
      VRMUtils.deepDispose(this.vrm.scene);
      this.vrm = null;
    }
    this.renderer.dispose();
    if (releaseContext) {
      try {
        this.renderer.forceContextLoss();
      } catch {
        // The context may already be gone.
      }
    }
  }

  private mount(vrm: VRM, scene: Object3D) {
    VRMUtils.removeUnnecessaryVertices(scene);
    VRMUtils.combineSkeletons(scene);
    VRMUtils.combineMorphs(vrm);
    scene.traverse((object) => {
      object.frustumCulled = false;
    });
    // VRM 1 already faces +Z. VRM 0 faces -Z until this turns it toward the camera.
    VRMUtils.rotateVRM0(vrm);
    this.scene.add(vrm.scene);
    vrm.scene.updateMatrixWorld(true);
    vrm.springBoneManager?.reset();
    vrm.update(0);
    const bounds = modelBounds(vrm.scene);
    if (!bounds) throw new Error('VRM 模型没有可见网格');
    this.vrm = vrm;
    this.bounds = bounds;
    // Normalized VRM bones use a T-pose. Give unanimated arms a relaxed rest
    // pose before the mixer captures its original values for cross-fades.
    vrm.humanoid.getNormalizedBoneNode(VRMHumanBoneName.LeftUpperArm)?.rotation.set(0, 0, 1.2);
    vrm.humanoid.getNormalizedBoneNode(VRMHumanBoneName.RightUpperArm)?.rotation.set(0, 0, -1.2);
    vrm.update(0);
    this.breathBone =
      vrm.humanoid?.getNormalizedBoneNode(VRMHumanBoneName.Chest) ??
      vrm.humanoid?.getNormalizedBoneNode(VRMHumanBoneName.Spine) ??
      null;
    if (this.breathBone) this.breathRest.copy(this.breathBone.quaternion);
    this.mixer = new AnimationMixer(vrm.scene);
    this.mixer.addEventListener('finished', (event) => {
      if (event.action !== this.current) return;
      this.current = null;
      this.currentName = null;
      this.oneShot = false;
    });
    this.frameCamera();
  }

  private async loadMotions(vrm: VRM, signal: AbortSignal) {
    const hips = vrm.humanoid?.getNormalizedBoneNode(VRMHumanBoneName.Hips);
    for (const [state, path] of Object.entries(this.pack.motions ?? {})) {
      if (!path) continue;
      try {
        const url = `${this.root}/${path}`;
        const gltf = await this.parse(await readBuffer(url, signal), directoryUrl(url));
        const animation = (gltf.userData.vrmAnimations as VRMAnimation[] | undefined)?.[0];
        if (!animation) throw new Error('文件不含 VRM 动画');
        const clip = createVRMAnimationClip(animation, vrm);
        if (hips) reAnchorRootPositionTrack(clip, hips.name, hips.position);
        this.clips.set(state as SemanticState, clip);
      } catch (error) {
        if (signal.aborted) throw error;
        throw new Error(`VRM 动作加载失败（${state}: ${path}）：${String(error)}`, {
          cause: error,
        });
      }
    }
  }

  private async parse(buffer: ArrayBuffer, base: string) {
    // GLTFLoader tolerates failed textures and resolves with blank materials.
    // Reject incomplete models instead of presenting them as successfully loaded.
    const manager = new LoadingManager();
    let resourceFailed = false;
    manager.onError = () => {
      resourceFailed = true;
    };
    const loader = new GLTFLoader(manager);
    loader.setMeshoptDecoder(MeshoptDecoder);
    loader.register((parser) => new VRMLoaderPlugin(parser));
    loader.register((parser) => new VRMAnimationLoaderPlugin(parser));
    const gltf = await new Promise<GLTF>((resolve, reject) => {
      loader.parse(buffer, base, resolve, (error) => {
        reject(error instanceof Error ? error : new Error('VRM 解析失败'));
      });
    });
    if (resourceFailed) {
      VRMUtils.deepDispose(gltf.scene);
      throw new Error('VRM 贴图或依赖资源加载失败，请检查资源文件和窗口安全策略');
    }
    return gltf;
  }

  private frameCamera() {
    if (!this.bounds) return;
    const rect = this.canvas.getBoundingClientRect();
    const frame = vrmCameraFrame(
      this.bounds,
      { width: rect.width, height: rect.height },
      this.pack.layout?.scale ?? 1,
      this.input.preferences,
    );
    if (!frame) return;
    this.camera.fov = frame.fov;
    this.camera.position.set(frame.position.x, frame.position.y, frame.position.z);
    this.camera.lookAt(frame.target.x, frame.target.y, frame.target.z);
    this.camera.near = Math.max(0.01, frame.distance / 100);
    this.camera.far = Math.max(10, frame.distance * 20);
    this.camera.updateProjectionMatrix();
    this.hitDirty = true;
  }

  private schedule() {
    if (this.alive && !this.paused && !this.frame)
      this.frame = requestAnimationFrame((now) => this.tick(now));
  }

  private tick(now: number) {
    this.frame = 0;
    if (!this.alive || this.paused || !this.vrm) return;
    if (!frameDue(now, this.last)) {
      this.schedule();
      return;
    }
    const dt = this.last ? Math.min(0.1, (now - this.last) / 1000) : 1 / 60;
    this.last = now;
    try {
      const input = this.input;
      const preferences = input.preferences;
      this.mixer?.update(dt);
      this.applyClip(input);
      this.applyBreath(preferences.idleAnimation && !this.current, now / 1000);
      this.applyExpression(
        semanticExpression(input.state, input.interaction, this.pack.expressions),
      );
      this.vrm.expressionManager?.setValue('blink', this.blink.update(dt, preferences.autoBlink));
      const visemes = this.mouth.update(
        input.speakingLevel,
        input.state === 'speaking',
        dt,
        now / 1000,
      );
      if (visemes) {
        for (const [name, weight] of Object.entries(visemes))
          this.vrm.expressionManager?.setValue(name, weight);
      }
      this.applyGaze(preferences, now, dt);
      this.vrm.update(dt);
      this.renderer.render(this.scene, this.camera);
      this.maybePublishHit(now);
    } catch (error) {
      this.pause(true);
      this.fail(error instanceof Error ? error : new Error(String(error)));
      return;
    }
    this.schedule();
  }

  private applyClip(input: RendererInput) {
    const firstActivation =
      input.state === 'activated' &&
      input.interaction !== 0 &&
      input.interaction !== this.consumedInteraction;
    if (firstActivation) this.consumedInteraction = input.interaction;
    const startError = input.state === 'error' && !this.consumedError;
    this.consumedError = input.state === 'error';
    const suppress = firstActivation
      ? null
      : input.state === 'activated' && input.interaction === this.consumedInteraction
        ? 'activated'
        : input.state === 'error' && !startError
          ? 'error'
          : null;
    const choice = selectVrmClip(
      input.state,
      input.preferences.motions,
      input.preferences.idleAnimation,
      new Set(this.clips.keys()),
      suppress,
    );
    if (this.oneShot && this.current?.isRunning() && (choice === null || choice.name === 'idle'))
      return;
    this.play(choice);
  }

  private play(choice: { name: SemanticState; loop: boolean } | null) {
    if ((choice?.name ?? null) === this.currentName) return;
    const fade = 0.4;
    if (!choice || !this.mixer) {
      this.current?.fadeOut(fade);
      this.current = null;
      this.currentName = null;
      this.oneShot = false;
      return;
    }
    const clip = this.clips.get(choice.name);
    if (!clip) return;
    const action = this.mixer.clipAction(clip);
    action.reset();
    action.setLoop(choice.loop ? LoopRepeat : LoopOnce, choice.loop ? Infinity : 1);
    action.clampWhenFinished = !choice.loop;
    action.enabled = true;
    action.fadeIn(fade).play();
    this.current?.fadeOut(fade);
    this.current = action;
    this.currentName = choice.name;
    this.oneShot = !choice.loop;
  }

  private applyBreath(active: boolean, time: number) {
    const bone = this.breathBone;
    if (!bone || !active) return;
    bone.quaternion.copy(this.breathRest);
    bone.rotateX(Math.sin(time * 1.5) * 0.018);
  }

  private applyExpression(name: string | undefined) {
    const manager = this.vrm?.expressionManager;
    if (!manager) return;
    if (this.shownExpression && this.shownExpression !== name)
      manager.setValue(this.shownExpression, 0);
    if (name && manager.getExpression(name)) {
      manager.setValue(name, 1);
      this.shownExpression = name;
    } else this.shownExpression = null;
  }

  private applyGaze(preferences: RendererInput['preferences'], now: number, dt: number) {
    const lookAt = this.vrm?.lookAt;
    if (!lookAt) return;
    const width = this.canvas.clientWidth;
    const height = this.canvas.clientHeight;
    const sample = this.gaze.update(
      preferences.mouseTracking ? this.input.cursorFocus : null,
      { x: width / 2, y: height / 2, width, height },
      now,
      dt,
      preferences.idleAnimation,
    );
    const enabled = preferences.mouseTracking || preferences.idleAnimation;
    if (!enabled) {
      if (this.lookEnabled) {
        lookAt.target = null;
        lookAt.reset();
        this.lookEnabled = false;
      }
      return;
    }
    this.lookTarget.position.set(sample.x * 1.6, sample.y * 1.1, -0.35);
    lookAt.target = this.lookTarget;
    this.lookEnabled = true;
  }

  private maybePublishHit(now: number) {
    if (!this.hitDirty && now - this.lastHitAt < 200) return;
    this.hitDirty = false;
    this.lastHitAt = now;
    const segments = this.hitSegments();
    const bits = segments
      ? fillCapsuleHitMask({ width: window.innerWidth, height: window.innerHeight }, segments)
      : null;
    if (!bits || (this.lastHitMask && sameHitMask(this.lastHitMask, bits))) return;
    this.lastHitMask = bits;
    void publishHitMask(bits).catch(() => {
      if (this.lastHitMask === bits) this.lastHitMask = null;
    });
  }

  private hitSegments(): HitSegment[] | null {
    const vrm = this.vrm;
    if (!vrm?.humanoid) return null;
    const points = new Map<string, { x: number; y: number }>();
    for (const name of new Set(LINKS.flat())) {
      const node = vrm.humanoid.getNormalizedBoneNode(name);
      if (!node) continue;
      const point = this.project(node);
      if (point) points.set(name, point);
    }
    const head = points.get(VRMHumanBoneName.Head);
    const hips = points.get(VRMHumanBoneName.Hips);
    const body =
      head && hips
        ? Math.hypot(head.x - hips.x, head.y - hips.y)
        : Math.min(window.innerHeight, 220);
    const segments: HitSegment[] = [];
    for (const [from, to] of LINKS) {
      const a = points.get(from);
      const b = points.get(to);
      if (!a || !b) continue;
      segments.push({
        ax: a.x,
        ay: a.y,
        bx: b.x,
        by: b.y,
        radius: Math.max(boneRadius(from, body), boneRadius(to, body)),
      });
    }
    return segments.length ? segments : null;
  }

  private project(node: Object3D) {
    node.getWorldPosition(this.world);
    this.camera.getWorldDirection(this.forward);
    this.offset.copy(this.world).sub(this.camera.position);
    if (this.forward.dot(this.offset) <= 0.05) return null;
    this.ndc.copy(this.world).project(this.camera);
    const rect = this.canvas.getBoundingClientRect();
    return {
      x: rect.left + (this.ndc.x * 0.5 + 0.5) * rect.width,
      y: rect.top + (this.ndc.y * -0.5 + 0.5) * rect.height,
    };
  }
}

function directoryUrl(url: string) {
  const slash = url.lastIndexOf('/') + 1;
  return slash > 0 ? url.slice(0, slash) : url;
}

async function readBuffer(url: string, signal: AbortSignal) {
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`VRM 资源加载失败：${response.status}`);
  return response.arrayBuffer();
}
