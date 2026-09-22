/**
 * Rasterise at 2× the display, then area-average premultiplied RGBA.
 * Averaging colour as well as coverage preserves subpixel texture strokes.
 * Picking a peak-alpha sample aliases opaque details (all samples have alpha 1).
 */

const SAMPLE = 2;

const VERT = `#version 300 es
layout(location=0) in vec2 a_pos;
void main() {
  gl_Position = vec4(a_pos, 0.0, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;
uniform sampler2D u_scene;
out vec4 o;
void main() {
  ivec2 p = ivec2(gl_FragCoord.xy) * 2;
  o = (texelFetch(u_scene, p, 0)
     + texelFetch(u_scene, p + ivec2(1, 0), 0)
     + texelFetch(u_scene, p + ivec2(0, 1), 0)
     + texelFetch(u_scene, p + ivec2(1, 1), 0)) * 0.25;
}`;

export class LineResolve {
  private program: WebGLProgram | null = null;
  private vao: WebGLVertexArrayObject | null = null;
  private buffer: WebGLBuffer | null = null;
  private texture: WebGLTexture | null = null;
  private framebuffer: WebGLFramebuffer | null = null;
  width = 0;
  height = 0;
  ready = false;

  constructor(private gl: WebGL2RenderingContext) {
    const program = gl.createProgram();
    const vs = compile(gl, gl.VERTEX_SHADER, VERT);
    const fs = compile(gl, gl.FRAGMENT_SHADER, FRAG);
    if (!program || !vs || !fs) {
      if (program) gl.deleteProgram(program);
      if (vs) gl.deleteShader(vs);
      if (fs) gl.deleteShader(fs);
      return;
    }
    gl.attachShader(program, vs);
    gl.attachShader(program, fs);
    gl.linkProgram(program);
    gl.deleteShader(vs);
    gl.deleteShader(fs);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      gl.deleteProgram(program);
      return;
    }
    this.program = program;
    this.vao = gl.createVertexArray();
    this.buffer = gl.createBuffer();
    if (!this.vao || !this.buffer) {
      this.dispose();
      return;
    }
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buffer);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.bindVertexArray(null);
  }

  /** Allocates a 2× target. Returns false when the GPU cannot hold it. */
  resize(viewWidth: number, viewHeight: number): boolean {
    const gl = this.gl;
    if (!this.program) return false;
    const max = gl.getParameter(gl.MAX_TEXTURE_SIZE) as number;
    if (viewWidth < 1 || viewHeight < 1 || viewWidth * SAMPLE > max || viewHeight * SAMPLE > max) {
      this.ready = false;
      return false;
    }
    const width = viewWidth * SAMPLE;
    const height = viewHeight * SAMPLE;
    if (this.ready && this.width === width && this.height === height) return true;
    this.texture ??= gl.createTexture();
    this.framebuffer ??= gl.createFramebuffer();
    if (!this.texture || !this.framebuffer) return false;
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, width, height, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.framebuffer);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.texture, 0);
    this.ready = gl.checkFramebufferStatus(gl.FRAMEBUFFER) === gl.FRAMEBUFFER_COMPLETE;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    if (!this.ready) return false;
    this.width = width;
    this.height = height;
    return true;
  }

  target(): { framebuffer: WebGLFramebuffer; width: number; height: number } | null {
    if (!this.ready || !this.framebuffer) return null;
    return { framebuffer: this.framebuffer, width: this.width, height: this.height };
  }

  present(viewWidth: number, viewHeight: number) {
    const gl = this.gl;
    if (!this.ready || !this.program || !this.texture || !this.vao) return;
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    gl.viewport(0, 0, viewWidth, viewHeight);
    gl.disable(gl.BLEND);
    gl.disable(gl.SCISSOR_TEST);
    gl.disable(gl.DEPTH_TEST);
    gl.disable(gl.CULL_FACE);
    gl.colorMask(true, true, true, true);
    gl.useProgram(this.program);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.texture);
    gl.bindVertexArray(this.vao);
    gl.drawArrays(gl.TRIANGLES, 0, 3);
    gl.bindVertexArray(null);
  }

  dispose() {
    const gl = this.gl;
    if (this.framebuffer) gl.deleteFramebuffer(this.framebuffer);
    if (this.texture) gl.deleteTexture(this.texture);
    if (this.buffer) gl.deleteBuffer(this.buffer);
    if (this.vao) gl.deleteVertexArray(this.vao);
    if (this.program) gl.deleteProgram(this.program);
    this.ready = false;
    this.framebuffer = null;
    this.texture = null;
    this.buffer = null;
    this.vao = null;
    this.program = null;
  }
}

function compile(gl: WebGL2RenderingContext, type: number, source: string) {
  const compiled = gl.createShader(type);
  if (!compiled) return null;
  gl.shaderSource(compiled, source);
  gl.compileShader(compiled);
  if (!gl.getShaderParameter(compiled, gl.COMPILE_STATUS)) {
    gl.deleteShader(compiled);
    return null;
  }
  return compiled;
}
