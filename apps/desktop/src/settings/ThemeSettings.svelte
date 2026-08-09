<script lang="ts">
  import type { Theme } from './theme';

  interface Props {
    theme: Theme;
    onthemechange: (theme: Theme) => void;
  }

  let { theme, onthemechange }: Props = $props();

  const options: Array<{ id: Theme; label: string; description: string; icon: string }> = [
    { id: 'light', label: '浅色模式', description: '明亮、清爽的浅色界面', icon: '☀' },
    { id: 'dark', label: '深色模式', description: '适合暗光环境的深色界面', icon: '☾' },
  ];
</script>

<section class="theme-settings" aria-labelledby="appearance-title">
  <div class="heading">
    <p>APPEARANCE</p>
    <h3 id="appearance-title">外观</h3>
    <span>选择 DeskAide 的界面主题。你的选择会自动保存，并在下次打开时恢复。</span>
  </div>

  <div class="theme-options" role="radiogroup" aria-label="界面主题">
    {#each options as option (option.id)}
      <button
        type="button"
        class:selected={theme === option.id}
        role="radio"
        aria-checked={theme === option.id}
        onclick={() => onthemechange(option.id)}
      >
        <span class="preview" class:light={option.id === 'light'} aria-hidden="true">
          <i class="preview-header"></i>
          <i class="preview-sidebar"></i>
          <i class="preview-content"></i>
        </span>
        <span class="option-copy">
          <strong><i>{option.icon}</i>{option.label}</strong>
          <small>{option.description}</small>
        </span>
        <span class="radio" aria-hidden="true"></span>
      </button>
    {/each}
  </div>
</section>

<style>
  .theme-settings {
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
  .theme-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }
  button {
    position: relative;
    display: grid;
    padding: 10px;
    border: 1px solid var(--theme-border);
    border-radius: 12px;
    grid-template-columns: 1fr auto;
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
    position: relative;
    display: block;
    height: 82px;
    overflow: hidden;
    grid-column: 1 / -1;
    border: 1px solid rgb(151 198 255 / 18%);
    border-radius: 8px;
    background: #101a29;
  }
  .preview.light {
    border-color: rgb(32 72 98 / 18%);
    background: #f5f8fc;
  }
  .preview i {
    position: absolute;
    display: block;
    border-radius: 3px;
    background: #24354b;
  }
  .preview.light i {
    background: #dce6ef;
  }
  .preview-header {
    top: 9px;
    right: 9px;
    left: 9px;
    height: 9px;
  }
  .preview-sidebar {
    top: 25px;
    bottom: 9px;
    left: 9px;
    width: 26%;
  }
  .preview-content {
    top: 25px;
    right: 9px;
    bottom: 9px;
    left: calc(26% + 15px);
  }
  .option-copy {
    display: grid;
    gap: 3px;
  }
  strong {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
  }
  strong i {
    color: var(--theme-accent);
    font-size: 14px;
    font-style: normal;
  }
  small {
    color: var(--theme-muted);
    font-size: 9px;
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
