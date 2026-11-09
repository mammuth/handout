import '@fontsource-variable/dm-sans';
import '@fontsource-variable/space-grotesk';
import './app.css';
import { mount } from 'svelte';
import App from './App.svelte';

export default mount(App, { target: document.getElementById('app')! });
