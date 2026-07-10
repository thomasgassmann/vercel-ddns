import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
    { ignores: ["dist", "src/routeTree.gen.ts"] },
    {
        files: ["**/*.{ts,tsx}"],
        extends: [js.configs.recommended, tseslint.configs.recommended],
        plugins: { "react-hooks": reactHooks },
        languageOptions: {
            ecmaVersion: 2022,
            globals: globals.browser,
        },
        rules: {
            "react-hooks/rules-of-hooks": "error",
            "react-hooks/exhaustive-deps": "warn",
        },
    },
    prettier,
);
