<script lang="ts">
  interface Props {
    src: string;
    alt: string;
  }

  let { src, alt }: Props = $props();
</script>

<video class="avatar-media" {src} aria-label={alt} autoplay muted loop playsinline draggable="false"
></video>

<style>
  .avatar-media {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
    background: transparent;
    filter: drop-shadow(0 8px 9px rgb(7 16 29 / 30%));
    transition: scale 120ms ease;
  }
</style>
