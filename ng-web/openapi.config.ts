import { GeneratorConfig } from 'ng-openapi';

const config: GeneratorConfig = {
  input: 'http://localhost/api-docs/openapi.json',
  output: './src/api',
  options: {
    dateType: 'Date',
    enumStyle: 'enum',
    generateEnumBasedOnDescription: true,
    generateServices: true,
    customHeaders: {
      'X-Requested-With': 'XMLHttpRequest',
      Accept: 'application/json',
    },
  },
};

export default config;
