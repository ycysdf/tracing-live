/* @refresh reload */
import './index.css';
import {render} from 'solid-js/web';
import {App} from "./app";
import i18next from "i18next";
import resources from "./i18n-resources.json";

i18next.init({
  ns: ['translation'],
  defaultNS: 'translation',
  load: 'all',
  supportedLngs: ['en', 'zh'],
  lng: 'en',
  fallbackLng: 'en',
  debug: true,
  resources
});

const root = document.getElementById('root');

if (import.meta.env.DEV && !(root instanceof HTMLElement)) {
  throw new Error(
    'Root element not found. Did you forget to add it to your index.html? Or maybe the id attribute got misspelled?',
  );
}

render(() => <App/>, root!);