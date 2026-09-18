import { copyFileSync } from 'node:fs';
const root = new URL('./', import.meta.url);
for (const name of ['index.html','style.css','bridge.js']) {
  copyFileSync(new URL(`ui/${name}`,root),new URL(`dist/${name}`,root));
}
copyFileSync(new URL('../../background1.png',root),new URL('dist/background1.png',root));
