// Ambient environment for the plain-JS Figma plugins (code.js and
// fixture-author/code.js), so `deno task check` type-checks them against the
// real Plugin API (issue #93).
//
// The `figma`/`__html__` globals live in a module-scoped `declare global`
// block in @figma/plugin-typings' index.d.ts. A `types=` reference to the
// package resolves those globals when it sits in a `.d.ts` (here), but not
// when it sits directly in a `// @ts-check` `.js` entry — so each plugin file
// references THIS file by path instead, and the globals resolve.

/// <reference types="@figma/plugin-typings" />
