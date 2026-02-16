#!/usr/bin/env node
import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, '..');
const forwardedArgs = process.argv.slice(2);

const isWindows = process.platform === 'win32';
const command = isWindows ? 'powershell.exe' : 'bash';
const commandArgs = isWindows
    ? [
          '-NoProfile',
          '-ExecutionPolicy',
          'Bypass',
          '-File',
          path.join(scriptDir, 'clean-windows.ps1'),
          ...forwardedArgs,
      ]
    : [path.join(scriptDir, 'clean-macos.sh'), ...forwardedArgs];

const child = spawn(command, commandArgs, {
    cwd: projectRoot,
    stdio: 'inherit',
    shell: false,
});

child.on('error', (error) => {
    console.error('[clean.js] Failed to launch cleaner:', error.message);
    process.exit(1);
});

child.on('exit', (code, signal) => {
    if (signal) {
        console.error(`[clean.js] Cleaner exited with signal ${signal}`);
        process.exit(1);
    }
    process.exit(code ?? 1);
});
