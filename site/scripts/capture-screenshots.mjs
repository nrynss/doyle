#!/usr/bin/env node
// Reproducible offline screenshot capture for the Doyle review documentation.
//
// 1. Builds an isolated scratch state in /tmp/doyle-screenshot-work
//    so no private user paths enter old_version or new_version strings.
// 2. Runs doyle check and diffrun offline using committed recordings.
// 3. Verifies that rendered HTML contains no private strings or reviewer tokens.
// 4. Captures 1920x1080 logged-out screenshots into site/public/media/.

import { spawn, execSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync, symlinkSync, rmSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';
import http from 'node:http';

const repoRoot = resolve(import.meta.dirname, '../..');
const workDir = '/tmp/doyle-screenshot-work';
const mediaDir = join(repoRoot, 'site/public/media');
const defaultPort = Number(process.env.DOYLE_SERVE_PORT ?? '8088');

// Locate the doyle binary.
let doyleBin = process.env.DOYLE_BIN || join(repoRoot, 'target/debug/doyle');
if (!existsSync(doyleBin)) {
  console.log('Building doyle binary...');
  execSync('cargo build --quiet', { cwd: repoRoot, stdio: 'inherit' });
}
console.log(`Using doyle binary: ${doyleBin}`);

// Locate a suitable headless browser.
function findBrowser() {
  if (process.env.CHROME_BIN && existsSync(process.env.CHROME_BIN)) {
    return { type: 'chrome', bin: process.env.CHROME_BIN };
  }
  const homeDir = process.env.HOME ?? '';
  const chromeCandidates = [
    join(homeDir, '.cache/ms-playwright/chromium-1248/chrome-linux64/chrome'),
    join(homeDir, '.cache/ms-playwright/chromium-1243/chrome-linux64/chrome'),
    join(homeDir, '.cache/ms-playwright/chromium-1234/chrome-linux64/chrome'),
  ];
  for (const candidate of chromeCandidates) {
    if (existsSync(candidate)) {
      return { type: 'chrome', bin: candidate };
    }
  }
  for (const cmd of ['google-chrome', 'chromium', 'chromium-browser']) {
    try {
      const path = execSync(`which ${cmd} 2>/dev/null`, { encoding: 'utf8' }).trim();
      if (path && existsSync(path)) {
        return { type: 'chrome', bin: path };
      }
    } catch {}
  }
  if (existsSync('/usr/bin/firefox')) {
    return { type: 'firefox', bin: '/usr/bin/firefox' };
  }
  throw new Error('No supported headless browser found (Chrome, Chromium, or Firefox)');
}

const browser = findBrowser();
console.log(`Using headless browser: ${browser.bin} (${browser.type})`);

// Prepare the scratch working directory.
if (existsSync(workDir)) {
  rmSync(workDir, { recursive: true, force: true });
}
mkdirSync(workDir, { recursive: true });
mkdirSync(mediaDir, { recursive: true });

// Symlink test fixtures and evidence into scratch working directory.
symlinkSync(join(repoRoot, 'testdata/contracts'), join(workDir, 'contracts'));
symlinkSync(join(repoRoot, 'testdata/legacy/story/v1'), join(workDir, 'v1'));
symlinkSync(join(repoRoot, 'testdata/legacy/story/v2'), join(workDir, 'v2'));
symlinkSync(join(repoRoot, 'testdata/legacy/diffrun/full-wrong-report.txt'), join(workDir, 'full-wrong-report.txt'));
symlinkSync(join(repoRoot, 'testdata/legacy/diffrun/full-record-paths.txt'), join(workDir, 'full-record-paths.txt'));
symlinkSync(join(repoRoot, 'evidence'), join(workDir, 'evidence'));

// Assemble screens-v1.json and screens-v2.json.
function screenDoc(name) {
  const file = join(repoRoot, `testdata/legacy/story/screens/${name}.json`);
  const val = JSON.parse(readFileSync(file, 'utf8'));
  return { name: val.screen, elements: val.elements };
}

const stepsData = JSON.parse(
  readFileSync(join(repoRoot, 'testdata/legacy/story/training-v1-steps.json'), 'utf8'),
).steps;

const trainingSteps = stepsData.map((s) => ({
  description: s.description,
  start_s: s.start_s,
  end_s: s.end_s,
  screen: ['T2', 'T3', 'T4'].includes(s.id) ? 'lookup' : ['T5', 'T6'].includes(s.id) ? 'decommission' : '',
}));

const screensV1 = {
  console: 'fernwhistle',
  screens: [screenDoc('lookup-v1'), screenDoc('decomm-v1')],
  steps: trainingSteps,
  recording: { name: 'training-v1', duration_s: 45.0 },
};

const screensV2 = {
  console: 'fernwhistle',
  screens: [screenDoc('lookup-v2'), screenDoc('decomm-v2')],
  steps: [],
};

writeFileSync(join(workDir, 'screens-v1.json'), JSON.stringify(screensV1, null, 2));
writeFileSync(join(workDir, 'screens-v2.json'), JSON.stringify(screensV2, null, 2));

// Assemble diffrun manifests: run-wrong.json and wrong-rewrite.json.
const diffrunManifest = {
  program: 'BILLBATCH',
  legacy_label: 'v1/BILLBATCH.cbl (GnuCOBOL 3.2)',
  rewrite_label: 'wonder-bill.rs (rustc -O)',
  inputs: 'v1/patients.dat',
  cobol_report: 'v1/report.txt',
  rewrite_report: 'full-wrong-report.txt',
  legacy_output: 'v1/report.txt',
  rewrite_output: 'full-wrong-report.txt',
  record_paths: 'full-record-paths.txt',
  trace: 'full-record-paths.txt',
  rewrite_artifacts: ['wonder-bill.rs', 'bin/billbatch'],
};

writeFileSync(join(workDir, 'run-wrong.json'), JSON.stringify(diffrunManifest, null, 2));
writeFileSync(join(workDir, 'wrong-rewrite.json'), JSON.stringify(diffrunManifest, null, 2));

// Prepare recheck replay directory with recordings.
const replayDir = join(workDir, 'replies');
mkdirSync(replayDir, { recursive: true });
const sourceReplayDir = join(repoRoot, 'testdata/replies/recheck');
for (const entry of execSync(`ls ${sourceReplayDir}/*.json`, { encoding: 'utf8' }).trim().split('\n')) {
  const name = entry.split('/').pop();
  symlinkSync(entry, join(replayDir, name));
}
// Support both version label hashes for diffrun.
if (existsSync(join(replayDir, 'c9497ec84bc43b29.json'))) {
  symlinkSync(join(replayDir, 'c9497ec84bc43b29.json'), join(replayDir, 'fd1032c1ff96fd74.json'));
}

// Assemble merged provenance.
const provMem = JSON.parse(readFileSync(join(repoRoot, 'testdata/memories/provenance.json'), 'utf8'));
const provStory = JSON.parse(readFileSync(join(repoRoot, 'testdata/legacy/story/memories/provenance.json'), 'utf8'));
const existingSessionIds = new Set(provMem.sessions.map((s) => s.id));
const mergedProvenance = {
  repository: 'clinic-ops',
  sessions: [...provMem.sessions, ...provStory.sessions.filter((s) => !existingSessionIds.has(s.id))],
  memories: [...provMem.memories, ...provStory.memories],
};

// Seed 4-flag state directory.
const stateDir = join(workDir, 'state');
mkdirSync(stateDir, { recursive: true });
writeFileSync(join(stateDir, 'provenance.json'), JSON.stringify(mergedProvenance, null, 2));

// Step 1: Cite M1 using doyle cite.
const m1Spec = {
  adapter: 'muse-code',
  memory_id: 'M1',
  contract: 'contracts/patients-v1.yaml',
  elements: ['GET /patients'],
  claim: 'GET /patients returns every patient in one response',
  recorded_by: 'seed',
  confidence: 1.0,
  source_text: 'GET /patients returns every patient in one response',
};
execSync(`${doyleBin} cite --state-dir state`, {
  cwd: workDir,
  input: JSON.stringify(m1Spec),
  stdio: ['pipe', 'inherit', 'inherit'],
});

// Append legacy COBOL, screen, and differential run citations.
const additionalCitations = [
  {
    memory: { adapter: 'muse-code', id: 'L2' },
    elements: [
      'le2_690243c98922cdcded58beed19f46cb8725f9690399b9f1c297cc465c4f21533',
      'le2_45b9463e291475e0882b9909f0f4e8762e5c39528785f7697b6023433315facc',
      'le2_aa156746e052e0c3095b931dceea8aa24c04efdee6b8331d58d5e1801cd82f1e',
      'le2_115161c1611668d048b209f67556c356ed639c6a74f05ecd39bf85c515909ce2',
      'le2_1f98795edb174bb36377a7f67f6ac8a2c369855fcad23ac9e9e1aa0d3e402914',
    ],
    claim: 'Each patient account record is 34 characters: a 6-digit account number, a 20-character name, a 7-digit balance with an implied decimal point, and a 1-character visit code',
    recorded_by: 'ops-1',
    confidence: 0.9,
    source_text: 'Each patient account record is 34 characters: a 6-digit account number, a 20-character name, a 7-digit balance with an implied decimal point, and a 1-character visit code',
  },
  {
    memory: { adapter: 'muse-code', id: 'S1' },
    elements: [
      'se2_f854905e809f6e29e01f684fb9dff8bc4bd0c8322ccd745e6f68f69ba945cf76',
      'se2_1ffe5a0803d371949f1465eff7a8eaff3d0d5d9769d8c526ceb893fa2e65599a',
      'se2_eae0f2e4c3420adbbc9065dfe97139ae1696b1fb923d51ed7d9df4e032e0190a',
    ],
    claim: 'To retire the console: open DECOMMISSION, type the clinic name in the confirmation prompt, press PF12 CONFIRM. The warning means it is permanent',
    recorded_by: 'ops-1',
    confidence: 0.9,
    source_text: 'To retire the console: open DECOMMISSION, type the clinic name in the confirmation prompt, press PF12 CONFIRM. The warning means it is permanent',
  },
  {
    memory: { adapter: 'muse-code', id: 'L1' },
    elements: [
      'le2_3422f89619e902b31f7fe202a2fc9a05701bbc7ae83f09cf366989f5e95f749e',
      'le2_115161c1611668d048b209f67556c356ed639c6a74f05ecd39bf85c515909ce2',
      'le2_af6c903a18d5e82a8429ca3d0b9c2ad6a5bdc11b876f8ae18bb2d926b47d83a3',
      'le2_1f98795edb174bb36377a7f67f6ac8a2c369855fcad23ac9e9e1aa0d3e402914',
    ],
    claim: 'BILLBATCH charges a 1.5% late fee on the whole balance, no exceptions; every account pays',
    recorded_by: 'ops-1',
    confidence: 0.9,
    source_text: 'BILLBATCH charges a 1.5% late fee on the whole balance, no exceptions; every account pays',
  },
];

for (const c of additionalCitations) {
  writeFileSync(join(stateDir, 'citations.jsonl'), JSON.stringify(c) + '\n', { flag: 'a' });
}

// Seed single-flag readme-state directory.
const readmeStateDir = join(workDir, 'readme-state');
mkdirSync(readmeStateDir, { recursive: true });
writeFileSync(join(readmeStateDir, 'provenance.json'), JSON.stringify(mergedProvenance, null, 2));
execSync(`${doyleBin} cite --state-dir readme-state`, {
  cwd: workDir,
  input: JSON.stringify(m1Spec),
  stdio: ['pipe', 'inherit', 'inherit'],
});

// Run checks in scratch directory expecting exit code 1.
function runExpectedFlag(cmd) {
  try {
    execSync(cmd, {
      cwd: workDir,
      env: { ...process.env, DOYLE_RECHECK_REPLAY_DIR: replayDir },
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    throw new Error(`Command was expected to exit with 1 but succeeded: ${cmd}`);
  } catch (err) {
    if (err.status !== 1) {
      throw new Error(`Command failed with status ${err.status} instead of 1: ${cmd}\n${err.stderr?.toString()}`);
    }
  }
}

console.log('Running 4 checks on state...');
runExpectedFlag(`${doyleBin} check contracts/patients-v1.yaml contracts/patients-v2.yaml --state-dir state`);
runExpectedFlag(`${doyleBin} check v1/BILLBATCH.cbl v2/BILLBATCH.cbl --state-dir state`);
runExpectedFlag(`${doyleBin} check screens-v1.json screens-v2.json --state-dir state`);
runExpectedFlag(`${doyleBin} diffrun v1/BILLBATCH.cbl wrong-rewrite.json --state-dir state`);

console.log('Running single check on readme-state...');
runExpectedFlag(`${doyleBin} check contracts/patients-v1.yaml contracts/patients-v2.yaml --state-dir readme-state`);

// Start doyle serve helper.
function startServe(stateSubdir, port) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(doyleBin, ['serve', '--state-dir', stateSubdir], {
      cwd: workDir,
      env: { ...process.env, DOYLE_SERVE_PORT: String(port) },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stderrOutput = '';
    child.stderr.on('data', (d) => {
      stderrOutput += d.toString();
    });
    child.on('error', reject);

    const timer = setTimeout(() => {
      reject(new Error(`doyle serve failed to respond in time:\n${stderrOutput}`));
    }, 8000);

    const interval = setInterval(() => {
      const req = http.get(`http://127.0.0.1:${port}/`, { headers: { Connection: 'close' } }, (res) => {
        if (res.statusCode === 200) {
          clearInterval(interval);
          clearTimeout(timer);
          resolvePromise(child);
        }
      });
      req.on('error', () => {});
    }, 150);
  });
}

// Fetch helper with Connection: close.
function fetchPage(url) {
  return new Promise((resolvePromise, reject) => {
    http
      .get(url, { headers: { Connection: 'close' } }, (res) => {
        let body = '';
        res.on('data', (chunk) => {
          body += chunk.toString();
        });
        res.on('end', () => resolvePromise(body));
      })
      .on('error', reject);
  });
}

// Capture screenshot helper.
function capture(url, outPath) {
  const flags =
    browser.type === 'chrome'
      ? ['--headless=new', '--no-sandbox', '--disable-gpu', '--window-size=1920,1080', `--screenshot=${outPath}`, url]
      : ['--headless', '--window-size=1920,1080', `--screenshot=${outPath}`, url];
  execSync(`"${browser.bin}" ${flags.join(' ')}`, { stdio: 'inherit' });
  const st = statSync(outPath);
  if (st.size === 0) {
    throw new Error(`Screenshot output file is empty: ${outPath}`);
  }
}

// Assert no private strings in HTML.
function assertCleanHtml(html, url) {
  const forbidden = ['/' + 'home/', process.env.USER || 'user', 'reviewer' + '='];
  for (const str of forbidden) {
    if (html.includes(str)) {
      throw new Error(`Private string "${str}" found in rendered HTML from ${url}`);
    }
  }
}

// 1. Capture single-flag README screenshot.
console.log('Capturing single-flag README review queue...');
const serveReadme = await startServe('readme-state', defaultPort);
try {
  const html = await fetchPage(`http://127.0.0.1:${defaultPort}/`);
  assertCleanHtml(html, '/');
  capture(`http://127.0.0.1:${defaultPort}/`, join(mediaDir, 'review-readme-single.png'));
} finally {
  serveReadme.kill('SIGTERM');
  await new Promise((r) => setTimeout(r, 500));
}

// 2. Capture four-flag review pages and judges overview.
console.log('Capturing 4-flag review pages and judges overview...');
const serveMain = await startServe('state', defaultPort);
try {
  const pages = [
    { path: '/', file: 'review-queue.png' },
    { path: '/flag/flag-1', file: 'flag-contract-m1.png' },
    { path: '/flag/flag-2', file: 'flag-legacy-l2.png' },
    { path: '/flag/flag-3', file: 'flag-screens-s1.png' },
    { path: '/flag/flag-4', file: 'flag-diffrun-l1.png' },
    { path: '/judges', file: 'judges-page.png' },
  ];

  for (const p of pages) {
    const url = `http://127.0.0.1:${defaultPort}${p.path}`;
    const html = await fetchPage(url);
    assertCleanHtml(html, p.path);
    console.log(`Capturing ${p.path} -> ${p.file}...`);
    capture(url, join(mediaDir, p.file));
  }
} finally {
  serveMain.kill('SIGTERM');
  await new Promise((r) => setTimeout(r, 500));
}

// Clean up scratch workdir.
rmSync(workDir, { recursive: true, force: true });
console.log('All 7 screenshots captured offline and scratch directory removed.');
