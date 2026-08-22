'use strict';

const assert = require('node:assert/strict');
const wrapper = require('../bin/copilot-datagate.js');

assert.equal(wrapper.platformName('linux', 'x64'), 'linux-x86_64');
assert.equal(wrapper.platformName('linux', 'arm64'), 'linux-aarch64');
assert.equal(wrapper.platformName('darwin', 'x64'), 'macos-x86_64');
assert.equal(wrapper.platformName('darwin', 'arm64'), 'macos-aarch64');
assert.equal(wrapper.platformName('win32', 'x64'), 'windows-x86_64');
assert.equal(wrapper.platformName('win32', 'arm64'), 'windows-aarch64');

assert.equal(wrapper.releaseTag('0.3.0'), 'copilot-datagate-v0.3.0');
assert.equal(wrapper.releaseTag('copilot-datagate-v0.3.0'), 'copilot-datagate-v0.3.0');

assert.equal(
  wrapper.archiveName('0.3.0', 'linux', 'x64'),
  'copilot-datagate-copilot-datagate-v0.3.0-linux-x86_64.tar.gz'
);
assert.equal(
  wrapper.archiveName('0.3.0', 'win32', 'arm64'),
  'copilot-datagate-copilot-datagate-v0.3.0-windows-aarch64.zip'
);
assert.equal(
  wrapper.archiveUrl('0.3.0', 'darwin', 'arm64'),
  'https://github.com/afurlane/copilot-datagate/releases/download/copilot-datagate-v0.3.0/copilot-datagate-copilot-datagate-v0.3.0-macos-aarch64.tar.gz'
);
assert.equal(wrapper.executableName('win32'), 'copilot-datagate.exe');
assert.equal(wrapper.executableName('linux'), 'copilot-datagate');

assert.throws(() => wrapper.platformName('freebsd', 'x64'), /unsupported platform/);
assert.throws(() => wrapper.platformName('linux', 's390x'), /unsupported architecture/);

console.log('platform tests ok');
