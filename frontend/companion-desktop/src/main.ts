import {mount} from 'svelte';
import App from './App.svelte';
import QuickWindowView from './features/quick/QuickWindowView.svelte';
import './app.css';

const isQuickWindow = window.location.search.includes('window=quick');

mount(isQuickWindow ? QuickWindowView : App, {
  target: document.getElementById('app')!,
});
