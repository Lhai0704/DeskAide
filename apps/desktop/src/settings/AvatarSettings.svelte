<script lang="ts">
  import { AVATAR_PACKS, type AvatarPackId } from '../avatar/catalog';

  interface Props {
    avatarPackId: AvatarPackId;
    onavatarchange: (packId: AvatarPackId) => void;
  }

  let { avatarPackId, onavatarchange }: Props = $props();
</script>

<section class="avatar-settings" aria-labelledby="avatar-title">
  <div class="heading">
    <p>ASSISTANT AVATAR</p>
    <h3 id="avatar-title">助手形象</h3>
    <span>选择显示在桌面的助手形象。切换后会立即生效，并在下次启动时恢复。</span>
  </div>

  <div class="avatar-options" role="radiogroup" aria-label="助手形象">
    {#each AVATAR_PACKS as pack (pack.id)}
      <button
        type="button"
        class:selected={avatarPackId === pack.id}
        role="radio"
        aria-checked={avatarPackId === pack.id}
        onclick={() => onavatarchange(pack.id)}
      >
        <span class="preview">
          <img src={`${pack.root}/idle.png`} alt="" />
        </span>
        <span class="option-copy">
          <strong>{pack.name}</strong>
          <small>{pack.description}</small>
        </span>
        <span class="radio" aria-hidden="true"></span>
      </button>
    {/each}
  </div>
</section>

<style>
  .avatar-settings {
    display: grid;
    max-width: 560px;
    gap: 18px;
  }
  .heading p {
    margin: 0 0 3px;
    color: var(--theme-accent);
    font-size: 9px;
    font-weight: 750;
    letter-spacing: 0.16em;
  }
  h3 {
    margin: 0 0 7px;
    color: var(--theme-text);
    font-size: 17px;
  }
  .heading > span {
    color: var(--theme-muted-strong);
    font-size: 11px;
    line-height: 1.5;
  }
  .avatar-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }
  button {
    position: relative;
    display: grid;
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--theme-border);
    border-radius: 12px;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 10px;
    color: var(--theme-text);
    background: var(--theme-control-bg);
    cursor: pointer;
    text-align: left;
  }
  button:hover {
    border-color: var(--theme-border-strong);
    background: var(--theme-control-hover);
  }
  button.selected {
    border-color: var(--theme-accent-border);
    box-shadow: 0 0 0 1px var(--theme-accent-soft);
  }
  .preview {
    display: grid;
    height: 132px;
    overflow: hidden;
    grid-column: 1 / -1;
    place-items: center;
    border: 1px solid var(--theme-border);
    border-radius: 9px;
    background:
      radial-gradient(circle at 50% 44%, rgb(126 226 255 / 13%), transparent 48%),
      var(--theme-input-bg);
  }
  .preview img {
    width: 124px;
    height: 124px;
    object-fit: contain;
    filter: drop-shadow(0 8px 9px rgb(7 16 29 / 25%));
  }
  .option-copy {
    display: grid;
    min-width: 0;
    gap: 3px;
  }
  strong {
    font-size: 11px;
  }
  small {
    overflow: hidden;
    color: var(--theme-muted);
    font-size: 9px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .radio {
    width: 15px;
    height: 15px;
    border: 1px solid var(--theme-border-strong);
    border-radius: 50%;
  }
  button.selected .radio {
    border: 4px solid var(--theme-accent);
  }
</style>
