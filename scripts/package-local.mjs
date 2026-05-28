#!/usr/bin/env node
/**
 * Phase C C.8 c103 — local release packaging.
 *
 * Bundles the canonical build artifacts (controller binary +
 * editor dist) into `dist-release/` under a version-tagged
 * subdirectory. Intentionally simple: no signing, no checksums,
 * no upload — that's a Phase D / CI concern.
 *
 * Run via:
 *   npm run package:local        # uses package.json version
 *   npm run package:local 0.3.0  # override version label
 *
 * What it produces (with version=0.3.0 on Linux):
 *
 *   dist-release/solflow-0.3.0-linux-x64/
 *     ├── solflow-controller          (release binary)
 *     ├── editor/                     (vite build output)
 *     │   ├── index.html
 *     │   ├── assets/...
 *     │   └── ...
 *     ├── README.md                   (top-level readme)
 *     ├── CHANGELOG.md                (full changelog)
 *     ├── LICENSE                     (top-level license)
 *     ├── controller/migrations/      (SQLite migrations)
 *     └── docs/                       (selected operator-relevant docs)
 *
 * The output is intentionally a directory, not a tarball — we
 * leave compression / signing / publishing to whatever wraps this
 * (CI, a release runbook, manual `tar czf`, etc).
 */
import { mkdir, copyFile, cp, readFile, writeFile, rm, stat } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { platform, arch } from 'node:os';

const __dirname = resolve(fileURLToPath(import.meta.url), '..');
const REPO_ROOT = resolve(__dirname, '..');
const OUT_BASE = join(REPO_ROOT, 'dist-release');

const VERSION_OVERRIDE = process.argv[2];

main().catch((err) => {
  console.error(`✗ ${err.message ?? err}`);
  process.exit(1);
});

