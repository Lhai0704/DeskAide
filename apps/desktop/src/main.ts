import { mount } from 'svelte';
import App from './App.svelte';
import { initializeTheme } from './settings/theme';
import './styles.css';

initializeTheme();

mount(App, {
  target: document.getElementById('app')!,
});
