<script lang="ts">
  import { onMount } from 'svelte';
  import type { Live2DAvatarPackManifest } from '../types';
  import { Live2DRenderer, type RendererInput } from '../live2d/model';
  interface Props {
    pack: Live2DAvatarPackManifest;
    root: string;
    input: RendererInput;
    onerror: (message: string) => void;
  }
  let { pack, root, input, onerror }: Props = $props();
  let canvas: HTMLCanvasElement;
  let renderer = $state.raw<Live2DRenderer>();
  $effect(() => {
    const currentInput = input;
    renderer?.update(currentInput);
  });
  onMount(() => {
    let disposed = false,
      recovered = false;
    const fail = (e: unknown) => {
      onerror(String(e));
      renderer?.dispose(false);
    };
    const start = () => {
      try {
        renderer = new Live2DRenderer(canvas, pack, root, input, fail);
        const current = renderer;
        void current
          .load()
          .then(() => {
            if (!disposed && renderer === current) {
              current.pause(document.hidden);
              onerror('');
            }
          })
          .catch((e) => {
            if (!disposed && renderer === current && e.name !== 'AbortError') fail(e);
          });
      } catch (e) {
        fail(e);
      }
    };
    const visibility = () => renderer?.pause(document.hidden);
    const lost = (e: Event) => {
      e.preventDefault();
      renderer?.pause(true);
      onerror('WebGL 上下文丢失，等待恢复');
    };
    const restored = () => {
      if (disposed || recovered) return;
      recovered = true;
      renderer?.dispose(false);
      start();
    };
    canvas.addEventListener('webglcontextlost', lost);
    canvas.addEventListener('webglcontextrestored', restored);
    document.addEventListener('visibilitychange', visibility);
    start();
    return () => {
      disposed = true;
      canvas.removeEventListener('webglcontextlost', lost);
      canvas.removeEventListener('webglcontextrestored', restored);
      document.removeEventListener('visibilitychange', visibility);
      renderer?.dispose();
      renderer = undefined;
    };
  });
</script>

<canvas bind:this={canvas} class="avatar-media" aria-label={pack.alt}></canvas>

<style>
  canvas {
    width: 100%;
    height: 100%;
    display: block;
    pointer-events: none;
  }
</style>
