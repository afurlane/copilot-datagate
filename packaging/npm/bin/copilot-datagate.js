#!/usr/bin/env node
'use strict';

const childProcess = require('node:child_process');
const fs = require('node:fs');
const https = require('node:https');
const os = require('node:os');
const path = require('node:path');

const REPOSITORY = 'afurlane/copilot-datagate';

function platformName(platform = process.platform, arch = process.arch) {
  const archName = arch === 'x64' ? 'x86_64' : arch === 'arm64' ? 'aarch64' : null;
  if (!archName) {
    throw new Error(`unsupported architecture: ${arch}`);
  }

  if (platform === 'linux') return `linux-${archName}`;
  if (platform === 'darwin') return `macos-${archName}`;
  if (platform === 'win32') return `windows-${archName}`;
  throw new Error(`unsupported platform: ${platform}`);
}

function packageVersion() {
  if (process.env.COPILOT_DATAGATE_VERSION) return process.env.COPILOT_DATAGATE_VERSION;
  const packageJson = require('../package.json');
  return packageJson.version;
}

function releaseTag(version = packageVersion()) {
  return version.startsWith('copilot-datagate-v') ? version : `copilot-datagate-v${version}`;
}

function archiveExtension(platform = process.platform) {
  return platform === 'win32' ? 'zip' : 'tar.gz';
}

function archiveName(version = packageVersion(), platform = process.platform, arch = process.arch) {
  const tag = releaseTag(version);
  return `copilot-datagate-${tag}-${platformName(platform, arch)}.${archiveExtension(platform)}`;
}

function archiveUrl(version = packageVersion(), platform = process.platform, arch = process.arch) {
  const tag = releaseTag(version);
  return `https://github.com/${REPOSITORY}/releases/download/${tag}/${archiveName(version, platform, arch)}`;
}

function cacheRoot() {
  return process.env.COPILOT_DATAGATE_CACHE || path.join(os.homedir(), '.cache', 'copilot-datagate');
}

function executableName(platform = process.platform) {
  return platform === 'win32' ? 'copilot-datagate.exe' : 'copilot-datagate';
}

function cachedBinaryPath(version = packageVersion(), platform = process.platform, arch = process.arch) {
  return path.join(cacheRoot(), releaseTag(version), platformName(platform, arch), executableName(platform));
}

function download(url, destination) {
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  return new Promise((resolve, reject) => {
    const request = https.get(url, response => {
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        response.resume();
        download(response.headers.location, destination).then(resolve, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`download failed with status ${response.statusCode}: ${url}`));
        return;
      }
      const file = fs.createWriteStream(destination);
      response.pipe(file);
      file.on('finish', () => file.close(resolve));
      file.on('error', reject);
    });
    request.on('error', reject);
  });
}

function extract(archivePath, destination) {
  fs.mkdirSync(destination, { recursive: true });
  childProcess.execFileSync('tar', ['-xf', archivePath, '-C', destination], { stdio: 'inherit' });
}

async function ensureBinary() {
  if (process.env.COPILOT_DATAGATE_BIN) return process.env.COPILOT_DATAGATE_BIN;

  const binaryPath = cachedBinaryPath();
  if (fs.existsSync(binaryPath)) return binaryPath;

  const archivePath = path.join(cacheRoot(), releaseTag(), archiveName());
  const extractDir = path.dirname(binaryPath);
  await download(archiveUrl(), archivePath);
  extract(archivePath, extractDir);

  const nestedBinary = findBinary(extractDir);
  if (!nestedBinary) {
    throw new Error(`unable to find ${executableName()} in downloaded archive`);
  }
  if (nestedBinary !== binaryPath) {
    fs.renameSync(nestedBinary, binaryPath);
  }
  if (process.platform !== 'win32') {
    fs.chmodSync(binaryPath, 0o755);
  }
  return binaryPath;
}

function findBinary(root) {
  const pending = [root];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const fullPath = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(fullPath);
      if (entry.isFile() && entry.name === executableName()) return fullPath;
    }
  }
  return null;
}

async function main() {
  const binary = await ensureBinary();
  const result = childProcess.spawnSync(binary, process.argv.slice(2), { stdio: 'inherit' });
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}

if (require.main === module) {
  main().catch(error => {
    console.error(error.message);
    process.exit(1);
  });
}

module.exports = {
  archiveName,
  archiveUrl,
  platformName,
  releaseTag,
  executableName,
};
