import typescript from '@rollup/plugin-typescript';
import { nodeResolve } from '@rollup/plugin-node-resolve';

export default {
  input: 'src/index.ts',
  output: {
    file: 'dist/index.js',
    format: 'es',
    sourcemap: true,
  },
  plugins: [
    nodeResolve(),
    typescript({
      tsconfig: './tsconfig.json',
      // Override tsconfig's noEmit to actually produce output.
      noEmit: false,
      declaration: false,
      sourceMap: true,
      outDir: undefined,
      rootDir: undefined,
    }),
  ],
  // Sigma FM loads the extension via `import()`, so we want ESM.
};