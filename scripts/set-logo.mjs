// Change le logo de Multitool : npm run logo -- logos/05-engrenage.svg
//
// - copie le dessin dans l'interface (ui/img/logo.svg, affiché en haut à gauche) ;
// - régénère toutes les icônes de l'application et de l'installateur (src-tauri/icons).
// Les bandeaux de l'installateur Windows sont refaits séparément (voir le README).

import { copyFileSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = process.argv[2];

if (!source) {
  console.error('Indiquez le logo à utiliser, par exemple : npm run logo -- logos/09-outils-croises.svg');
  process.exit(1);
}
const logo = resolve(process.cwd(), source);
if (!existsSync(logo) || !logo.toLowerCase().endsWith('.svg')) {
  console.error(`Fichier SVG introuvable : ${logo}`);
  process.exit(1);
}

mkdirSync(join(root, 'ui', 'img'), { recursive: true });
copyFileSync(logo, join(root, 'ui', 'img', 'logo.svg'));

// Le CLI Tauri est lancé via npx pour fonctionner aussi sous Windows (npx.cmd).
const result = spawnSync('npx', ['tauri', 'icon', logo, '--output', join(root, 'src-tauri', 'icons')], {
  cwd: root,
  stdio: 'inherit',
  shell: process.platform === 'win32',
});
if (result.status !== 0) {
  console.error('La génération des icônes a échoué.');
  process.exit(result.status ?? 1);
}

// Dossiers mobiles créés par le CLI : inutiles pour une application de bureau.
for (const dir of ['android', 'ios']) {
  rmSync(join(root, 'src-tauri', 'icons', dir), { recursive: true, force: true });
}
console.log('Logo appliqué : interface et icônes à jour.');
