import {defineConfig} from '@farmfe/core';
import solid from 'vite-plugin-solid';
import * as path from "node:path";
import farmJsPluginPostcss from '@farmfe/js-plugin-postcss';

export default defineConfig({
  vitePlugins: [
    () => ({
      vitePlugin: solid(),
      filters: ['\\.tsx$', '\\.jsx$']
    })
  ],
  plugins: [
    farmJsPluginPostcss({})
  ],
  server: {
    port: 3000,
  },
  compilation: {
    output: {
      targetEnv: 'browser-legacy',
    },
    resolve: {
      alias: {
        '~': path.resolve(__dirname, 'src'),
      },
    },
  }
});