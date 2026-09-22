<script lang="ts">
  import { onMount } from 'svelte';
  import type { RendererInput, VrmAvatarPackManifest } from '../types';
  import { VrmRenderer } from '../vrm/model';

  interface Props {
    pack: VrmAvatarPackManifest;
    root: string;
    input: RendererInput;
    onerror: (message: string) => void;
  }
  let { pack, root, input, onerror }: Props = $props();
  let canvas: HTMLCanvasElement;
  let renderer = $state.raw<VrmRenderer>();
  $effect(() => {
    const currentInput = input;
    renderer?.update(currentInput);
  });
  onMount(() => {
    let disposed = false,
      recovered = false;
    const fail = (error: unknown) => {
      onerror(String(error));
      renderer?.dispose(false);
    };
    const start = () => {
      try {
        renderer = new VrmRenderer(canvas, pack, root, input, fail);
        const current = renderer;
        void current
          .load()
          .then(() => {
            if (!disposed && renderer === current) {
              current.pause(document.hidden);
              onerror('');
            }
          })
          .catch((error: unknown) => {
            if (
              !disposed &&
              renderer === current &&
              (error as { name?: string }).name !== 'AbortError'
            )
              fail(error);
          });
      } catch (error) {
        fail(error);
      }
    };
    const visibility = () => renderer?.pause(document.hidden);
    const lost = (event: Event) => {
      event.preventDefault();
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