async function main() {
  const version = VERSION_OVERRIDE ?? (await readJson(join(REPO_ROOT, 'package.json'))).version;
  const platformTag = platformTagFor();
  const archTag = archTagFor();
  const outDir = join(OUT_BASE, `solflow-${version}-${platformTag}-${archTag}`);

  console.log(`◆ packaging solflow ${version} for ${platformTag}-${archTag}`);
  console.log(`◆ target dir: ${outDir}`);

  // Clean target.
  await rm(outDir, { recursive: true, force: true });
  await mkdir(outDir, { recursive: true });

  // ---- 1. Build controller (release) ----
  console.log('◆ building solflow-controller (release)…');
  await runCmd('cargo', ['build', '--release', '--bin', 'solflow-controller'], REPO_ROOT);
  const binSrc = join(
    REPO_ROOT,
    'target',
    'release',
    platformTag === 'windows' ? 'solflow-controller.exe' : 'solflow-controller',
  );
  if (!existsSync(binSrc)) {
    throw new Error(`controller binary not found at ${binSrc} after build`);
  }
  const binDest = join(
    outDir,
    platformTag === 'windows' ? 'solflow-controller.exe' : 'solflow-controller',
  );
  await copyFile(binSrc, binDest);
  console.log(`  → ${binDest}`);

  // ---- 2. Build editor (vite) ----
  console.log('◆ building editor (vite)…');
  await runCmd('npm', ['run', 'build'], REPO_ROOT);
  const editorSrc = join(REPO_ROOT, 'dist');
  if (!existsSync(editorSrc)) {
    throw new Error(`editor build output not found at ${editorSrc}`);
  }
  await cp(editorSrc, join(outDir, 'editor'), { recursive: true });
  console.log(`  → ${outDir}/editor/`);

  // ---- 3. Copy migrations + docs + top-level files ----
  console.log('◆ copying migrations + docs + LICENSE…');
  await cp(
    join(REPO_ROOT, 'controller', 'migrations'),
    join(outDir, 'controller', 'migrations'),
    { recursive: true },
  );
  // Operator-relevant docs subset. We deliberately skip the full
  // sol-language tree (it's 30+ files; releases ship a curated set).
  const docFiles = [
    ['README.md', 'README.md'],
    ['CHANGELOG.md', 'CHANGELOG.md'],
    ['LICENSE', 'LICENSE'],
    ['docs/README.md', 'docs/README.md'],
    ['docs/dev/CONTROLLER_LOCAL.md', 'docs/CONTROLLER_LOCAL.md'],
    ['docs/dev/REMOTE_CONTROLLER.md', 'docs/REMOTE_CONTROLLER.md'],
    ['docs/dev/CONTROLLER_OPERATIONS.md', 'docs/CONTROLLER_OPERATIONS.md'],
    ['docs/dev/RUN_LIFECYCLE.md', 'docs/RUN_LIFECYCLE.md'],
    ['docs/dev/SCHEDULING.md', 'docs/SCHEDULING.md'],
    ['docs/dev/CONNECTORS.md', 'docs/CONNECTORS.md'],
    ['docs/dev/EVENTS.md', 'docs/EVENTS.md'],
    ['docs/user/QUICKSTART.md', 'docs/QUICKSTART.md'],
    ['docs/user/FAQ.md', 'docs/FAQ.md'],
    ['docs/user/INSTALL.md', 'docs/INSTALL.md'],
  ];
  for (const [src, dst] of docFiles) {
    const s = join(REPO_ROOT, src);
    if (!existsSync(s)) {
      console.warn(`  ! skip (missing): ${src}`);
      continue;
    }
    const d = join(outDir, dst);
    await mkdir(resolve(d, '..'), { recursive: true });
    await copyFile(s, d);
  }

  // ---- 4. Drop a release-specific README at the top of the bundle ----
  await writeFile(
    join(outDir, 'RELEASE.txt'),
    [
      `solflow ${version}`,
      `built ${new Date().toISOString()}`,
      `host: ${platformTag}-${archTag}`,
      '',
      'Contents:',
      `  ./${platformTag === 'windows' ? 'solflow-controller.exe' : 'solflow-controller'}`,
      `      release build of the controller binary`,
      `  ./editor/`,
      `      static editor; serve via any HTTP server (vite preview,`,
      `      caddy, nginx)`,
      `  ./controller/migrations/`,
      `      SQLite migrations the controller runs at startup`,
      `  ./docs/`,
      `      operator-facing docs. Start with INSTALL.md +`,
      `      CONTROLLER_LOCAL.md, then REMOTE_CONTROLLER.md when`,
      `      you're ready to expose this to a network.`,
      '',
      'Smoke test:',
      `  ./${platformTag === 'windows' ? 'solflow-controller.exe' : 'solflow-controller'}`,
      `  # in another shell:`,
      `  curl http://127.0.0.1:3939/healthz`,
      '',
      'Production-shape boot (see docs/REMOTE_CONTROLLER.md for the full',
      'recipe):',
      `  SOLFLOW_CONTROLLER_BIND=0.0.0.0:3939 \\`,
      `  SOLFLOW_CONTROLLER_TLS_CERT=/etc/solflow/cert.pem \\`,
      `  SOLFLOW_CONTROLLER_TLS_KEY=/etc/solflow/key.pem \\`,
      `  SOLFLOW_CONTROLLER_AUTH_TOKEN="$(cat /etc/solflow/token)" \\`,
      `  ./${platformTag === 'windows' ? 'solflow-controller.exe' : 'solflow-controller'}`,
      '',
    ].join('\n'),
  );

  // ---- 5. Report ----
  const sizes = await collectSizes(outDir);
  console.log(`\n✓ packaged solflow ${version}`);
  console.log(`  output: ${outDir}`);
  console.log(`  size:   ${(sizes.total / (1024 * 1024)).toFixed(1)} MiB`);
  console.log(`  files:  ${sizes.count}`);
}

function platformTagFor() {
  const p = platform();
  if (p === 'win32') return 'windows';
  if (p === 'darwin') return 'macos';
  return 'linux';
}

function archTagFor() {
  const a = arch();
  // x64 → x64; arm64 → arm64; ia32 → x86; everything else → as-is
  if (a === 'x64') return 'x64';
  if (a === 'arm64') return 'arm64';
  if (a === 'ia32') return 'x86';
  return a;
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

function runCmd(cmd, args, cwd) {
  return new Promise((resolveP, rejectP) => {
    // Inherit stdio so cargo + vite progress streams to the console.
    // shell:true on Windows so npm.cmd resolves correctly.
    const child = spawn(cmd, args, {
      cwd,
      stdio: 'inherit',
      shell: platform() === 'win32',
    });
    child.on('exit', (code) => {
      if (code === 0) resolveP();
      else rejectP(new Error(`${cmd} ${args.join(' ')} exited with code ${code}`));
    });
    child.on('error', rejectP);
  });
}

async function collectSizes(dir) {
  let total = 0;
  let count = 0;
  async function walk(d) {
    const entries = await import('node:fs/promises').then((m) => m.readdir(d, { withFileTypes: true }));
    for (const e of entries) {
      const p = join(d, e.name);
      if (e.isDirectory()) await walk(p);
      else {
        const s = await stat(p);
        total += s.size;
        count++;
      }
    }
  }
  await walk(dir);
  return { total, count };
}
