<script lang="ts">
  interface Props {
    src: string;
    alt: string;
  }

  let { src, alt }: Props = $props();
</script>

<img class="avatar-media" {src} {alt} draggable="false" />

<style>
  .avatar-media {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
    pointer-events: none;
    filter: drop-shadow(0 8px 9px rgb(7 16 29 / 30%));
    transition: scale 120ms ease;
  }
</style>
