// DeskAide integration code. Official SDK sources are resolved only by prepare-live2d.
import { CubismFramework } from "@framework/live2dcubismframework";
import { CubismUserModel } from "@framework/model/cubismusermodel";
import { CubismModelSettingJson } from "@framework/cubismmodelsettingjson";
import { CubismEyeBlink } from "@framework/effect/cubismeyeblink";
import { CubismMatrix44 } from "@framework/math/cubismmatrix44";
import { ACubismMotion } from "@framework/motion/acubismmotion";

let users = 0;
export function createModel(gl, width, height, json, moc) {
  if (!users) {
    CubismFramework.startUp();
    CubismFramework.initialize();
  }
  users++;
  try {
    return new Model(gl, width, height, json, moc);
  } catch (e) {
    if (!--users) {
      CubismFramework.dispose();
      CubismFramework.cleanUp();
    }
    throw e;
  }
}
class Model extends CubismUserModel {
  constructor(gl, width, height, json, moc) {
    super();
    this.gl = gl;
    this.dead = false;
    this.motions = new Map();
    this.expressions = new Map();
    try {
      this.settings = new CubismModelSettingJson(json, json.byteLength);
      this.loadModel(moc, true);
      if (!this._model) throw new Error("无法加载 moc3");
      const layout = new Map();
      if (this.settings.getLayoutMap(layout))
        this._modelMatrix.setupFromLayout(layout);
      this.createRenderer(width, height);
      const renderer = this.getRenderer();
      // SDK 5 blends premultiplied and rejects the straight-alpha path.
      renderer.setIsPremultipliedAlpha(true);
      // Redraw each clip into the whole mask so thin lines are not packed into a corner.
      renderer.useHighPrecisionMask(true);
      // High-precision mode redraws/clears this target for every clipped mesh.
      // 2048 preserves detailed masks without repeatedly clearing a 4096² target.
      const mask = Math.min(2048, gl.getParameter(gl.MAX_TEXTURE_SIZE) || 2048);
      if (renderer.getClippingMaskBufferSize() > 0)
        renderer.setClippingMaskBufferSize(mask);
      renderer.startUp(gl);
      const aniso =
        gl.getExtension("EXT_texture_filter_anisotropic") ||
        gl.getExtension("WEBKIT_EXT_texture_filter_anisotropic");
      if (aniso)
        renderer.setAnisotropy(
          Math.min(
            8,
            gl.getParameter(aniso.MAX_TEXTURE_MAX_ANISOTROPY_EXT) || 1,
          ),
        );
      this.shaderPath = new URL("./shaders/", import.meta.url).href;
      renderer.loadShaders(this.shaderPath);
      this._eyeBlink = CubismEyeBlink.create(this.settings);
      if (!this.settings.getEyeBlinkParameterCount()) {
        const ids = this._model.getModel().parameters.ids;
        this._eyeBlink.setParameterIds(
          ["ParamEyeLOpen", "ParamEyeROpen"]
            .filter((id) => ids.includes(id))
            .map((id) => CubismFramework.getIdManager().getId(id)),
        );
      }
      this.eyeIds = new Set();
      for (let i = 0; i < this.settings.getEyeBlinkParameterCount(); i++)
        this.eyeIds.add(this.settings.getEyeBlinkParameterId(i).getString().s);
      for (const id of ["ParamEyeLOpen", "ParamEyeROpen"]) this.eyeIds.add(id);
      this._model.saveParameters();
    } catch (e) {
      this.release();
      throw e;
    }
  }
  addMotion(key, buffer, fadeIn, fadeOut) {
    const m = this.loadMotion(buffer, buffer.byteLength, key);
    if (!m) throw new Error("motion 无效");
    // The presentation controller loops idle and semantic motions explicitly.
    m.setLoop(false);
    if (Number.isFinite(fadeIn) && fadeIn >= 0)
      m.setFadeInTime(Math.min(10, fadeIn));
    if (Number.isFinite(fadeOut) && fadeOut >= 0)
      m.setFadeOutTime(Math.min(10, fadeOut));
    const eyes = [],
      lips = [];
    for (let i = 0; i < this.settings.getEyeBlinkParameterCount(); i++)
      eyes.push(this.settings.getEyeBlinkParameterId(i));
    for (let i = 0; i < this.settings.getLipSyncParameterCount(); i++)
      lips.push(this.settings.getLipSyncParameterId(i));
    m.setEffectIds(eyes, lips);
    const data = JSON.parse(new TextDecoder().decode(buffer));
    const ownsEyes = (data.Curves ?? []).some(
      (c) => this.eyeIds.has(c.Id) || c.Id === "EyeBlink",
    );
    this.motions.set(key, { motion: m, ownsEyes });
    return true;
  }
  addExpression(name, buffer) {
    const m = this.loadExpression(buffer, buffer.byteLength, name);
    if (!m) throw new Error("expression 无效");
    const data = JSON.parse(new TextDecoder().decode(buffer));
    this.expressions.set(name, {
      motion: m,
      ownsEyes: (data.Parameters ?? []).some((p) => this.eyeIds.has(p.Id)),
    });
  }
  physics(buffer) {
    this.loadPhysics(buffer, buffer.byteLength);
  }
  pose(buffer) {
    this.loadPose(buffer, buffer.byteLength);
  }
  texture(index, texture) {
    this.getRenderer().bindTexture(index, texture);
    this.getRenderer().setIsPremultipliedAlpha(true);
  }
  start(key) {
    const item = this.motions.get(key);
    if (!item) return false;
    this.current = key;
    this._motionManager.startMotionPriority(item.motion, false, 3);
    return true;
  }
  finished() {
    return this._motionManager.isFinished();
  }
  stop() {
    this._motionManager.stopAllMotions();
    this.current = null;
  }
  expression(name) {
    if (this.expressionName === name) return;
    this._expressionManager.stopAllMotions();
    this.expressionName = name;
    const item = this.expressions.get(name);
    if (item) this._expressionManager.startMotion(item.motion, false);
  }
  set(id, value) {
    const ids = this._model.getModel().parameters.ids;
    const index = ids.indexOf(id);
    if (index < 0) return;
    this._model.setParameterValueByIndex(
      index,
      Math.max(
        this._model.getParameterMinimumValue(index),
        Math.min(this._model.getParameterMaximumValue(index), value),
      ),
    );
  }
  frame(dt, input) {
    this._model.loadParameters();
    this._motionManager.updateMotion(this._model, dt);
    this._model.saveParameters();
    const eyes =
      this.motions.get(this.current)?.ownsEyes ||
      this.expressions.get(this.expressionName)?.ownsEyes;
    if (input.blink && !eyes) this._eyeBlink.updateParameters(this._model, dt);
    this._expressionManager.updateMotion(this._model, dt);
    this.set("ParamEyeBallX", input.gaze.x * 0.75);
    this.set("ParamEyeBallY", input.gaze.y * 0.65);
    this.set("ParamAngleX", input.gaze.x * 16);
    this.set("ParamAngleY", input.gaze.y * 12);
    if (input.idle)
      this.set("ParamBreath", 0.5 + Math.sin(input.time * 1.6) * 0.12);
    if (this._physics) this._physics.evaluate(this._model, dt);
    if (this._pose) this._pose.updateParameters(this._model, dt);
    if (input.speaking || input.closeMouth) {
      const level = input.speaking ? input.level : 0;
      this.set("ParamMouthOpenY", level);
      for (let i = 0; i < this.settings.getLipSyncParameterCount(); i++)
        this.set(this.settings.getLipSyncParameterId(i).getString().s, level);
    }
    this._model.update();
    const matrix = new CubismMatrix44();
    matrix.scale(input.sx, input.sy);
    matrix.translate(input.tx, input.ty);
    matrix.multiplyByMatrix(this._modelMatrix);
    const r = this.getRenderer();
    r.setMvpMatrix(matrix);
    const target = input.target;
    const width = target?.width || this.gl.canvas.width;
    const height = target?.height || this.gl.canvas.height;
    r.setRenderState(target?.framebuffer || null, [0, 0, width, height]);
    r.drawModel(this.shaderPath);
  }
  resize(w, h) {
    this.setRenderTargetSize(w, h);
  }
  hit(x, y) {
    for (let i = 0; i < this.settings.getHitAreasCount(); i++)
      if (this.isHit(this.settings.getHitAreaId(i), x, y))
        return this.settings.getHitAreaName(i);
    return null;
  }
  opaqueTriangles() {
    if (this.dead || !this._model) return new Float32Array();
    const model = this._model, matrix = this._modelMatrix;
    // Reuse storage across samples. Topology does not change during animation.
    if (!this.hitTriangles) {
      let size = 0;
      for (let i = 0; i < model.getDrawableCount(); i++)
        size += model.getDrawableVertexIndices(i).length * 2;
      this.hitTriangles = new Float32Array(size);
    }
    let offset = 0;
    for (let i = 0; i < model.getDrawableCount(); i++) {
      if (!model.getDrawableDynamicFlagIsVisible(i) || model.getDrawableOpacity(i) < 0.1)
        continue;
      const vertices = model.getDrawableVertices(i);
      for (const index of model.getDrawableVertexIndices(i)) {
        this.hitTriangles[offset++] = matrix.transformX(vertices[index * 2]);
        this.hitTriangles[offset++] = matrix.transformY(vertices[index * 2 + 1]);
      }
    }
    return this.hitTriangles.subarray(0, offset);
  }
  opaqueBounds() {
    if (this.dead || !this._model) return new Float32Array();
    const model = this._model;
    const matrix = this._modelMatrix;
    const boxes = [];
    const count = model.getDrawableCount();
    for (let i = 0; i < count; i++) {
      if (!model.getDrawableDynamicFlagIsVisible(i) || model.getDrawableOpacity(i) < 0.1)
        continue;
      const n = model.getDrawableVertexCount(i);
      if (n < 3) continue;
      const vertices = model.getDrawableVertices(i);
      let minX = Infinity;
      let minY = Infinity;
      let maxX = -Infinity;
      let maxY = -Infinity;
      for (let j = 0; j < n; j++) {
        const x = matrix.transformX(vertices[j * 2]);
        const y = matrix.transformY(vertices[j * 2 + 1]);
        if (x < minX) minX = x;
        if (y < minY) minY = y;
        if (x > maxX) maxX = x;
        if (y > maxY) maxY = y;
      }
      if (minX <= maxX && minY <= maxY) boxes.push(minX, minY, maxX, maxY);
    }
    return Float32Array.from(boxes);
  }
  dimensions() {
    return {
      width: this._model.getCanvasWidth(),
      height: this._model.getCanvasHeight(),
    };
  }
  dispose() {
    if (this.dead) return;
    this.dead = true;
    this.stop();
    this._expressionManager.stopAllMotions();
    for (const { motion } of this.motions.values())
      ACubismMotion.delete(motion);
    for (const { motion } of this.expressions.values())
      ACubismMotion.delete(motion);
    this.motions.clear();
    this.expressions.clear();
    this.release();
    this.settings.release();
    if (!--users) {
      CubismFramework.dispose();
      CubismFramework.cleanUp();
    }
  }
}
