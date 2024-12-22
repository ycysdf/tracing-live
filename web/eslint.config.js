import js from "@eslint/js";
import globals from "globals";
import solid from "eslint-plugin-solid/configs/typescript";
import * as tsParser from "@typescript-eslint/parser";

export default [js.configs.recommended, {languageOptions: {globals: globals.browser}}, {
  files: ["**/*.{ts,tsx}"], ...solid, languageOptions: {
    parser: tsParser, parserOptions: {
      project: "tsconfig.json",
    },
  },
},];