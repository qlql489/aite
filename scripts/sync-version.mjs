import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');

const packageJsonPath = path.join(rootDir, 'package.json');
const packageLockPath = path.join(rootDir, 'package-lock.json');
const tauriConfigPath = path.join(rootDir, 'src-tauri', 'tauri.conf.json');
const cargoTomlPath = path.join(rootDir, 'src-tauri', 'Cargo.toml');
const cargoLockPath = path.join(rootDir, 'src-tauri', 'Cargo.lock');

const packageJson = JSON.parse(readFileSync(packageJsonPath, 'utf8'));
const version = packageJson.version;

if (!version || typeof version !== 'string') {
  throw new Error('package.json 缺少有效的 version 字段');
}

const updatedFiles = [];

function writeIfChanged(filePath, nextContent) {
  const currentContent = readFileSync(filePath, 'utf8');
  if (currentContent === nextContent) {
    return;
  }

  writeFileSync(filePath, nextContent);
  updatedFiles.push(path.relative(rootDir, filePath));
}

function updateJsonFile(filePath, updater) {
  const currentContent = readFileSync(filePath, 'utf8');
  const parsed = JSON.parse(currentContent);
  updater(parsed);
  writeIfChanged(filePath, `${JSON.stringify(parsed, null, 2)}\n`);
}

function replaceOrThrow(filePath, pattern, replacer, description) {
  const currentContent = readFileSync(filePath, 'utf8');
  if (!pattern.test(currentContent)) {
    throw new Error(`${description} 未匹配到，文件：${path.relative(rootDir, filePath)}`);
  }

  const nextContent = currentContent.replace(pattern, replacer);
  writeIfChanged(filePath, nextContent);
}

updateJsonFile(packageLockPath, (parsed) => {
  parsed.version = version;
  if (parsed.packages?.['']) {
    parsed.packages[''].version = version;
  }
});

updateJsonFile(tauriConfigPath, (parsed) => {
  parsed.version = '../package.json';
});

replaceOrThrow(
  cargoTomlPath,
  /(^\[package\][\s\S]*?^version = )"[^"]+"/m,
  `$1"${version}"`,
  'Cargo.toml 包版本'
);

replaceOrThrow(
  cargoLockPath,
  /(\[\[package\]\]\nname = "aite"\nversion = )"[^"]+"/,
  `$1"${version}"`,
  'Cargo.lock 项目包版本'
);

if (updatedFiles.length === 0) {
  console.log(`版本已同步，无需修改 (${version})`);
} else {
  console.log(`版本已同步到 ${version}:`);
  updatedFiles.forEach((file) => console.log(`- ${file}`));
}
